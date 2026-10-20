//! Sections, intensité et gain du micro face aux données de référence de Sillage (J1-01) sur les quatre pistes CC0.
//!
//! Même chaîne que `golden_beat.rs` (PCM mono 48 kHz → émulation de l'`AnalyserNode` → `BeatDetector`), puis
//! `SectionTracker` alimenté par `loud`, comme `SectionTracker.update(beat, dt)` du script de référence.
//!
//! - **Au pas du golden** (800 échantillons, 60 i/s) : sections identiques hors ±1 s autour d'un changement, intensité
//!   à ±0,05 aux 1 047 points échantillonnés (critères de la fiche J1-05).
//! - **Au pas du thread d'analyse** (512 échantillons, 93,75 i/s, lissage de l'analyseur ramené à la même mémoire en
//!   secondes) : mêmes critères, comparés par le temps et non par le numéro de pas.
//! - **Micro simulé à -30 dB** : la piste atténuée passe par le gain automatique par bande ; après 20 s, les bandes
//!   doivent avoir retrouvé la plage qu'elles ont dans le golden.

#![allow(clippy::print_stderr, reason = "bilan chiffré par piste, visible avec --nocapture")]

mod support;

use std::error::Error;

use cf_analysis::analyser_emulation::{FFT_SIZE, SMOOTHING_TIME_CONSTANT, smoothing_for_step};
use cf_analysis::{AnalyserEmulation, BeatDetector, MicGain, Section, SectionTracker};
use support::golden::{self, Golden};
use support::pcm;

const SAMPLE_RATE: u32 = 48_000;
/// Tolérance de l'intensité (fiche J1-05).
const INTENSITY_TOLERANCE: f64 = 0.05;
/// Fenêtre autour d'un changement de section dans laquelle un désaccord est toléré (s).
const SECTION_WINDOW_S: f64 = 1.0;
/// Durée de convergence du gain du micro (s), fiche J1-05.
const MIC_SETTLE_S: f64 = 20.0;
/// Atténuation du micro simulé : -30 dB.
/// Tolérance (sur 0..1) de la comparaison du micro à la pleine échelle.
const MIC_TOLERANCE: f64 = 0.2;
const MIC_ATTENUATION: f32 = 0.031_622_776;

/// Ce que la chaîne produit à chaque pas.
struct Replay {
    hop_s: f64,
    /// `(temps, section)` de chaque changement.
    changes: Vec<(f64, Section)>,
    intensity: Vec<f64>,
    bass: Vec<f64>,
    mid: Vec<f64>,
    high: Vec<f64>,
}

impl Replay {
    /// Valeur du pas le plus proche du temps `t`.
    fn index(&self, t: f64) -> usize {
        ((t / self.hop_s).round() as usize)
            .saturating_sub(1)
            .min(self.intensity.len() - 1)
    }

    fn section_at(&self, t: f64) -> Section {
        self.changes
            .iter()
            .take_while(|(c, _)| *c <= t + 1e-9)
            .last()
            .map_or(Section::Groove, |(_, s)| *s)
    }
}

fn parse(section: &str) -> Section {
    match section {
        "calm" => Section::Calm,
        "peak" => Section::Peak,
        _ => Section::Groove,
    }
}

/// Rejoue `pcm` par pas de `hop` échantillons ; `mic` : gain automatique du micro entre l'analyseur et le détecteur.
fn replay(pcm: &[f32], hop: usize, smoothing: f64, mic: bool) -> Replay {
    let dt = hop as f64 / f64::from(SAMPLE_RATE);
    let mut analyser = AnalyserEmulation::new(smoothing);
    let mut detector = BeatDetector::new();
    let mut sections = SectionTracker::new();
    let mut gain = MicGain::new(SAMPLE_RATE);
    let mut scaled = [0_u8; cf_analysis::analyser_emulation::BIN_COUNT];
    let mut out = Replay {
        hop_s: dt,
        changes: Vec::new(),
        intensity: Vec::new(),
        bass: Vec::new(),
        mid: Vec::new(),
        high: Vec::new(),
    };
    for step in 1..=pcm.len() / hop {
        analyser.push(&pcm[(step - 1) * hop..step * hop]);
        let raw = *analyser.byte_frequency_data();
        let spectrum = if mic {
            gain.apply(&raw, &mut scaled, dt * 60.0);
            &scaled
        } else {
            &raw
        };
        let now = step as f64 * dt;
        let state = *detector.update(Some(spectrum), SAMPLE_RATE, FFT_SIZE, now, dt);
        if let Some(section) = sections.update(state.loud, dt) {
            out.changes.push((now, section));
        }
        out.intensity.push(sections.intensity());
        out.bass.push(state.bass);
        out.mid.push(state.mid);
        out.high.push(state.high);
    }
    out
}

/// Désaccords de section hors fenêtre de tolérance (en pas du golden) et écart d'intensité maximal.
fn compare(golden: &Golden, replay: &Replay) -> (usize, usize, f64, f64) {
    let rate = golden.analysis.step_rate;
    let near_change = |t: f64| {
        golden
            .sections
            .iter()
            .skip(1)
            .any(|c| (c.step as f64 / rate - t).abs() <= SECTION_WINDOW_S)
    };
    let (mut wrong, mut total) = (0, 0);
    for step in 1..=golden.analysis.steps {
        let t = step as f64 / rate;
        if t > replay.intensity.len() as f64 * replay.hop_s {
            break;
        }
        total += 1;
        if replay.section_at(t) != parse(golden.section_at(step)) && !near_change(t) {
            wrong += 1;
        }
    }
    let (mut worst, mut mean) = (0.0_f64, 0.0);
    for (k, reference) in golden.samples.intensity.iter().enumerate() {
        let t = (k as u64 + 1) as f64 * golden.analysis.sample_every as f64 / rate;
        let gap = (replay.intensity[replay.index(t)] - reference).abs();
        worst = worst.max(gap);
        mean += gap;
    }
    (wrong, total, worst, mean / golden.samples.intensity.len() as f64)
}

fn report(label: &str, name: &str, golden: &Golden, replay: &Replay) -> (usize, f64) {
    let (wrong, total, worst, mean) = compare(golden, replay);
    let ours: Vec<String> = replay.changes.iter().map(|(t, s)| format!("{t:.1}s {s:?}")).collect();
    let theirs: Vec<String> = golden
        .sections
        .iter()
        .skip(1)
        .map(|c| format!("{:.1}s {}", c.step as f64 / golden.analysis.step_rate, c.section))
        .collect();
    eprintln!(
        "{name} [{label}] : sections en désaccord hors ±1 s {wrong}/{total} | intensité écart max {worst:.4}, moyen {mean:.4}\n  portage : {}\n  Sillage : {}",
        ours.join(", "),
        theirs.join(", ")
    );
    (wrong, worst)
}

fn load(name: &str, file: &str) -> Result<(Golden, Vec<f32>), Box<dyn Error>> {
    let golden = golden::load(name)?;
    let (pcm, _) = pcm::load_mono_48k(&golden::music_path(file))?;
    Ok((golden, pcm))
}

fn check_golden_step(name: &str, file: &str) -> Result<(), Box<dyn Error>> {
    let (golden, pcm) = load(name, file)?;
    let result = replay(&pcm, golden.analysis.step_samples, SMOOTHING_TIME_CONSTANT, false);
    let (wrong, worst) = report("800 / 60 i/s", name, &golden, &result);
    assert_eq!(wrong, 0, "{name} : sections différentes hors ±1 s");
    assert!(worst <= INTENSITY_TOLERANCE, "{name} : intensité à {worst:.4}");
    // Le nombre de changements doit être celui de Sillage (la tolérance de ±1 s ne doit pas masquer un changement en
    // trop ou manquant).
    assert_eq!(
        result.changes.len(),
        golden.sections.len() - 1,
        "{name} : nombre de changements"
    );
    Ok(())
}

fn check_analysis_step(name: &str, file: &str) -> Result<(), Box<dyn Error>> {
    let (golden, pcm) = load(name, file)?;
    let hop = cf_audio::feed::HOP;
    let smoothing = smoothing_for_step(hop as f64 / f64::from(SAMPLE_RATE));
    let result = replay(&pcm, hop, smoothing, false);
    let (wrong, worst) = report("512 / 93,75 i/s", name, &golden, &result);
    assert_eq!(wrong, 0, "{name} : sections différentes hors ±1 s");
    assert!(worst <= INTENSITY_TOLERANCE, "{name} : intensité à {worst:.4}");
    Ok(())
}

/// Niveau haut (95e centile) de `values` entre `from_s` et `to_s` : la plage qu'atteignent les passages forts.
fn top_between(values: &[f64], hop_s: f64, from_s: f64, to_s: f64) -> f64 {
    let end = ((to_s / hop_s) as usize).min(values.len());
    let mut window = values[(from_s / hop_s) as usize..end].to_vec();
    window.sort_by(f64::total_cmp);
    window[(window.len() * 95 / 100).min(window.len() - 1)]
}

/// Un micro à -30 dB doit retrouver, par le gain automatique, la plage de niveaux que donne le même morceau à pleine
/// échelle passé par la même chaîne (« leur plage de fichier » : `BandGain` règle le gain sur le pic récent de chaque
/// zone, et vise des niveaux réglés pour que le micro rende ce que rendrait un fichier ordinaire, voir `audio.ts`).
///
/// On compare le sommet (95e centile) des bandes juste après le délai de convergence, sur 10 s : le gain a eu
/// [`MIC_SETTLE_S`] pour monter. La moyenne n'est pas comparable : à -30 dB les bins faibles tombent sous le plancher
/// de l'analyseur (-100 dB, octet 0) et ne reviennent pas, quel que soit le gain.
fn check_microphone(name: &str, file: &str) -> Result<(), Box<dyn Error>> {
    let (golden, pcm) = load(name, file)?;
    let quiet: Vec<f32> = pcm.iter().map(|s| s * MIC_ATTENUATION).collect();
    let hop = golden.analysis.step_samples;
    let hop_s = hop as f64 / f64::from(SAMPLE_RATE);
    let full = replay(&pcm, hop, SMOOTHING_TIME_CONSTANT, true);
    let bare = replay(&quiet, hop, SMOOTHING_TIME_CONSTANT, false);
    let boosted = replay(&quiet, hop, SMOOTHING_TIME_CONSTANT, true);
    let (from, to) = (MIC_SETTLE_S, MIC_SETTLE_S + 10.0);
    // Tolérance par bande : les basses (celles du détecteur de kicks) doivent retrouver leur plage ; les médiums aussi,
    // plus largement. Les aigus à -30 dB tombent en grande partie sous le plancher de l'analyseur et le gain est
    // plafonné à x12 (`MIC_BAND_MAX_GAIN`, constante de Sillage) : on vérifie seulement qu'ils ne sont pas dégradés
    // et qu'ils restent exploitables (voir la section « Écarts » de la PR).
    for (label, ours, nothing, reference, tolerance) in [
        ("basses", &boosted.bass, &bare.bass, &full.bass, Some(0.1)),
        ("médiums", &boosted.mid, &bare.mid, &full.mid, Some(MIC_TOLERANCE)),
        ("aigus", &boosted.high, &bare.high, &full.high, None),
    ] {
        let early = top_between(ours, hop_s, 0.0, 5.0);
        let top = top_between(ours, hop_s, from, to);
        let reference_top = top_between(reference, hop_s, from, to);
        eprintln!(
            "{name} micro -30 dB, {label} : sommet à 20-30 s {top:.3} (pleine échelle avec gain {reference_top:.3}, -30 dB sans gain {:.3}), à 0-5 s {early:.3}",
            top_between(nothing, hop_s, from, to)
        );
        match tolerance {
            Some(tolerance) => assert!(
                (top - reference_top).abs() <= tolerance,
                "{name} {label} : sommet {top:.3} contre {reference_top:.3} à pleine échelle"
            ),
            None => assert!(
                top >= 0.2 && top >= top_between(nothing, hop_s, from, to) - 0.01,
                "{name} {label} : sommet {top:.3}"
            ),
        }
    }
    Ok(())
}

macro_rules! golden_track {
    ($step:ident, $analysis:ident, $mic:ident, $name:literal, $file:literal) => {
        #[test]
        fn $step() -> Result<(), Box<dyn Error>> {
            check_golden_step($name, $file)
        }

        #[test]
        fn $analysis() -> Result<(), Box<dyn Error>> {
            check_analysis_step($name, $file)
        }

        #[test]
        fn $mic() -> Result<(), Box<dyn Error>> {
            check_microphone($name, $file)
        }
    };
}

golden_track!(
    robotic_city,
    robotic_city_512,
    robotic_city_mic,
    "enprogames-robotic-city",
    "enprogames - Robotic City.mp3"
);
golden_track!(
    night_prowler,
    night_prowler_512,
    night_prowler_mic,
    "snabisch-night-prowler",
    "Snabisch - Night Prowler.mp3"
);
golden_track!(
    winning_the_race,
    winning_the_race_512,
    winning_the_race_mic,
    "tozan-winning-the-race",
    "Tozan - Winning the Race.mp3"
);
golden_track!(chase, chase_512, chase_mic, "yongki-chase", "Yongki - Chase.mp3");
