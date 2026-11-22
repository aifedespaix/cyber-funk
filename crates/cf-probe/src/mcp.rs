//! Outils MCP : un par fonction. Toute erreur devient un résultat d'outil `isError`, jamais un plantage.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cf_control::Action;
use cf_probe_proto::{ProbeReply, ProbeRequest};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::client::{ProbeClient, READ_TIMEOUT};
use crate::logs::{self, LogArgs};
use crate::process::{self, DEFAULT_STARTUP_TIMEOUT, LaunchOptions, Launched};

/// Nom du fichier d'état écrit par l'application (voir `cyber-funk::probe`).
const STATE_FILE: &str = "probe.json";
/// Nom du dossier de l'application dans le dossier de données (miroir de `cyber-funk::logging`).
const APP_DIR: &str = "cyber-funk";

/// Dossier où l'application écrit `probe.json` : `<dossier de données>/cyber-funk/logs`.
/// Même logique que `logging::default_log_dir` : `cf-probe` ne dépend ni de Bevy ni du binaire.
#[must_use]
pub fn default_state_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|dir| dir.join(APP_DIR).join("logs"))
}

/// Paramètres de `send_action`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SendActionParams {
    /// Action au format JSON de `cf_control::Action` (serde par défaut : `"Go"`, `{"Cue": 3}`...).
    pub action: serde_json::Value,
}

/// Paramètres de `capture`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct CaptureParams {
    /// Nom du fichier image, sans dossier.
    pub name: String,
}

/// Paramètres de `step`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StepParams {
    /// Nombre d'images à avancer : de 1 à 1200 (0 est refusé, au-delà l'application plafonne).
    pub frames: u32,
}

/// Paramètres de `launch`.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct LaunchParams {
    /// Show à jouer (`--show`), chemin d'un fichier RON.
    pub show: Option<String>,
    /// Épingle le pas de temps des `step` (`CF_PROBE_STEP_S`).
    pub fixed_step: Option<bool>,
    /// Rendu logiciel (variable wgpu `WGPU_FORCE_FALLBACK_ADAPTER=1`).
    pub software_render: Option<bool>,
    /// Arguments supplémentaires de l'application.
    pub extra_args: Option<Vec<String>>,
    /// Délai de démarrage en secondes (120 par défaut : la première compilation est longue).
    pub startup_timeout_s: Option<u64>,
}

/// Paramètres de `log`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct LogParams {
    /// Sous-commande de dbg.py : crash, summary, timeline, find, spans, check ou sessions.
    pub subcommand: String,
    /// Arguments de la sous-commande (ex. `--around 83500 --window 3000`, ou une référence `slot1/cue2`).
    pub args: Option<Vec<String>>,
    /// Nombre de lignes au plus (40 par défaut, 400 au maximum).
    pub limit: Option<usize>,
    /// Session voulue (la dernière par défaut).
    pub session: Option<String>,
}

/// Serveur MCP : tient la connexion à l'application une fois `attach` ou `launch` fait.
#[derive(Debug, Clone)]
pub struct ProbeServer {
    client: Arc<Mutex<Option<ProbeClient>>>,
    /// Application lancée par `launch` (absente après `stop`, ou une fois sa mort signalée).
    child: Arc<Mutex<Option<Launched>>>,
    state_dir: Option<PathBuf>,
    read_timeout: Duration,
    startup_timeout: Duration,
}

fn ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

fn fail(text: impl Into<String>) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(text)])
}

/// Lit le port dans `probe.json`.
fn read_port(dir: &std::path::Path) -> Result<u16, String> {
    let path = dir.join(STATE_FILE);
    let content = std::fs::read_to_string(&path).map_err(|error| {
        format!(
            "{} illisible ({error}) : l'application tourne-t-elle avec --probe ?",
            path.display()
        )
    })?;
    serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| value.get("port")?.as_u64())
        .and_then(|port| u16::try_from(port).ok())
        .ok_or_else(|| format!("{} ne contient pas de port valide", path.display()))
}

/// Prend le verrou même s'il est empoisonné : la connexion y est soit absente soit entière, jamais à moitié écrite.
fn lock(client: &Mutex<Option<ProbeClient>>) -> MutexGuard<'_, Option<ProbeClient>> {
    client.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Prend le verrou de l'enfant, même empoisonné (l'état est un simple `Option`).
fn lock_child(child: &Mutex<Option<Launched>>) -> MutexGuard<'_, Option<Launched>> {
    child.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Si l'application lancée par `launch` est morte : l'oublie et rend le message à montrer (code de sortie, stderr).
fn dead_child_message(child: &mut Option<Launched>) -> Option<String> {
    let status = child.as_mut()?.exited()?;
    let launched = child.take()?;
    Some(format!(
        "l'application s'est arrêtée ({status}) ; rappeler `launch`. Fin de stderr :\n{}",
        launched.stderr_tail()
    ))
}

impl ProbeServer {
    /// Délai de lecture des requêtes courtes (les tests le raccourcissent).
    #[cfg(test)]
    fn with_read_timeout(mut self, read_timeout: Duration) -> Self {
        self.read_timeout = read_timeout;
        self
    }

    /// `state_dir` : dossier où chercher `probe.json` ; `None` quand le système n'a pas de dossier de données.
    #[must_use]
    pub fn new(state_dir: Option<PathBuf>) -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            state_dir,
            read_timeout: READ_TIMEOUT,
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
        }
    }

    /// Lance `command` (remplace une application déjà lancée), attend sa sonde et s'y connecte.
    async fn launch_with(&self, command: Command, startup_timeout: Duration) -> CallToolResult {
        let Some(dir) = self.state_dir.clone() else {
            return fail("pas de dossier de données sur ce système : probe.json introuvable");
        };
        let (client, child, read_timeout) = (Arc::clone(&self.client), Arc::clone(&self.child), self.read_timeout);
        let outcome = tokio::task::spawn_blocking(move || {
            let mut slot = lock_child(&child);
            if let Some(old) = slot.as_mut() {
                old.kill();
            }
            *slot = None;
            *lock(&client) = None;
            let mut launched = process::launch_command(command, &dir, startup_timeout)?;
            match ProbeClient::connect_with(launched.port, read_timeout) {
                Ok(connection) => {
                    *lock(&client) = Some(connection);
                    let summary = format!(
                        "application lancée (pid {}, port {})",
                        launched.child.id(),
                        launched.port
                    );
                    *slot = Some(launched);
                    Ok(summary)
                }
                Err(error) => {
                    launched.kill();
                    Err(error.context(format!("sonde annoncée sur le port {} mais injoignable", launched.port)))
                }
            }
        })
        .await;
        match outcome {
            Ok(Ok(summary)) => ok(summary),
            Ok(Err(error)) => fail(format!("{error:#}")),
            Err(error) => fail(format!("tâche interrompue : {error}")),
        }
    }

    /// Envoie une requête sur la connexion ouverte ; rend le texte d'erreur à montrer à l'appelant.
    async fn request(&self, request: ProbeRequest) -> Result<ProbeReply, String> {
        let client = Arc::clone(&self.client);
        let child = Arc::clone(&self.child);
        let outcome = tokio::task::spawn_blocking(move || {
            // Ordre des verrous : enfant puis connexion, partout.
            let mut child = lock_child(&child);
            if let Some(why) = dead_child_message(&mut child) {
                *lock(&client) = None;
                return Err(why);
            }
            let mut guard = lock(&client);
            let Some(connection) = guard.as_mut() else {
                return Err("pas de connexion : appeler d'abord l'outil `attach`".to_owned());
            };
            connection.call(&request).map_err(|error| {
                // Une réponse tardive fausserait la requête suivante : on referme, il faut se rattacher.
                *guard = None;
                // Si l'application vient de mourir en pleine requête, c'est la vraie explication.
                dead_child_message(&mut child)
                    .unwrap_or_else(|| format!("{error:#} ; connexion fermée, rappeler `attach`"))
            })
        })
        .await;
        outcome.unwrap_or_else(|error| Err(format!("tâche interrompue : {error}")))
    }

    /// Transforme la réponse de l'application en résultat d'outil.
    async fn reply_as_result(&self, request: ProbeRequest) -> CallToolResult {
        match self.request(request).await {
            Ok(ProbeReply::Ack) => ok("ok"),
            Ok(ProbeReply::Pong) => ok("pong"),
            Ok(ProbeReply::Error(why)) => fail(format!("refusé par l'application : {why}")),
            Ok(ProbeReply::State(snapshot)) => {
                ok(serde_json::to_string_pretty(&snapshot).unwrap_or_else(|error| error.to_string()))
            }
            Ok(ProbeReply::Captured { path }) => ok(format!("capture demandée : {path}")),
            Ok(ProbeReply::Trace(trace)) => ok(format!("trace de {} échantillons", trace.samples.len())),
            Err(why) => fail(why),
        }
    }
}

#[tool_router]
impl ProbeServer {
    /// Se connecte à l'application lancée avec `--probe`.
    #[tool(
        description = "Se connecte à l'application cyber-funk lancée avec --probe (lit probe.json). \
                          À appeler avant tout autre outil."
    )]
    pub async fn attach(&self) -> CallToolResult {
        let Some(dir) = self.state_dir.clone() else {
            return fail("pas de dossier de données sur ce système : probe.json introuvable");
        };
        let port = match read_port(&dir) {
            Ok(port) => port,
            Err(why) => return fail(why),
        };
        let client = Arc::clone(&self.client);
        let read_timeout = self.read_timeout;
        let connected = tokio::task::spawn_blocking(move || {
            let connection = ProbeClient::connect_with(port, read_timeout)?;
            *lock(&client) = Some(connection);
            Ok::<(), anyhow::Error>(())
        })
        .await;
        match connected {
            Ok(Ok(())) => ok(format!("connecté à l'application (port {port})")),
            Ok(Err(error)) => fail(format!(
                "{error:#} : l'application est-elle encore lancée ? (probe.json peut dater d'un lancement précédent)"
            )),
            Err(error) => fail(format!("tâche interrompue : {error}")),
        }
    }

    /// Lance l'application avec la sonde et s'y connecte.
    #[tool(
        description = "Lance l'application cyber-funk avec --probe (binaire déjà compilé, sinon cargo run), attend \
                          sa sonde et s'y connecte : remplace attach. Rend l'erreur avec la fin de stderr si elle \
                          meurt au démarrage."
    )]
    pub async fn launch(&self, Parameters(params): Parameters<LaunchParams>) -> CallToolResult {
        let options = LaunchOptions {
            show: params.show.map(PathBuf::from),
            fixed_step: params.fixed_step.unwrap_or(false),
            software_render: params.software_render.unwrap_or(false),
            extra_args: params.extra_args.unwrap_or_default(),
        };
        let timeout = params
            .startup_timeout_s
            .map_or(self.startup_timeout, Duration::from_secs);
        self.launch_with(process::build_command(&options), timeout).await
    }

    /// Arrête l'application lancée par `launch`.
    #[tool(description = "Tue l'application lancée par `launch` et ferme la connexion. `log` reste utilisable.")]
    pub async fn stop(&self) -> CallToolResult {
        let (client, child) = (Arc::clone(&self.client), Arc::clone(&self.child));
        let outcome = tokio::task::spawn_blocking(move || {
            *lock(&client) = None;
            lock_child(&child).take().map(|mut launched| launched.kill()).is_some()
        })
        .await;
        match outcome {
            Ok(true) => ok("application arrêtée"),
            Ok(false) => ok("aucune application lancée par `launch` ; connexion fermée"),
            Err(error) => fail(format!("tâche interrompue : {error}")),
        }
    }

    /// Lit les journaux de session via dbg.py.
    #[tool(
        description = "Lit les journaux de session (tools/debuglog/dbg.py) : crash, summary, timeline, find, spans, \
                          check, sessions. Fonctionne même si l'application est morte."
    )]
    pub async fn log(&self, Parameters(params): Parameters<LogParams>) -> CallToolResult {
        let args = LogArgs {
            subcommand: params.subcommand,
            args: params.args.unwrap_or_default(),
            limit: params.limit.unwrap_or(logs::DEFAULT_LIMIT),
            root: self.state_dir.as_ref().map(|dir| dir.display().to_string()),
            session: params.session,
        };
        match tokio::task::spawn_blocking(move || logs::run(&args)).await {
            Ok(text) => ok(text),
            Err(error) => fail(format!("tâche interrompue : {error}")),
        }
    }

    /// Injecte une action comme si l'opérateur l'avait produite.
    #[tool(description = "Injecte une Action (JSON de cf_control::Action, ex. \"Go\") dans l'application.")]
    pub async fn send_action(&self, Parameters(params): Parameters<SendActionParams>) -> CallToolResult {
        match serde_json::from_value::<Action>(params.action) {
            Ok(action) => self.reply_as_result(ProbeRequest::SendAction(action)).await,
            Err(error) => fail(format!("action JSON invalide : {error}")),
        }
    }

    /// Instantané de l'état.
    #[tool(description = "Rend l'état du spectacle et du rythme (phase, cue, plan, phases du beat, kicks, fps).")]
    pub async fn state(&self) -> CallToolResult {
        self.reply_as_result(ProbeRequest::State).await
    }

    /// Capture de l'image du projecteur.
    #[tool(
        description = "Capture l'image du projecteur sous ce nom. Le fichier est écrit quelques images APRÈS la \
                          réponse : l'attendre en sondant le chemin rendu, ou faire d'abord un court `step`."
    )]
    pub async fn capture(&self, Parameters(params): Parameters<CaptureParams>) -> CallToolResult {
        self.reply_as_result(ProbeRequest::Capture { name: params.name }).await
    }

    /// Avance la simulation.
    #[tool(description = "Avance la simulation de N images (1 à 1200 ; 0 est refusé, au-delà l'application plafonne).")]
    pub async fn step(&self, Parameters(params): Parameters<StepParams>) -> CallToolResult {
        self.reply_as_result(ProbeRequest::Step { frames: params.frames }).await
    }
}

// Le code généré par `tool_handler` déclare `async` des méthodes sans `.await`.
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler]
impl ServerHandler for ProbeServer {}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "mcp_lifecycle_tests.rs"]
mod lifecycle_tests;
