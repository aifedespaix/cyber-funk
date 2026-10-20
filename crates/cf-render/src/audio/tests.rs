//! Tests du pont dans une `App` sans rendu (`MinimalPlugins`), sans carte son : capture synthétique et sortie nulle.
//!
//! Les threads tournent en temps réel : chaque test attend de vrais kicks, il dure quelques secondes.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use cf_audio::capture::{DeviceSelector, InputFormat, SampleType, Signal, SyntheticBus, SyntheticDevice};

use super::*;
use crate::CorePlugin;

const MIC: &str = "Micro simulé";
const LINE: &str = "Interface ligne simulée";

#[derive(Resource, Default)]
struct Seen {
    kicks: Vec<Kick>,
    sections: Vec<SectionChanged>,
}

fn collect(mut seen: ResMut<Seen>, mut kicks: MessageReader<Kick>, mut sections: MessageReader<SectionChanged>) {
    seen.kicks.extend(kicks.read().copied());
    seen.sections.extend(sections.read().copied());
}

fn bus() -> SyntheticBus {
    let bus = SyntheticBus::new();
    let format = InputFormat {
        sample_rate: 48_000,
        channels: 2,
        sample: SampleType::F32,
    };
    let kicks = Signal::Kicks {
        bpm: 128.0,
        amplitude: 0.8,
    };
    bus.add(SyntheticDevice::new(MIC, format, kicks));
    bus.add(SyntheticDevice::new(LINE, format, kicks));
    bus
}

fn music_dir() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/music"))
}

fn app(config: AudioConfig) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin, CfAudioPlugin::new(config)))
        .init_resource::<Seen>()
        .add_systems(Update, collect.after(CfSet::Input));
    app
}

fn synthetic_config(source: AudioSource) -> AudioConfig {
    AudioConfig {
        input: InputBackendChoice::Synthetic(bus()),
        output: OutputChoice::Null { speed: 1.0 },
        music_dir: music_dir(),
        initial_source: source,
        mic_device: DeviceSelector::Name(MIC.into()),
        line_device: DeviceSelector::Name(LINE.into()),
        ..AudioConfig::default()
    }
}

/// Tourne à ~200 i/s jusqu'à ce que `done` soit vrai ou que `timeout` passe. Renvoie le plus long `update`.
fn run_until(app: &mut App, timeout: Duration, mut done: impl FnMut(&App) -> bool) -> Duration {
    let start = Instant::now();
    let mut worst = Duration::ZERO;
    while start.elapsed() < timeout && !done(app) {
        let before = Instant::now();
        app.update();
        worst = worst.max(before.elapsed());
        std::thread::sleep(Duration::from_millis(5));
    }
    worst
}

fn kicks(app: &App) -> usize {
    app.world().resource::<Seen>().kicks.len()
}

fn send(app: &mut App, command: AudioCommand) {
    app.world_mut().write_message(command);
}

#[test]
fn le_micro_publie_des_kicks_et_des_features_vivantes() {
    let mut app = app(synthetic_config(AudioSource::Microphone));
    run_until(&mut app, Duration::from_secs(20), |a| {
        kicks(a) >= 4 && a.world().resource::<AudioFeatures>().bpm.is_some()
    });
    assert!(kicks(&app) >= 4, "kicks vus : {}", kicks(&app));
    let features = app.world().resource::<AudioFeatures>();
    assert!(features.live, "{features:?}");
    assert!(features.count >= 4);
    assert!(
        features.bpm.is_some_and(|bpm| (bpm - 128.0).abs() < 4.0),
        "{:?}",
        features.bpm
    );
    let status = app.world().resource::<AudioStatus>();
    assert_eq!(status.source, AudioSource::Microphone);
    assert!(status.devices.iter().any(|d| d == MIC), "{:?}", status.devices);
}

#[test]
fn la_demo_publie_des_kicks_et_avance_dans_la_piste() {
    let mut app = app(synthetic_config(AudioSource::Demo));
    run_until(&mut app, Duration::from_secs(25), |a| kicks(a) >= 4);
    assert!(kicks(&app) >= 4, "kicks vus : {}", kicks(&app));
    let status = app.world().resource::<AudioStatus>();
    let demo = status.demo.as_ref().expect("lecteur de démo");
    assert!(demo.playing && demo.position_s > 0.5, "{demo:?}");
    assert!(!status.tracks.is_empty());
}

#[test]
fn changer_de_source_a_chaud_ne_fige_pas_l_image() {
    let mut app = app(synthetic_config(AudioSource::Demo));
    run_until(&mut app, Duration::from_secs(25), |a| {
        a.world().resource::<AudioFeatures>().live
    });
    assert!(app.world().resource::<AudioFeatures>().live, "la démo ne démarre pas");

    // 60 bascules en rafale, plusieurs par image : micro, ligne, démo, et changements de carte pendant l'écoute.
    let cycle = [AudioSource::Microphone, AudioSource::Line, AudioSource::Demo];
    let mut worst = Duration::ZERO;
    for i in 0..60_usize {
        send(&mut app, AudioCommand::SelectSource(cycle[i % 3]));
        if i % 7 == 0 {
            send(
                &mut app,
                AudioCommand::SelectDevice {
                    source: AudioSource::Line,
                    device: DeviceSelector::Name(if i % 2 == 0 { MIC } else { LINE }.into()),
                },
            );
        }
        let before = Instant::now();
        app.update();
        worst = worst.max(before.elapsed());
        std::thread::sleep(Duration::from_millis(8));
    }
    assert!(
        worst < Duration::from_millis(100),
        "une image a duré {worst:?} pendant les bascules"
    );

    // Et on retombe sur ses pieds : la démo repart et rend du signal.
    send(&mut app, AudioCommand::SelectSource(AudioSource::Microphone));
    let before = kicks(&app);
    let worst_after = run_until(&mut app, Duration::from_secs(12), |a| kicks(a) >= before + 2);
    assert!(kicks(&app) >= before + 2, "plus de kick après les bascules");
    assert!(worst_after < Duration::from_millis(100), "{worst_after:?}");
    assert!(app.world().resource::<AudioFeatures>().live);
}

#[test]
fn l_entree_ligne_publie_des_kicks_sans_gain_automatique() {
    // Le gain coupé pour la ligne est testé dans `cf-analysis` ; ici, la chaîne complète tient sans lui.
    let mut app = app(synthetic_config(AudioSource::Line));
    run_until(&mut app, Duration::from_secs(12), |a| kicks(a) >= 2);
    assert!(kicks(&app) >= 2, "la ligne ne produit pas de kick");
    let status = app.world().resource::<AudioStatus>();
    assert_eq!(status.source, AudioSource::Line);
}

#[test]
fn sans_lecteur_la_source_demo_se_replie_sur_le_micro() {
    let config = AudioConfig {
        output: OutputChoice::Disabled,
        ..synthetic_config(AudioSource::Demo)
    };
    let mut app = app(config);
    app.update();
    let status = app.world().resource::<AudioStatus>();
    assert_eq!(status.source, AudioSource::Microphone);
    assert!(status.demo.is_none());
    assert!(
        status.notes.iter().any(|n| n.contains("Démo indisponible")),
        "{:?}",
        status.notes
    );
}

#[test]
fn le_decalage_visuel_retarde_la_publication() {
    let mut delayed = synthetic_config(AudioSource::Microphone);
    delayed.latency_ms = 300.0;
    let mut app = app(delayed);
    // Pendant les 250 premières ms, l'historique ne remonte pas assez loin : rien n'est encore montré.
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(250) {
        app.update();
        assert_eq!(app.world().resource::<AudioFeatures>().step, 0, "montré avant le délai");
        std::thread::sleep(Duration::from_millis(5));
    }
    run_until(&mut app, Duration::from_secs(5), |a| {
        a.world().resource::<AudioFeatures>().step > 0
    });
    assert!(app.world().resource::<AudioFeatures>().step > 0);
}
