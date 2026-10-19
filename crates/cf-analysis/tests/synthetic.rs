//! Signaux synthétiques à tempo connu à travers `BeatAnalyzer` (émulation de l'analyseur + `BeatDetector`) au pas du
//! thread d'analyse (512 échantillons à 48 kHz), puis de bout en bout par le thread d'analyse.
//!
//! Le « clic » est un coup de grosse caisse : sinus grave (55 Hz) à décroissance rapide, car `beat.ts` ne regarde que
//! le flux d'énergie entre 40 et 150 Hz (un clic d'un échantillon, large bande, y tombe aussi mais avec un niveau qui
//! dépend de sa position dans la fenêtre).

#![allow(clippy::print_stderr, reason = "bilan chiffré par test, visible avec --nocapture")]

use std::f64::consts::PI;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant};

use cf_analysis::analyser_emulation::{FFT_SIZE, SMOOTHING_TIME_CONSTANT};
use cf_analysis::{
    AnalyserEmulation, AnalysisConfig, AnalysisThread, Analyzer, AudioEventKind, AudioFeatures, BeatAnalyzer,
    BeatDetector, EventSink, StepContext,
};
use cf_audio::feed::{HOP, HOP_SECONDS, feed};
use cf_audio::source::SourceKind;

// `AllocDisabler` n'existe qu'en debug : le thread d'analyse exécute les pas sous `assert_no_alloc`, donc une allocation
// de `BeatAnalyzer` dans le test de bout en bout ferait avorter le binaire.
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const RATE: f64 = 48_000.0;

/// Train de coups de grosse caisse à `bpm`, `seconds` de long, premier coup à `first_s`.
fn kick_train(bpm: f64, seconds: f64, first_s: f64) -> Vec<f32> {
    let len = (seconds * RATE) as usize;
    let period = 60.0 / bpm;
    let mut out = vec![0.0_f32; len];
    let mut onset = first_s;
    while onset < seconds {
        let start = (onset * RATE) as usize;
        // Queue de 0,6 s (−106 dB) : tronquer plus tôt ferait un clic parasite que le détecteur prendrait pour un coup.
        for (n, slot) in out.iter_mut().enumerate().skip(start).take((0.6 * RATE) as usize) {
            let t = (n - start) as f64 / RATE;
            *slot += (0.8 * (-t / 0.05).exp() * (2.0 * PI * 55.0 * t).sin()) as f32;
        }
        onset += period;
    }
    out
}

/// Bruit blanc uniforme de niveau crête `amplitude`, `mulberry32(seed)` : le générateur de
/// `tools/sillage-golden/noise.ts` (arithmétique entière 32 bits, identique bit à bit à celle de JavaScript).
fn white_noise(amplitude: f64, seconds: f64, seed: u32) -> Vec<f32> {
    let mut a = seed;
    (0..(seconds * RATE) as usize)
        .map(|_| {
            a = a.wrapping_add(0x6d2b_79f5);
            let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
            t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(0x3d | t)) ^ t;
            let uniform = f64::from(t ^ (t >> 14)) / 4_294_967_296.0;
            (amplitude * (uniform * 2.0 - 1.0)) as f32
        })
        .collect()
}

/// Kicks de `beat.ts` (pas de 800 échantillons, 60 i/s) sur `white_noise(0.3, 30.0, 12345)`, produits par
/// `bun run tools/sillage-golden/noise.ts`. Sillage voit des kicks dans un bruit stationnaire : le portage aussi.
const SILLAGE_NOISE_KICKS: [u64; 70] = [
    1, 26, 50, 92, 110, 128, 143, 163, 180, 196, 216, 246, 275, 294, 320, 365, 388, 417, 433, 465, 481, 496, 517, 548,
    583, 607, 630, 649, 669, 687, 712, 735, 751, 786, 803, 821, 863, 897, 920, 947, 970, 987, 1015, 1038, 1068, 1087,
    1104, 1126, 1173, 1203, 1222, 1237, 1265, 1289, 1339, 1363, 1381, 1432, 1447, 1467, 1487, 1512, 1567, 1589, 1645,
    1668, 1712, 1741, 1763, 1782,
];
/// Tempo final de `beat.ts` sur ce bruit : un faux tempo, médiane d'intervalles aléatoires.
const SILLAGE_NOISE_BPM: f32 = 144.0;

/// Ce qu'une passe à travers l'analyseur a vu.
struct Run {
    kick_times_s: Vec<f64>,
    events: usize,
    last: AudioFeatures,
    max_loud: f32,
}

fn run(signal: &[f32]) -> Run {
    let (tx, rx) = crossbeam_channel::bounded(4096);
    let dropped = AtomicU64::new(0);
    let mut analyzer = BeatAnalyzer::new();
    let mut features = AudioFeatures::NEUTRAL;
    let mut kick_times_s = Vec::new();
    let mut max_loud = 0.0_f32;
    for (i, hop) in signal.as_chunks::<HOP>().0.iter().enumerate() {
        let step = i as u64 + 1;
        let time_s = step as f64 * HOP_SECONDS;
        let ctx = StepContext {
            step,
            time_s,
            dt_s: HOP_SECONDS as f32,
            sample_rate: 48_000,
        };
        let mut sink = EventSink::new(&tx, &dropped, step, time_s);
        analyzer.step(hop, &ctx, &mut features, &mut sink);
        max_loud = max_loud.max(features.loud);
        if features.kick {
            kick_times_s.push(time_s);
        }
    }
    Run {
        kick_times_s,
        events: rx.try_iter().filter(|e| e.kind == AudioEventKind::Kick).count(),
        last: features,
        max_loud,
    }
}

/// Un train à `bpm` : le tempo trouvé, le nombre de kicks et leur position par rapport aux coups.
fn check_tempo(bpm: f64) {
    let first = 0.5;
    let seconds = 30.0;
    let result = run(&kick_train(bpm, seconds, first));
    let period = 60.0 / bpm;
    let expected = ((seconds - first) / period).floor() as usize + 1;
    let found = result.last.bpm.unwrap_or_default();
    assert!(found > 0.0, "aucun tempo établi pour {bpm} BPM");
    eprintln!(
        "{bpm} BPM : tempo {found}, {} kicks pour {expected} coups, {} événements",
        result.kick_times_s.len(),
        result.events
    );
    // Les pas durent 10,7 ms : les intervalles sont des multiples de ce pas, d'où l'écart de quelques dixièmes de BPM.
    assert!((f64::from(found) - bpm).abs() <= 3.0, "tempo {found} pour {bpm}");
    assert!(
        result.kick_times_s.len().abs_diff(expected) <= 2,
        "{} kicks pour {expected} coups",
        result.kick_times_s.len()
    );
    // Chaque coup est détecté dans les quelques pas qui suivent son attaque (lissage de l'analyseur compris).
    let late = |t: f64| {
        let beats = ((t - first) / period).round();
        t - (first + beats * period)
    };
    assert!(
        result.kick_times_s.iter().all(|t| (0.0..0.06).contains(&late(*t))),
        "kicks hors de la fenêtre d'attaque : {:?}",
        result.kick_times_s.iter().map(|t| late(*t)).collect::<Vec<_>>()
    );
    assert_eq!(result.events, result.kick_times_s.len(), "un événement Kick par kick");
    assert_eq!(result.last.count as usize, result.kick_times_s.len());
}

#[test]
fn clicks_at_120_bpm() {
    check_tempo(120.0);
}

#[test]
fn clicks_at_128_bpm() {
    check_tempo(128.0);
}

#[test]
fn clicks_at_174_bpm() {
    check_tempo(174.0);
}

#[test]
fn silence_gives_nothing() {
    let result = run(&vec![0.0; 10 * 48_000]);
    assert_eq!(result.kick_times_s.len(), 0);
    assert_eq!(result.last.bpm, None);
    assert_eq!(result.last.count, 0);
    assert!(result.max_loud.abs() < f32::EPSILON);
}

/// Un bruit blanc n'a pas de tempo, mais `beat.ts` y détecte des kicks (les quatre bandes fluctuent au hasard) et en
/// tire un faux tempo : le portage doit faire exactement pareil, pas « réparer ». Référence : `beat.ts` d'origine
/// sur le même bruit, au même pas (800 échantillons, 60 i/s).
#[test]
fn white_noise_matches_sillage() {
    let noise = white_noise(0.3, 30.0, 12345);
    let mut analyser = AnalyserEmulation::new(SMOOTHING_TIME_CONSTANT);
    let mut detector = BeatDetector::new();
    let mut kicks = Vec::new();
    for (i, hop) in noise.as_chunks::<800>().0.iter().enumerate() {
        let step = i as u64 + 1;
        analyser.push(hop);
        let spectrum = *analyser.byte_frequency_data();
        if detector
            .update(Some(&spectrum), 48_000, FFT_SIZE, step as f64 / 60.0, 1.0 / 60.0)
            .kick
        {
            kicks.push(step);
        }
    }
    assert_eq!(kicks, SILLAGE_NOISE_KICKS);
    assert_eq!(detector.state().bpm, Some(SILLAGE_NOISE_BPM));
}

/// Au pas du thread d'analyse, le bruit blanc ne casse rien : sorties dans 0..1, kicks séparés par la période
/// réfractaire, aucune donnée non finie.
#[test]
fn white_noise_stays_bounded_at_the_analysis_step() {
    let result = run(&white_noise(0.3, 30.0, 12345));
    eprintln!(
        "bruit blanc : {} kicks, tempo {:?}",
        result.kick_times_s.len(),
        result.last.bpm
    );
    assert!(
        result.kick_times_s.windows(2).all(|w| w[1] - w[0] > 0.24),
        "période réfractaire non respectée"
    );
    assert!(
        result.max_loud > 0.1 && result.max_loud <= 1.0,
        "loud {}",
        result.max_loud
    );
    let f = result.last;
    assert!(
        [f.bass, f.mid, f.high, f.loud, f.pulse]
            .iter()
            .all(|v| (0.0..=1.0).contains(v))
    );
    assert!(f.beat.is_finite() && f.beat >= f64::from(f.count));
}

#[test]
fn tempo_follows_a_new_tempo_after_a_long_silence() {
    let mut signal = kick_train(150.0, 15.0, 0.5);
    signal.extend(vec![0.0; 12 * 48_000]);
    signal.extend(kick_train(100.0, 20.0, 0.3));
    let result = run(&signal);
    let found = result.last.bpm.expect("tempo établi");
    assert!((f64::from(found) - 100.0).abs() <= 3.0, "tempo {found}");
}

/// Bout en bout : la même musique par le thread d'analyse (flux → pas de 512 → `AudioFeatures` + événements).
#[test]
fn analysis_thread_publishes_tempo_and_kick_events() {
    let config = AnalysisConfig {
        initial_source: Some(SourceKind::Demo),
        ..AnalysisConfig::default()
    };
    let (analysis, mut outputs) = AnalysisThread::spawn(BeatAnalyzer::new(), config).expect("thread d'analyse");
    let signal = kick_train(128.0, 20.0, 0.5);
    let (mut writer, input) = feed(SourceKind::Demo, 48_000, signal.len()).expect("flux");
    analysis.control().attach(input);
    writer.push_interleaved(&signal, 1);

    let last_step = (signal.len() / HOP) as u64;
    let deadline = Instant::now() + Duration::from_secs(10);
    while outputs.features.latest().step < last_step && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let features = *outputs.features.latest();
    let kicks = outputs
        .events
        .try_iter()
        .filter(|e| e.kind == AudioEventKind::Kick)
        .count();
    eprintln!(
        "thread : pas {}, tempo {:?}, {kicks} événements Kick",
        features.step, features.bpm
    );
    assert_eq!(features.step, last_step);
    assert!(features.live);
    let bpm = features.bpm.expect("tempo publié");
    assert!((f64::from(bpm) - 128.0).abs() <= 3.0, "tempo {bpm}");
    assert!(
        kicks.abs_diff(features.count as usize) == 0 && kicks >= 40,
        "{kicks} kicks"
    );
}
