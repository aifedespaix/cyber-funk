//! Bout en bout (J1-02 × J1-03) : le lecteur de démos de `cf-audio`, sur une sortie nulle cadencée en temps réel,
//! alimente le thread d'analyse par le flux `SourceKind::Demo`. On vérifie que l'analyse entend la musique même en
//! sourdine, au rythme d'un pas de 512 échantillons à 48 kHz, et que la pause la rend muette (`live = false`).

mod common;

use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use cf_analysis::{AnalysisConfig, AnalysisThread};
use cf_audio::player::{NullOutput, Player, PlayerConfig};
use cf_audio::playlist::{PlayMode, Playlist, track_from_path};
use cf_audio::source::SourceKind;
use common::{Probe, analysis_config, wait_for};

// Comme `pipeline.rs` : en debug, toute allocation sous `assert_no_alloc` (pas d'analyse, poussée du rendu vers le
// flux) ferait avorter le test.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[test]
fn demo_player_feeds_the_analysis_thread_even_when_muted() {
    let music = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/music");
    let track = track_from_path(music.join("Tozan - Winning the Race.mp3"), false);
    let playlist = Playlist::new(vec![track], Some(0), 1);
    let config = PlayerConfig {
        mode: PlayMode::Loop,
        volume: 0.0,
        muted: true,
    };
    let mut player = Player::new(playlist, config, Box::new(NullOutput::new(48_000))).unwrap();

    let (probe, probe_state) = Probe::new();
    let (analysis, mut outputs) = AnalysisThread::spawn(
        probe,
        AnalysisConfig {
            initial_source: Some(SourceKind::Demo),
            ..analysis_config()
        },
    )
    .unwrap();
    analysis.control().attach(player.take_analysis_feed().unwrap());

    // En pause au départ : rien n'arrive à l'analyse.
    thread::sleep(Duration::from_millis(300));
    assert!(!outputs.features.latest().live, "lecteur en pause : source muette");

    player.play();
    wait_for("live", Duration::from_secs(3), || outputs.features.latest().live);
    let steps_before = outputs.features.latest().step;
    let start = Instant::now();
    thread::sleep(Duration::from_secs(1));
    let rate = (outputs.features.latest().step - steps_before) as f64 / start.elapsed().as_secs_f64();
    assert!(
        (rate - 93.75).abs() < 10.0,
        "pas d'analyse au rythme de la lecture : {rate:.1} pas/s"
    );
    assert!(
        probe_state.rms() > 0.01,
        "l'analyse entend la musique malgré la sourdine : rms {}",
        probe_state.rms()
    );

    player.pause();
    wait_for("live = false en pause", Duration::from_secs(2), || {
        !outputs.features.latest().live
    });
    let state = player.state();
    assert_eq!(state.underruns, 0);
    assert_eq!(state.analysis_dropped, 0);
}
