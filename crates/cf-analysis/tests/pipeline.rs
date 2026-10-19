//! Test d'intégration sans matériel (critère J1-03) : source synthétique injectée par le backend de capture →
//! callback → ring → thread d'analyse → `AudioFeatures` publiées au rythme d'un pas de 512 échantillons à 48 kHz.
//!
//! Le binaire installe `AllocDisabler` : le callback simulé et chaque pas d'analyse tournent sous `assert_no_alloc`,
//! donc la moindre allocation sur ces chemins ferait avorter le test (voir `alloc_trap.rs` pour la preuve que le
//! piège est armé).

mod common;

use std::f32::consts::FRAC_1_SQRT_2;
use std::thread;
use std::time::{Duration, Instant};

use cf_analysis::{AudioEventKind, LATENCY_TARGET};
use cf_audio::capture::{CaptureConfig, CaptureState, SampleType, SyntheticDevice};
use cf_audio::feed::{HOP, HOP_SECONDS, feed};
use cf_audio::source::SourceKind;
use common::{format, rig, sine, wait_for};

// `AllocDisabler` n'existe qu'en debug (en release, `assert_no_alloc` est compilé en appel direct).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Pas par seconde attendus : 48 000 / 512.
const HOPS_PER_SECOND: f64 = 48_000.0 / HOP as f64;

/// Mesure le rythme de publication pendant `window` et vérifie les invariants d'horloge.
fn measure_rate(r: &mut common::Rig, window: Duration) -> f64 {
    let start_step = r.features().step;
    let start = Instant::now();
    thread::sleep(window);
    let f = r.features();
    let elapsed = start.elapsed().as_secs_f64();
    // Horloge audio = compteur de pas, jamais l'horloge murale.
    assert!((f.time_s - f.step as f64 * HOP_SECONDS).abs() < 1e-9);
    (f.step - start_step) as f64 / elapsed
}

#[test]
fn synthetic_capture_publishes_features_at_hop_rate() {
    let device = SyntheticDevice::new("Synth USB", format(48_000, 2, SampleType::I16), sine(440.0, 0.5));
    let mut r = rig(device, CaptureConfig::default()).unwrap();
    let became_live = wait_for("live", Duration::from_secs(3), || r.outputs.features.latest().live);
    let live_event = r.next_event(AudioEventKind::LiveChanged { live: true }, Duration::from_secs(1));
    assert!(live_event.is_some(), "événement LiveChanged(true) attendu");
    thread::sleep(Duration::from_millis(200));

    let pitch_mark = r.probe.mark();
    let rate = measure_rate(&mut r, Duration::from_millis(1500));
    let freq = r.probe.frequency_since(pitch_mark);
    let f = r.features();
    let stats = r.analysis.stats();
    eprintln!(
        "48 kHz i16 stéréo : live après {became_live:?}, {rate:.2} pas/s (attendu {HOPS_PER_SECOND:.2}), \
         {freq:.1} Hz, loud {:.4}, latence callback → publication moy {:?} max {:?} sur {} pas",
        f.loud, stats.latency_mean, stats.latency_max, stats.latency_samples
    );
    assert!(
        (rate - HOPS_PER_SECOND).abs() < HOPS_PER_SECOND * 0.15,
        "rythme {rate:.2} pas/s"
    );
    assert!((freq - 440.0).abs() < 5.0, "fréquence {freq:.1} Hz");
    // Ce que calcule l'analyseur (ici la valeur efficace dans `loud`) arrive bien au lecteur.
    assert!((f.loud - 0.5 * FRAC_1_SQRT_2).abs() < 0.01, "loud {}", f.loud);
    assert!(f.live);
    assert!(stats.latency_samples > 100);
    assert!(
        stats.latency_mean < LATENCY_TARGET,
        "latence moyenne {:?}",
        stats.latency_mean
    );
    assert_eq!(stats.dropped_events, 0);
    assert_eq!(stats.input_dropped, 0);
    assert!(matches!(r.capture.state(), CaptureState::Connected { .. }));
}

#[test]
fn resampled_capture_keeps_rate_and_pitch() {
    let device = SyntheticDevice::new("Synth 44k1", format(44_100, 4, SampleType::U16), sine(1000.0, 0.4));
    let mut r = rig(device, CaptureConfig::default()).unwrap();
    wait_for("live", Duration::from_secs(3), || r.outputs.features.latest().live);
    thread::sleep(Duration::from_millis(200));
    let pitch_mark = r.probe.mark();
    let rate = measure_rate(&mut r, Duration::from_millis(1500));
    let freq = r.probe.frequency_since(pitch_mark);
    let f = r.features();
    let stats = r.analysis.stats();
    eprintln!(
        "44,1 kHz u16 4 canaux → 48 kHz : {rate:.2} pas/s, {freq:.1} Hz, loud {:.4}, latence moy {:?} max {:?}",
        f.loud, stats.latency_mean, stats.latency_max
    );
    assert!(
        (rate - HOPS_PER_SECOND).abs() < HOPS_PER_SECOND * 0.15,
        "rythme {rate:.2} pas/s"
    );
    assert!((freq - 1000.0).abs() < 10.0, "fréquence {freq:.1} Hz");
    assert!((f.loud - 0.4 * FRAC_1_SQRT_2).abs() < 0.01, "loud {}", f.loud);
    assert!(
        stats.latency_mean < LATENCY_TARGET,
        "latence moyenne {:?}",
        stats.latency_mean
    );
}

#[test]
fn finite_source_gives_exact_step_count_then_goes_idle() {
    let mut device = SyntheticDevice::new("Synth fini", format(48_000, 1, SampleType::F32), sine(250.0, 0.3));
    // Une seconde exactement, puis plus rien (sans panne) : comme un lecteur mis en pause.
    device.total_frames = Some(48_000);
    let config = CaptureConfig {
        stall_timeout: Duration::from_secs(60),
        ..CaptureConfig::default()
    };
    let mut r = rig(device, config).unwrap();
    wait_for("live", Duration::from_secs(3), || r.outputs.features.latest().live);
    let silence = wait_for("live = false", Duration::from_secs(3), || {
        !r.outputs.features.latest().live
    });
    let f = r.features();
    eprintln!(
        "source finie : {} pas, live = false {silence:?} après le passage à vrai",
        f.step
    );
    // 48 000 / 512 = 93,75 : 93 pas complets, le reste attend un pas qui ne viendra pas.
    assert_eq!(f.step, 93);
    assert!((f.time_s - 93.0 * HOP_SECONDS).abs() < 1e-9);
    // Au repos : énergies à zéro (`Analyzer::idle`), horloge figée.
    assert!(f.loud.abs() < f32::EPSILON);
    assert!(
        r.next_event(AudioEventKind::LiveChanged { live: false }, Duration::from_secs(1))
            .is_some()
    );
    assert_eq!(r.probe.idles.load(std::sync::atomic::Ordering::Relaxed), 1);
    thread::sleep(Duration::from_millis(100));
    assert_eq!(r.features().step, 93);
}

#[test]
fn hot_switch_between_capture_and_demo_keeps_the_thread() {
    let device = SyntheticDevice::new("Micro", format(48_000, 1, SampleType::F32), sine(440.0, 0.5));
    let mut r = rig(device, CaptureConfig::default()).unwrap();
    wait_for("live", Duration::from_secs(3), || r.outputs.features.latest().live);

    // Lecteur de démo simulé (J1-02) : son callback de sortie recopie le signal joué vers l'analyse.
    let (mut writer, demo) = feed(SourceKind::Demo, 48_000, 24_000).unwrap();
    r.analysis.control().attach(demo);
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let player_stop = std::sync::Arc::clone(&stop);
    let player = thread::spawn(move || {
        let mut block = [0.0_f32; 480];
        let mut n = 0_u64;
        let mut deadline = Instant::now();
        while !player_stop.load(std::sync::atomic::Ordering::Relaxed) {
            for sample in &mut block {
                *sample = 0.2 * (std::f64::consts::TAU * 1000.0 * n as f64 / 48_000.0).sin() as f32;
                n += 1;
            }
            writer.push_interleaved(&block, 1);
            deadline += Duration::from_millis(10);
            thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
    });

    thread::sleep(Duration::from_millis(200));
    let before = r.features().step;
    r.analysis.control().select(Some(SourceKind::Demo));
    let event = r.next_event(
        AudioEventKind::SourceChanged {
            source: Some(SourceKind::Demo),
        },
        Duration::from_secs(1),
    );
    assert!(event.is_some(), "événement SourceChanged(Demo) attendu");
    thread::sleep(Duration::from_millis(300));
    let mark = r.probe.mark();
    thread::sleep(Duration::from_millis(500));
    let freq = r.probe.frequency_since(mark);
    let f = r.features();
    eprintln!(
        "bascule vers la démo : {freq:.1} Hz, loud {:.4}, pas {before} → {}",
        f.loud, f.step
    );
    assert!((freq - 1000.0).abs() < 10.0, "fréquence {freq:.1} Hz");
    assert!((f.loud - 0.2 * FRAC_1_SQRT_2).abs() < 0.01);
    // Même thread, même horloge : les pas continuent sans repartir de zéro.
    assert!(f.step > before + 40);
    assert!(f.live);
    assert_eq!(r.probe.resets.load(std::sync::atomic::Ordering::Relaxed), 1);

    r.analysis.control().select(Some(SourceKind::Capture));
    thread::sleep(Duration::from_millis(300));
    let mark = r.probe.mark();
    thread::sleep(Duration::from_millis(500));
    let freq = r.probe.frequency_since(mark);
    assert!((freq - 440.0).abs() < 5.0, "retour au micro : {freq:.1} Hz");

    // Aucune source : live tombe à faux tout de suite (pas d'attente de famine).
    r.analysis.control().select(None);
    let off = wait_for("live = false sans source", Duration::from_secs(1), || {
        !r.outputs.features.latest().live
    });
    assert!(off < Duration::from_millis(100), "{off:?}");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    player.join().unwrap();
}
