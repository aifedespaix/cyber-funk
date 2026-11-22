//! Tests de `launch`, `stop` et `log` avec un FAUX binaire : le binaire de test lui-même, relancé sur
//! `fake_app_entry` avec `CF_FAKE_MODE` (aucun autre exécutable à compiler).

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

use cf_probe_proto::{ProbeReply, encode_line};
use rmcp::model::CallToolResult;

use super::*;
use crate::logs::{self, LogArgs};
use crate::process;

/// Point d'entrée du faux binaire : sans `CF_FAKE_MODE` (cas normal de la suite) ce test ne fait rien.
#[test]
fn fake_app_entry() {
    let Ok(mode) = std::env::var("CF_FAKE_MODE") else {
        return;
    };
    if mode == "exit1" {
        for index in 0..60 {
            eprintln!("ligne d'erreur {index}");
        }
        eprintln!("PANIC simulé au démarrage");
        std::process::exit(1);
    }
    let dir = PathBuf::from(std::env::var("CF_FAKE_DIR").unwrap());
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::fs::write(
        dir.join("probe.json"),
        format!(r#"{{"port":{port},"pid":{}}}"#, std::process::id()),
    )
    .unwrap();
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                (&stream).write_all(encode_line(&ProbeReply::Pong).as_bytes()).unwrap();
                line.clear();
            }
        });
    }
}

fn fake_command(mode: &str, dir: &std::path::Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "mcp::lifecycle_tests::fake_app_entry",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("CF_FAKE_MODE", mode)
        .env("CF_FAKE_DIR", dir);
    command
}

fn text(result: &CallToolResult) -> String {
    serde_json::to_string(&result.content).unwrap()
}

fn server_in(dir: &tempfile::TempDir) -> ProbeServer {
    ProbeServer::new(Some(dir.path().to_path_buf())).with_read_timeout(Duration::from_secs(5))
}

impl LogParams {
    fn sessions() -> Self {
        Self {
            subcommand: "sessions".into(),
            args: None,
            limit: None,
            session: None,
        }
    }
}

impl ProbeServer {
    fn child_id_for_test(&self) -> Option<u32> {
        lock_child(&self.child).as_ref().map(|launched| launched.child.id())
    }

    /// Simule un plantage : tue l'enfant sans passer par `stop`.
    fn kill_child_for_test(&self) {
        if let Some(launched) = lock_child(&self.child).as_mut() {
            launched.child.kill().unwrap();
            launched.child.wait().unwrap();
        }
    }
}

const STARTUP: Duration = Duration::from_secs(30);

#[tokio::test]
async fn launch_attend_probe_json_puis_se_connecte() {
    let dir = tempfile::tempdir().unwrap();
    // Un probe.json périmé (d'un lancement précédent) ne doit jamais être pris pour le bon.
    std::fs::write(dir.path().join("probe.json"), r#"{"port":1,"pid":1}"#).unwrap();
    let server = server_in(&dir);
    let result = server.launch_with(fake_command("serve", dir.path()), STARTUP).await;
    assert_ne!(result.is_error, Some(true), "{}", text(&result));
    let state = server.state().await;
    assert_ne!(state.is_error, Some(true), "{}", text(&state));
    assert_ne!(server.stop().await.is_error, Some(true));
}

#[tokio::test]
async fn launch_echoue_proprement_si_l_app_meurt_au_demarrage() {
    let dir = tempfile::tempdir().unwrap();
    let server = server_in(&dir);
    let result = server.launch_with(fake_command("exit1", dir.path()), STARTUP).await;
    assert_eq!(result.is_error, Some(true));
    let message = text(&result);
    assert!(message.contains("PANIC simulé"), "{message}");
    // Seule la fin de la sortie est conservée (40 lignes au plus).
    assert!(!message.contains("ligne d'erreur 0\n"), "{message}");
}

#[tokio::test]
async fn un_appel_apres_la_mort_de_l_app_rend_une_erreur_claire_et_log_reste_utilisable() {
    let dir = tempfile::tempdir().unwrap();
    let server = server_in(&dir);
    let launched = server.launch_with(fake_command("serve", dir.path()), STARTUP).await;
    assert_ne!(launched.is_error, Some(true), "{}", text(&launched));
    server.kill_child_for_test();
    let result = tokio::time::timeout(Duration::from_secs(10), server.state())
        .await
        .expect("l'appel ne doit pas bloquer");
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("arrêtée"), "{}", text(&result));
    let log = server.log(Parameters(LogParams::sessions())).await;
    assert!(
        !text(&log).contains("arrêtée"),
        "log doit rester utilisable : {}",
        text(&log)
    );
}

#[tokio::test]
async fn stop_tue_le_processus_enfant() {
    let dir = tempfile::tempdir().unwrap();
    let server = server_in(&dir);
    server.launch_with(fake_command("serve", dir.path()), STARTUP).await;
    let pid = server.child_id_for_test().expect("un enfant tourne");
    assert_ne!(server.stop().await.is_error, Some(true));
    assert!(server.child_id_for_test().is_none());
    assert!(!process::is_alive(pid), "le processus {pid} vit encore");
    // Après stop, plus de connexion.
    assert_eq!(server.state().await.is_error, Some(true));
}

#[test]
fn log_transmet_la_sous_commande_et_borne_la_sortie() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake_dbg.py");
    std::fs::write(
        &script,
        "import sys\nprint('ARGV ' + ' '.join(sys.argv[1:]))\nfor i in range(100): print('l', i)\n",
    )
    .unwrap();
    let args = LogArgs {
        subcommand: "timeline".into(),
        args: vec!["--around".into(), "83500".into()],
        limit: 5,
        root: None,
        session: None,
    };
    let out = logs::run_with(&logs::python_program(), &script, &args);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 5, "{out}");
    assert!(lines[0].contains("ARGV timeline --around 83500 --limit 5"), "{out}");
    // Sous-commande hors liste blanche : refusée sans rien exécuter.
    let bad = LogArgs {
        subcommand: "rm".into(),
        ..args
    };
    assert!(logs::run_with(&logs::python_program(), &script, &bad).contains("sous-commande"));
}
