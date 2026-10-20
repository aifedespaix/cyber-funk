//! Sections et intensité sur des signaux synthétiques au pas du thread d'analyse (512 échantillons à 48 kHz,
//! 93,75 i/s) : montée progressive, break, drop. Le golden de Sillage est calculé à 800 échantillons (60 i/s) ; ces
//! tests vérifient que le comportement tient au pas réel, avec les événements `SectionChanged` et le drapeau
//! `AudioFeatures::section_changed`.
//!
//! Le signal est un bruit blanc dont le niveau suit une enveloppe : il remplit les trois bandes, donc `loud` suit
//! l'enveloppe (le détecteur de kicks n'intervient pas dans les sections, qui ne lisent que `loud`).

#![allow(clippy::print_stderr, reason = "bilan chiffré par test, visible avec --nocapture")]

use std::sync::atomic::AtomicU64;

use cf_analysis::{Analyzer, AudioEvent, AudioEventKind, AudioFeatures, BeatAnalyzer, EventSink, Section, StepContext};
use cf_audio::feed::{HOP, HOP_SECONDS};
use cf_audio::source::SourceKind;

const RATE: f64 = 48_000.0;
/// Niveau (crête, bruit blanc) d'un passage fort et d'un passage calme.
const LOUD: f64 = 0.35;
const QUIET: f64 = 0.001;

/// Générateur de bruit uniforme `mulberry32`, comme `tools/sillage-golden/noise.ts`.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = (self.0 ^ (self.0 >> 15)).wrapping_mul(1 | self.0);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(0x3d | t)) ^ t;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0 * 2.0 - 1.0
    }
}

/// Ce que `BeatAnalyzer` publie sur un signal : changements de section datés, intensité et pas où le drapeau est levé.
struct Run {
    /// `(temps audio, section)` d'après les événements.
    events: Vec<(f64, Section)>,
    /// Temps des pas où `section_changed` est vrai.
    flagged: Vec<f64>,
    /// Intensité à la fin de chaque seconde.
    intensity: Vec<f32>,
    last: AudioFeatures,
}

impl Run {
    fn sections(&self) -> Vec<Section> {
        self.events.iter().map(|e| e.1).collect()
    }

    fn at(&self, seconds: f64) -> Section {
        self.events
            .iter()
            .take_while(|e| e.0 <= seconds)
            .last()
            .map_or(Section::Groove, |e| e.1)
    }
}

/// Joue `seconds` de bruit dont le niveau crête vaut `level(t)`.
fn run(source: Option<SourceKind>, seconds: f64, level: impl Fn(f64) -> f64) -> Run {
    let (tx, rx) = crossbeam_channel::bounded::<AudioEvent>(1024);
    let dropped = AtomicU64::new(0);
    let mut analyzer = BeatAnalyzer::new();
    analyzer.set_source(source);
    let mut noise = Noise(0x00C0_FFEE);
    let mut features = AudioFeatures::NEUTRAL;
    let mut out = Run {
        events: Vec::new(),
        flagged: Vec::new(),
        intensity: Vec::new(),
        last: features,
    };
    let steps = (seconds * RATE / HOP as f64) as u64;
    let mut hop = [0.0_f32; HOP];
    for step in 1..=steps {
        let time_s = step as f64 * HOP_SECONDS;
        let amplitude = level(time_s);
        for slot in &mut hop {
            *slot = (amplitude * noise.next()) as f32;
        }
        let ctx = StepContext {
            step,
            time_s,
            dt_s: HOP_SECONDS as f32,
            sample_rate: 48_000,
        };
        features.step = step;
        features.time_s = time_s;
        let mut sink = EventSink::new(&tx, &dropped, step, time_s);
        analyzer.step(&hop, &ctx, &mut features, &mut sink);
        if features.section_changed {
            out.flagged.push(time_s);
        }
        if step % (RATE as u64 / HOP as u64) == 0 {
            out.intensity.push(features.intensity);
        }
    }
    while let Ok(event) = rx.try_recv() {
        if let AudioEventKind::SectionChanged { section } = event.kind {
            out.events.push((event.time_s, section));
        }
    }
    out.last = features;
    out
}

fn print(label: &str, run: &Run) {
    let events: Vec<String> = run.events.iter().map(|(t, s)| format!("{t:.1}s {s:?}")).collect();
    eprintln!(
        "{label} : {} | intensité finale {:.3}",
        events.join(", "),
        run.last.intensity
    );
}

fn check_flags(run: &Run) {
    // Un drapeau par événement, au même instant : `section_changed` n'est levé que pendant le pas du changement.
    assert_eq!(run.flagged.len(), run.events.len());
    for (flag, event) in run.flagged.iter().zip(&run.events) {
        assert!((flag - event.0).abs() < 1e-9);
    }
}

#[test]
fn steady_loud_signal_reaches_peak_and_stays() {
    let run = run(None, 60.0, |_| LOUD);
    print("fort constant", &run);
    assert_eq!(run.sections(), [Section::Peak]);
    assert!(run.events[0].0 < 10.0);
    assert_eq!(run.last.section, Section::Peak);
    check_flags(&run);
}

#[test]
fn quiet_signal_falls_to_calm() {
    let run = run(None, 30.0, |_| QUIET);
    print("calme constant", &run);
    assert_eq!(run.sections(), [Section::Calm]);
    check_flags(&run);
}

#[test]
fn progressive_build_climbs_through_groove_to_peak() {
    // 20 s calmes, puis le niveau monte de façon exponentielle (linéaire en dB) sur 80 s, puis reste au sommet.
    let level = |t: f64| {
        if t < 20.0 {
            QUIET
        } else {
            (QUIET * (LOUD / QUIET).powf(((t - 20.0) / 80.0).min(1.0))).min(LOUD)
        }
    };
    let run = run(None, 140.0, level);
    print("montée progressive", &run);
    check_flags(&run);
    let sections = run.sections();
    assert_eq!(sections.first(), Some(&Section::Calm), "{sections:?}");
    assert_eq!(sections.last(), Some(&Section::Peak), "{sections:?}");
    // Jamais de saut direct calme → sommet pendant la montée : on passe par groove (au moins un changement entre
    // les deux) sauf si l'intensité franchit les deux seuils d'un coup, ce que le délai de 6 s interdit ici.
    assert!(sections.contains(&Section::Groove), "{sections:?}");
    assert!(run.events.windows(2).all(|w| w[1].0 - w[0].0 >= 6.0 - 1e-6));
    // L'intensité monte avec le niveau.
    assert!(run.intensity.last().copied().unwrap_or_default() > run.intensity[19] + 0.3);
}

#[test]
fn break_returns_to_calm_then_drop_returns_to_peak() {
    // 40 s fortes, break de 30 s (presque silence), drop à 70 s.
    let level = |t: f64| if (40.0..70.0).contains(&t) { QUIET } else { LOUD };
    let run = run(None, 110.0, level);
    print("break puis drop", &run);
    check_flags(&run);
    // L'intensité est lissée et la hystérésis étagée : on descend et on remonte par groove, à 6 s d'intervalle au moins.
    assert_eq!(
        run.sections(),
        [
            Section::Peak,
            Section::Groove,
            Section::Calm,
            Section::Groove,
            Section::Peak
        ]
    );
    let calm = run.events[2].0;
    let peak = run.events[4].0;
    assert!((40.0..60.0).contains(&calm), "calme à {calm}");
    // Le sommet revient peu après le drop (lissage de 2,5 s et délai minimal de 6 s depuis le calme).
    assert!(peak > 70.0 && peak - 70.0 < 12.0, "sommet à {peak}");
    assert_eq!(run.at(65.0), Section::Calm);
    assert_eq!(run.at(100.0), Section::Peak);
}

#[test]
fn capture_at_minus_30_db_follows_the_same_sections_as_a_file() {
    // Même morceau à pleine échelle (démo) et 30 dB plus bas (micro, gain automatique) : même forme.
    let level = |t: f64| if (40.0..70.0).contains(&t) { QUIET } else { LOUD };
    let file = run(Some(SourceKind::Demo), 110.0, level);
    let mic = run(Some(SourceKind::Capture), 110.0, |t| level(t) * 0.031_622_776);
    print("démo", &file);
    print("capture -30 dB", &mic);
    assert_eq!(file.sections(), mic.sections());
}

#[test]
fn reset_forgets_the_mood() {
    let (tx, _rx) = crossbeam_channel::bounded::<AudioEvent>(8);
    let dropped = AtomicU64::new(0);
    let mut analyzer = BeatAnalyzer::new();
    let mut features = AudioFeatures::NEUTRAL;
    let mut noise = Noise(7);
    let mut hop = [0.0_f32; HOP];
    for step in 1..=3000_u64 {
        for slot in &mut hop {
            *slot = (LOUD * noise.next()) as f32;
        }
        let ctx = StepContext {
            step,
            time_s: step as f64 * HOP_SECONDS,
            dt_s: HOP_SECONDS as f32,
            sample_rate: 48_000,
        };
        let mut sink = EventSink::new(&tx, &dropped, step, ctx.time_s);
        analyzer.step(&hop, &ctx, &mut features, &mut sink);
    }
    assert_eq!(features.section, Section::Peak);
    analyzer.reset();
    analyzer.idle(&mut features);
    assert_eq!(features.section, Section::Groove);
    assert!((features.intensity - 0.4).abs() < 1e-6);
}
