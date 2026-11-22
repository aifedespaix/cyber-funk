//! Outils MCP : un par fonction. Toute erreur devient un résultat d'outil `isError`, jamais un plantage.

use std::path::PathBuf;
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

/// Serveur MCP : tient la connexion à l'application une fois `attach` fait.
#[derive(Debug, Clone)]
pub struct ProbeServer {
    client: Arc<Mutex<Option<ProbeClient>>>,
    state_dir: Option<PathBuf>,
    read_timeout: Duration,
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
            state_dir,
            read_timeout: READ_TIMEOUT,
        }
    }

    /// Envoie une requête sur la connexion ouverte ; rend le texte d'erreur à montrer à l'appelant.
    async fn request(&self, request: ProbeRequest) -> Result<ProbeReply, String> {
        let client = Arc::clone(&self.client);
        let outcome = tokio::task::spawn_blocking(move || {
            let mut guard = lock(&client);
            let Some(connection) = guard.as_mut() else {
                return Err("pas de connexion : appeler d'abord l'outil `attach`".to_owned());
            };
            connection.call(&request).map_err(|error| {
                // Une réponse tardive fausserait la requête suivante : on referme, il faut se rattacher.
                *guard = None;
                format!("{error:#} ; connexion fermée, rappeler `attach`")
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
mod tests {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::net::TcpListener;
    use std::thread::JoinHandle;

    use cf_control::Action;
    use cf_probe_proto::{ProbeReply, ProbeRequest, StateSnapshot, decode_line, encode_line};
    use rmcp::handler::server::wrapper::Parameters;

    use super::*;

    /// Fausse application : répond `reply` à la première requête reçue et la rend au test.
    fn fake_app(reply: ProbeReply) -> (u16, JoinHandle<ProbeRequest>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(stream.try_clone().unwrap())
                .read_line(&mut line)
                .unwrap();
            let request: ProbeRequest = decode_line(&line).unwrap();
            (&stream).write_all(encode_line(&reply).as_bytes()).unwrap();
            request
        });
        (port, handle)
    }

    fn server_attached_to(port: u16) -> (ProbeServer, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("probe.json"), format!(r#"{{"port":{port},"pid":1}}"#)).unwrap();
        (ProbeServer::new(Some(dir.path().to_path_buf())), dir)
    }

    fn text(result: &CallToolResult) -> String {
        serde_json::to_string(&result.content).unwrap()
    }

    #[tokio::test]
    async fn send_action_transmet_l_action_et_rend_ack() {
        let (port, app) = fake_app(ProbeReply::Ack);
        let (server, _dir) = server_attached_to(port);
        assert_ne!(server.attach().await.is_error, Some(true));
        let action = serde_json::to_value(Action::Go).unwrap();
        let result = server.send_action(Parameters(SendActionParams { action })).await;
        assert_ne!(result.is_error, Some(true), "{}", text(&result));
        assert_eq!(app.join().unwrap(), ProbeRequest::SendAction(Action::Go));
    }

    #[tokio::test]
    async fn state_rend_le_snapshot() {
        let snapshot = StateSnapshot {
            sim_time_s: 1.5,
            phase: "Show".into(),
            cue: Some("intro".into()),
            shot: None,
            beat_phase: 0.25,
            bar_phase: 0.5,
            grid_beat: 3.0,
            grid_trust: 0.5,
            kick_count: 2,
            kick_now: false,
            fps: 60.0,
        };
        let (port, app) = fake_app(ProbeReply::State(snapshot));
        let (server, _dir) = server_attached_to(port);
        server.attach().await;
        let result = server.state().await;
        assert_ne!(result.is_error, Some(true));
        assert!(text(&result).contains("intro"));
        assert_eq!(app.join().unwrap(), ProbeRequest::State);
    }

    #[tokio::test]
    async fn un_outil_sans_attach_dit_d_attacher_d_abord() {
        let server = ProbeServer::new(None);
        let result = server.state().await;
        assert_eq!(result.is_error, Some(true));
        assert!(text(&result).contains("attach"));
    }

    #[tokio::test]
    async fn une_action_json_invalide_est_une_erreur_d_outil_pas_un_plantage() {
        let (port, _app) = fake_app(ProbeReply::Ack);
        let (server, _dir) = server_attached_to(port);
        server.attach().await;
        let action = serde_json::json!({ "NExistePas": 3 });
        let result = server.send_action(Parameters(SendActionParams { action })).await;
        assert_eq!(result.is_error, Some(true));
    }

    /// Ce que fait la fausse application d'une connexion.
    enum Behavior {
        /// Lit une requête puis la ferme sans répondre.
        CloseAfterRead,
        /// Lit une requête et reste muette plus longtemps que le délai de lecture.
        Silent,
        /// Lit une requête et répond.
        Reply(ProbeReply),
    }

    /// Fausse application qui sert une connexion par comportement, dans l'ordre.
    fn fake_app_script(script: Vec<Behavior>) -> u16 {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for behavior in script {
                let Ok((stream, _)) = listener.accept() else { return };
                let mut line = String::new();
                BufReader::new(stream.try_clone().unwrap())
                    .read_line(&mut line)
                    .unwrap();
                match behavior {
                    Behavior::CloseAfterRead => {}
                    Behavior::Silent => std::thread::sleep(Duration::from_millis(800)),
                    Behavior::Reply(reply) => (&stream).write_all(encode_line(&reply).as_bytes()).unwrap(),
                }
            }
        });
        port
    }

    fn write_port(dir: &tempfile::TempDir, port: u16) {
        std::fs::write(dir.path().join("probe.json"), format!(r#"{{"port":{port},"pid":1}}"#)).unwrap();
    }

    fn short_server(dir: &tempfile::TempDir) -> ProbeServer {
        ProbeServer::new(Some(dir.path().to_path_buf())).with_read_timeout(Duration::from_millis(150))
    }

    /// Après une chute : le second appel dit de rappeler `attach`, et un `attach` réussi remet le serveur d'aplomb.
    async fn assert_recovers(server: &ProbeServer, dir: &tempfile::TempDir) {
        let again = server.state().await;
        assert_eq!(again.is_error, Some(true));
        assert!(text(&again).contains("attach"), "{}", text(&again));
        let port = fake_app_script(vec![Behavior::Reply(ProbeReply::Pong)]);
        write_port(dir, port);
        assert_ne!(server.attach().await.is_error, Some(true));
        let result = server.state().await;
        assert_ne!(result.is_error, Some(true), "{}", text(&result));
    }

    #[tokio::test]
    async fn une_connexion_fermee_en_pleine_requete_est_une_erreur_d_outil() {
        let dir = tempfile::tempdir().unwrap();
        write_port(&dir, fake_app_script(vec![Behavior::CloseAfterRead]));
        let server = short_server(&dir);
        server.attach().await;
        let result = server.state().await;
        assert_eq!(result.is_error, Some(true));
        assert!(text(&result).contains("fermé"), "{}", text(&result));
        assert_recovers(&server, &dir).await;
    }

    #[tokio::test]
    async fn un_delai_de_lecture_depasse_est_une_erreur_d_outil() {
        let dir = tempfile::tempdir().unwrap();
        write_port(&dir, fake_app_script(vec![Behavior::Silent]));
        let server = short_server(&dir);
        server.attach().await;
        let result = server.state().await;
        assert_eq!(result.is_error, Some(true));
        assert!(text(&result).contains("délai"), "{}", text(&result));
        assert_recovers(&server, &dir).await;
    }

    #[tokio::test]
    async fn attach_sur_un_port_perime_est_une_erreur_d_outil_et_le_serveur_reste_utilisable() {
        let dir = tempfile::tempdir().unwrap();
        let stale = TcpListener::bind(("127.0.0.1", 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        write_port(&dir, stale);
        let server = short_server(&dir);
        let result = server.attach().await;
        assert_eq!(result.is_error, Some(true));
        assert!(text(&result).contains("lancée"), "{}", text(&result));
        assert_recovers(&server, &dir).await;
    }
}
