//! Tests des outils MCP de pilotage (attach, `send_action`, state...).

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
