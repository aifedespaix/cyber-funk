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

use crate::client::{ProbeClient, READ_TIMEOUT};
use crate::logs::{self, LogArgs};
use crate::params::{CaptureParams, LaunchParams, LogParams, MeasureSyncParams, SendActionParams, StepParams};
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

pub(crate) fn ok(text: impl Into<String>) -> CallToolResult {
    CallToolResult::success(vec![ContentBlock::text(text)])
}

pub(crate) fn fail(text: impl Into<String>) -> CallToolResult {
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

    /// Lance `command` (remplace une application déjà lancée), attend sa sonde et s'y connecte. Le verrou de l'enfant
    /// n'est tenu que pour le ranger et le sonder : `stop` peut interrompre l'attente. `note` est ajouté à la réponse.
    async fn launch_with(&self, command: Command, startup_timeout: Duration, note: String) -> CallToolResult {
        let Some(dir) = self.state_dir.clone() else {
            return fail("pas de dossier de données sur ce système : probe.json introuvable");
        };
        let (client, child, read_timeout) = (Arc::clone(&self.client), Arc::clone(&self.child), self.read_timeout);
        let outcome = tokio::task::spawn_blocking(move || {
            let pid = {
                let mut slot = lock_child(&child);
                if let Some(mut old) = slot.take() {
                    old.kill();
                }
                *lock(&client) = None;
                let launched = Launched::spawn(command, &dir)?;
                let pid = launched.child.id();
                *slot = Some(launched);
                pid
            };
            let port = process::wait_for_probe(&child, pid, &dir, startup_timeout)?;
            let connection = match ProbeClient::connect_with(port, read_timeout) {
                Ok(connection) => connection,
                Err(error) => {
                    let ours = lock_child(&child).take_if(|launched| launched.child.id() == pid);
                    drop(ours);
                    return Err(error.context(format!("sonde annoncée sur le port {port} mais injoignable")));
                }
            };
            let slot = lock_child(&child);
            if slot.as_ref().map(|launched| launched.child.id()) != Some(pid) {
                anyhow::bail!("lancement interrompu : `stop` ou un nouveau `launch` a retiré l'application");
            }
            *lock(&client) = Some(connection);
            Ok(format!("application lancée (pid {pid}, port {port}) ; {note}"))
        })
        .await;
        match outcome {
            Ok(Ok(summary)) => ok(summary),
            Ok(Err(error)) => fail(format!("{error:#}")),
            Err(error) => fail(format!("tâche interrompue : {error}")),
        }
    }

    /// Envoie une requête sur la connexion ouverte ; rend le texte d'erreur à montrer à l'appelant.
    pub(crate) async fn request(&self, request: ProbeRequest) -> Result<ProbeReply, String> {
        let client = Arc::clone(&self.client);
        let child = Arc::clone(&self.child);
        let outcome = tokio::task::spawn_blocking(move || {
            // Le verrou de l'enfant n'est jamais tenu pendant l'appel (il peut durer longtemps) : `stop` reste libre.
            if let Some(why) = dead_child_message(&mut lock_child(&child)) {
                *lock(&client) = None;
                return Err(why);
            }
            let called = {
                let mut guard = lock(&client);
                let Some(connection) = guard.as_mut() else {
                    return Err("pas de connexion : appeler d'abord l'outil `launch` (ou `attach`)".to_owned());
                };
                connection.call(&request).inspect_err(|_| {
                    // Une réponse tardive fausserait la requête suivante : on referme, il faut se rattacher.
                    *guard = None;
                })
            };
            called.map_err(|error| {
                // Si l'application vient de mourir en pleine requête, c'est la vraie explication.
                dead_child_message(&mut lock_child(&child))
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
        let resolved = match process::build_command(&options) {
            Ok(resolved) => resolved,
            Err(error) => return fail(format!("{error:#}")),
        };
        let timeout = params
            .startup_timeout_s
            .map_or(self.startup_timeout, Duration::from_secs)
            .min(process::MAX_STARTUP_TIMEOUT);
        let note = resolved.describe();
        self.launch_with(resolved.command, timeout, note).await
    }

    /// Arrête l'application lancée par `launch`.
    #[tool(description = "Tue l'application lancée par `launch` et ferme la connexion. `log` reste utilisable.")]
    pub async fn stop(&self) -> CallToolResult {
        let (client, child) = (Arc::clone(&self.client), Arc::clone(&self.child));
        let outcome = tokio::task::spawn_blocking(move || {
            // D'abord l'enfant : sa mort fait échouer un appel en cours (qui tient le verrou de la connexion).
            let taken = lock_child(&child).take();
            let had_child = taken.is_some();
            drop(taken);
            *lock(&client) = None;
            had_child
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

    /// Mesure la synchronisation musique/image.
    #[tool(
        description = concat!(
            "Mesure la latence interne détection du kick → commande d'image (borne basse : ne voit ni l'émissif du ",
            "monde, ni le GPU, ni l'affichage). Joue la piste `track` (ou l'actuelle) `seconds` secondes de ",
            "simulation (1 à 300) et rend un rapport JSON (kicks, retard moyen, gigue, pente du retard, délai de ",
            "confiance, verdict, notes). ATTENTION : avec un pas de temps fixe (`step`) découplé de l'horloge audio, ",
            "le retard n'est pas comparable à un retard musique → image : le verdict est alors Unreliable. ",
            "NoData sans kick mesuré."
        )
    )]
    pub async fn measure_sync(&self, Parameters(params): Parameters<MeasureSyncParams>) -> CallToolResult {
        self.run_measure_sync(params).await
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

#[cfg(test)]
#[path = "mcp_sync_tests.rs"]
mod sync_tests;
