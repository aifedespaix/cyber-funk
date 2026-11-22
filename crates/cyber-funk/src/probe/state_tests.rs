//! Tests de `state`, `capture` et `step` : une `App` minimale, requêtes déposées directement dans le canal (pas de
//! réseau : le déroulé image par image doit être exact).

#![allow(clippy::panic)] // un test qui reçoit la mauvaise réponse doit échouer bruyamment

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::view::screenshot::Screenshot;
use bevy::time::TimeUpdateStrategy;
use cf_probe_proto::{ProbeReply, ProbeRequest};
use cf_render::audio::FilteredSignals;

use super::bridge::ProbeInbox;
use super::capture::CaptureDir;
use super::server::Pending;

const STEP_S: f32 = 0.05;

fn app_with_channel() -> (App, Sender<Pending>) {
    let (tx, rx) = mpsc::channel();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_message::<cf_render::ControlAction>()
        .insert_resource(ProbeInbox::new(rx));
    super::install_systems(&mut app);
    (app, tx)
}

fn ask(tx: &Sender<Pending>, request: ProbeRequest) -> Receiver<ProbeReply> {
    let (reply, answer) = mpsc::channel();
    tx.send(Pending {
        request,
        deadline: Instant::now() + Duration::from_secs(60),
        reply,
    })
    .unwrap();
    answer
}

#[test]
fn l_instantane_reflete_les_signaux_filtres() {
    let (mut app, tx) = app_with_channel();
    let mut signals = FilteredSignals::default();
    signals.grid_trust = 0.7;
    signals.kick_count = 3;
    app.insert_resource(signals);
    let answer = ask(&tx, ProbeRequest::State);
    app.update();
    let ProbeReply::State(state) = answer.try_recv().unwrap() else {
        panic!("réponse inattendue");
    };
    assert!((state.grid_trust - 0.7).abs() < 1e-6, "{state:?}");
    assert_eq!(state.kick_count, 3);
    assert_eq!(state.phase, "Running", "sans séquenceur, le show joue");
    assert_eq!(state.cue, None);
    assert!(state.fps.abs() < f32::EPSILON, "sans diagnostics : 0");
}

#[test]
fn step_avance_de_n_images_au_pas_fixe() {
    let (mut app, tx) = app_with_channel();
    app.update(); // la première image du temps a un delta nul
    let answer = ask(&tx, ProbeRequest::Step { frames: 10 });
    app.update(); // l'ordre est pris ici : le pas fixe vaut à partir de l'image suivante
    assert_eq!(answer.try_recv(), Err(TryRecvError::Empty));
    let start = app.world().resource::<Time<Virtual>>().elapsed_secs_f64();
    for frame in 1..=10 {
        assert_eq!(answer.try_recv(), Err(TryRecvError::Empty), "trop tôt, image {frame}");
        app.update();
    }
    assert_eq!(answer.try_recv(), Ok(ProbeReply::Ack));
    let advanced = app.world().resource::<Time<Virtual>>().elapsed_secs_f64() - start;
    assert!(
        (advanced - 10.0 * f64::from(STEP_S)).abs() < 1e-6,
        "avancé de {advanced} s"
    );
    // Le temps est rendu au rythme réel une fois le pas fini.
    assert!(matches!(
        app.world().resource::<TimeUpdateStrategy>(),
        TimeUpdateStrategy::Automatic
    ));
}

#[test]
fn capture_refuse_un_nom_avec_chemin() {
    let (mut app, tx) = app_with_channel();
    let answer = ask(&tx, ProbeRequest::Capture { name: "../x".into() });
    app.update();
    assert!(matches!(answer.try_recv(), Ok(ProbeReply::Error(_))));
    let count = app.world_mut().query::<&Screenshot>().iter(app.world()).count();
    assert_eq!(count, 0);
}

#[test]
fn capture_ecrit_dans_captures_sonde() {
    let (mut app, tx) = app_with_channel();
    let answer = ask(&tx, ProbeRequest::Capture { name: "a".into() });
    app.update();
    assert_eq!(
        answer.try_recv(),
        Ok(ProbeReply::Captured {
            path: "captures/probe/a.png".into()
        })
    );
    let count = app.world_mut().query::<&Screenshot>().iter(app.world()).count();
    assert_eq!(count, 1, "la capture est demandée une fois");
}

#[test]
fn capture_nettoie_le_nom_et_respecte_le_dossier_configure() {
    let (mut app, tx) = app_with_channel();
    app.insert_resource(CaptureDir("sortie".into()));
    let answer = ask(&tx, ProbeRequest::Capture { name: "a b.c".into() });
    app.update();
    assert_eq!(
        answer.try_recv(),
        Ok(ProbeReply::Captured {
            path: "sortie/a_b_c.png".into()
        })
    );
}

fn strategy_is_automatic(app: &App) -> bool {
    matches!(
        app.world().resource::<TimeUpdateStrategy>(),
        TimeUpdateStrategy::Automatic
    )
}

fn expect_error(answer: &Receiver<ProbeReply>) {
    assert!(
        matches!(answer.try_recv(), Ok(ProbeReply::Error(_))),
        "une erreur était attendue"
    );
}

#[test]
fn un_second_step_est_rejete_et_le_premier_se_termine() {
    let (mut app, tx) = app_with_channel();
    app.update();
    let first = ask(&tx, ProbeRequest::Step { frames: 3 });
    app.update();
    let second = ask(&tx, ProbeRequest::Step { frames: 5 });
    app.update();
    expect_error(&second);
    assert_eq!(first.try_recv(), Err(TryRecvError::Empty));
    app.update();
    app.update();
    assert_eq!(first.try_recv(), Ok(ProbeReply::Ack), "le premier pas va à son terme");
    assert!(strategy_is_automatic(&app));
}

#[test]
fn step_zero_et_step_enorme_sont_refuses_sans_toucher_au_temps() {
    let (mut app, tx) = app_with_channel();
    for frames in [0, super::capture::MAX_STEP_FRAMES + 1, u32::MAX] {
        let answer = ask(&tx, ProbeRequest::Step { frames });
        app.update();
        expect_error(&answer);
        assert!(strategy_is_automatic(&app), "frames = {frames}");
    }
    // Le plafond lui-même est accepté.
    let answer = ask(
        &tx,
        ProbeRequest::Step {
            frames: super::capture::MAX_STEP_FRAMES,
        },
    );
    app.update();
    assert_eq!(answer.try_recv(), Err(TryRecvError::Empty));
    assert!(!strategy_is_automatic(&app));
}

#[test]
fn un_step_perime_est_jete_sans_avancer_le_temps() {
    let (mut app, tx) = app_with_channel();
    app.update();
    let (reply, answer) = mpsc::channel();
    tx.send(Pending {
        request: ProbeRequest::Step { frames: 4 },
        deadline: Instant::now().checked_sub(Duration::from_secs(1)).unwrap(),
        reply,
    })
    .unwrap();
    app.update();
    assert!(strategy_is_automatic(&app));
    assert!(answer.try_recv().is_err(), "aucune réponse pour une requête jetée");
}

#[test]
fn un_client_parti_pendant_un_step_ne_bloque_pas_le_temps() {
    let (mut app, tx) = app_with_channel();
    app.update();
    let answer = ask(&tx, ProbeRequest::Step { frames: 2 });
    app.update();
    assert!(!strategy_is_automatic(&app));
    drop(answer);
    app.update();
    app.update();
    assert!(
        strategy_is_automatic(&app),
        "la stratégie est restaurée malgré l'absence de destinataire"
    );
}

#[test]
fn le_delai_d_un_step_croit_avec_les_images_et_est_plafonne() {
    use super::server::timeout_for;
    let base = Duration::from_secs(2);
    assert_eq!(timeout_for(&ProbeRequest::Ping, base), base);
    assert_eq!(
        timeout_for(&ProbeRequest::Step { frames: 10 }, base),
        Duration::from_millis(4500)
    );
    assert_eq!(
        timeout_for(&ProbeRequest::Step { frames: u32::MAX }, base),
        Duration::from_secs(300)
    );
    let long = Duration::from_secs(600);
    assert_eq!(
        timeout_for(&ProbeRequest::Step { frames: 100 }, long),
        long,
        "base au-dessus du plafond"
    );
}

#[test]
fn les_noms_reserves_de_windows_sont_refuses() {
    for name in ["CON", "nul", "Com1", "LPT9"] {
        let (mut app, tx) = app_with_channel();
        let answer = ask(&tx, ProbeRequest::Capture { name: name.into() });
        app.update();
        expect_error(&answer);
    }
}
