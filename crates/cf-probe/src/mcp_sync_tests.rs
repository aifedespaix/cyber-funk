//! Tests de `measure_sync` contre une fausse application qui joue le script : elle répond à chaque requête, avance son
//! temps de simulation à chaque `Step` et rend une trace synthétique à `StopSyncRecording`.

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use cf_control::{Action, Switch};
use cf_probe_proto::{
    MAX_SYNC_SAMPLES, ProbeReply, ProbeRequest, StateSnapshot, SyncSample, SyncTrace, decode_line, encode_line,
};
use rmcp::handler::server::wrapper::Parameters;

use super::*;
use crate::params::MeasureSyncParams;

const STEP_S: f64 = 0.05;

fn text(result: &CallToolResult) -> String {
    serde_json::to_string(&result.content).unwrap()
}

/// Trace synthétique : un kick toutes les 0,5 s, une réponse lumineuse en triangle `delay_s` plus tard.
fn synthetic_trace(seconds: f64, kicks: bool, delay_s: f64) -> SyncTrace {
    let frames = (seconds / 0.01) as usize;
    let samples = (0..frames)
        .map(|f| {
            let t_s = f as f64 * 0.01;
            let since_kick = t_s % 0.5;
            let luminance = if kicks {
                (1.0 - (since_kick - delay_s).abs() / 0.05).max(0.0)
            } else {
                0.0
            };
            SyncSample {
                t_s,
                wall_s: t_s,
                kick_now: kicks && f % 50 == 0,
                beat_phase: 0.0,
                grid_trust: 1.0,
                luminance: luminance as f32,
            }
        })
        .collect();
    SyncTrace { samples }
}

/// Ce que la fausse application a vu et comment elle se comporte.
struct Script {
    seen: Arc<Mutex<Vec<ProbeRequest>>>,
    stepped: Arc<Mutex<u64>>,
}

/// Lance la fausse application. `trace` : ce qu'elle rend à `StopSyncRecording` ; `fail_step` : refuse les `Step` ;
/// `track_ok` : accepte `SelectTrack`.
fn fake_app(trace: SyncTrace, fail_step: bool, track_ok: bool) -> (u16, Script) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let stepped = Arc::new(Mutex::new(0_u64));
    let (seen_thread, stepped_thread) = (Arc::clone(&seen), Arc::clone(&stepped));
    std::thread::spawn(move || {
        let Ok((stream, _)) = listener.accept() else { return };
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                return;
            }
            let request: ProbeRequest = decode_line(&line).unwrap();
            seen_thread.lock().unwrap().push(request.clone());
            let reply = match request {
                ProbeRequest::State => ProbeReply::State(StateSnapshot {
                    sim_time_s: *stepped_thread.lock().unwrap() as f64 * STEP_S,
                    phase: "Running".into(),
                    cue: None,
                    shot: None,
                    beat_phase: 0.0,
                    bar_phase: 0.0,
                    grid_beat: 0.0,
                    grid_trust: 1.0,
                    kick_count: 0,
                    kick_now: false,
                    fps: 60.0,
                }),
                ProbeRequest::Step { .. } if fail_step => ProbeReply::Error("pas de temps non pilotable".into()),
                ProbeRequest::Step { frames } => {
                    *stepped_thread.lock().unwrap() += u64::from(frames);
                    ProbeReply::Ack
                }
                ProbeRequest::SelectTrack { name } if !track_ok => {
                    ProbeReply::Error(format!("piste inconnue : {name}"))
                }
                ProbeRequest::StopSyncRecording => ProbeReply::Trace(trace.clone()),
                _ => ProbeReply::Ack,
            };
            if (&stream).write_all(encode_line(&reply).as_bytes()).is_err() {
                return;
            }
        }
    });
    (port, Script { seen, stepped })
}

async fn attached(port: u16) -> (ProbeServer, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("probe.json"), format!(r#"{{"port":{port},"pid":1}}"#)).unwrap();
    let server = ProbeServer::new(Some(dir.path().to_path_buf()));
    assert_ne!(server.attach().await.is_error, Some(true));
    (server, dir)
}

fn body(result: &CallToolResult) -> serde_json::Value {
    let content = serde_json::to_value(&result.content).unwrap();
    let text = content[0]["text"].as_str().unwrap().to_owned();
    serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
}

fn requests(script: &Script) -> Vec<ProbeRequest> {
    script.seen.lock().unwrap().clone()
}

fn params(track: Option<&str>, seconds: u32) -> Parameters<MeasureSyncParams> {
    Parameters(MeasureSyncParams {
        track: track.map(str::to_owned),
        seconds,
    })
}

#[tokio::test]
async fn measure_sync_deroule_la_mesure_et_rend_le_rapport() {
    let (port, script) = fake_app(synthetic_trace(10.0, true, 0.03), false, true);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(Some("Chase"), 10)).await;
    assert_ne!(result.is_error, Some(true), "{}", text(&result));
    let report = body(&result);
    assert_eq!(report["verdict"], "Ok", "{report}");
    let delay = report["mean_delay_ms"].as_f64().unwrap();
    assert!((delay - 30.0).abs() <= 6.0, "retard : {delay}");
    assert!(report["kicks"].as_u64().unwrap() >= 18);
    assert!(
        report["thresholds"]["max_delay_ms"].is_number(),
        "les seuils utilisés sont rendus"
    );
    // L'ordre : piste, lecture, enregistrement, pas, arrêt de l'enregistrement.
    let seen = requests(&script);
    assert_eq!(seen[0], ProbeRequest::SelectTrack { name: "Chase".into() });
    assert_eq!(seen[1], ProbeRequest::SendAction(Action::Playback(Switch::On)));
    assert!(seen.contains(&ProbeRequest::StartSyncRecording));
    assert_eq!(seen.last(), Some(&ProbeRequest::StopSyncRecording));
    let start = seen
        .iter()
        .position(|r| *r == ProbeRequest::StartSyncRecording)
        .unwrap();
    let first_step = seen
        .iter()
        .position(|r| matches!(r, ProbeRequest::Step { .. }))
        .unwrap();
    assert!(start < first_step, "on enregistre avant d'avancer");
    // Le temps de simulation a couvert la durée demandée, par pas de 1200 images au plus.
    assert!(*script.stepped.lock().unwrap() as f64 * STEP_S >= 10.0);
    for request in &seen {
        if let ProbeRequest::Step { frames } = request {
            assert!((1..=cf_probe_proto::MAX_STEP_FRAMES).contains(frames), "{frames}");
        }
    }
}

#[tokio::test]
async fn sans_piste_demandee_aucun_choix_de_piste_n_est_envoye() {
    let (port, script) = fake_app(synthetic_trace(2.0, true, 0.03), false, true);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(None, 2)).await;
    assert_ne!(result.is_error, Some(true), "{}", text(&result));
    assert!(
        !requests(&script)
            .iter()
            .any(|r| matches!(r, ProbeRequest::SelectTrack { .. }))
    );
}

#[tokio::test]
async fn le_silence_donne_pas_de_donnees_jamais_un_retard_nul() {
    let (port, _script) = fake_app(synthetic_trace(10.0, false, 0.0), false, true);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(None, 10)).await;
    assert_ne!(
        result.is_error,
        Some(true),
        "NoData est un résultat, pas une panne : {}",
        text(&result)
    );
    let report = body(&result);
    assert_eq!(report["verdict"], "NoData");
    assert_eq!(report["kicks"], 0);
    assert!(report["mean_delay_ms"].is_null(), "{report}");
    assert!(
        report["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("aucun kick")),
        "la raison probable est dite : {report}"
    );
}

#[tokio::test]
async fn une_duree_demesuree_est_ramenee_au_plafond_avec_un_message() {
    let (port, script) = fake_app(synthetic_trace(10.0, true, 0.03), false, true);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(None, u32::MAX)).await;
    assert_ne!(result.is_error, Some(true), "{}", text(&result));
    let report = body(&result);
    let notes = report["notes"].as_array().unwrap();
    assert!(
        notes.iter().any(|n| n.as_str().unwrap().contains("ramenée à 300 s")),
        "le client sait qu'on a écrêté : {report}"
    );
    let simulated = *script.stepped.lock().unwrap() as f64 * STEP_S;
    assert!((300.0..320.0).contains(&simulated), "{simulated} s simulées");
}

#[tokio::test]
async fn zero_seconde_est_refuse_sans_rien_envoyer() {
    let (port, script) = fake_app(SyncTrace::default(), false, true);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(None, 0)).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("seconds"), "{}", text(&result));
    assert_eq!(requests(&script), vec![]);
}

#[tokio::test]
async fn une_piste_refusee_arrete_tout_avant_l_enregistrement() {
    let (port, script) = fake_app(SyncTrace::default(), false, false);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(Some("Inconnue"), 5)).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("piste inconnue"), "{}", text(&result));
    assert!(!requests(&script).contains(&ProbeRequest::StartSyncRecording));
}

#[tokio::test]
async fn un_pas_refuse_arrete_quand_meme_l_enregistrement() {
    let (port, script) = fake_app(SyncTrace::default(), true, true);
    let (server, _dir) = attached(port).await;
    let result = server.measure_sync(params(None, 5)).await;
    assert_eq!(result.is_error, Some(true));
    assert!(text(&result).contains("pas de temps"), "{}", text(&result));
    // Sinon l'enregistreur resterait actif et la mesure suivante serait refusée (« déjà en cours »).
    assert_eq!(requests(&script).last(), Some(&ProbeRequest::StopSyncRecording));
}

#[tokio::test]
async fn une_trace_pleine_est_signalee_comme_tronquee() {
    let full = SyncTrace {
        samples: vec![
            SyncSample {
                t_s: 0.0,
                wall_s: 0.0,
                kick_now: false,
                beat_phase: 0.0,
                grid_trust: 0.0,
                luminance: 0.0,
            };
            MAX_SYNC_SAMPLES
        ],
    };
    let (port, _script) = fake_app(full, false, true);
    let (server, _dir) = attached(port).await;
    let report = body(&server.measure_sync(params(None, 5)).await);
    assert!(
        report["notes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n.as_str().unwrap().contains("tronquée")),
        "{report}"
    );
}
