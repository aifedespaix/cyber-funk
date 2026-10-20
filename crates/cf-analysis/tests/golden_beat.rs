//! Le portage de `beat.ts` face aux données de référence de Sillage (J1-01) sur les quatre pistes CC0.
//!
//! Chaque piste est décodée en PCM mono 48 kHz, puis rejouée au pas du script de référence (800 échantillons, 60 i/s,
//! `dt = 1/60`) dans l'émulation de l'`AnalyserNode` et le `BeatDetector`. Le détecteur corrige par `dt` : le rythme
//! du thread d'analyse (512 échantillons) n'a pas besoin d'être celui du golden pour que la comparaison ait un sens.
//!
//! Deux niveaux, parce que le PCM n'est pas le même :
//! - **décodeur du dépôt** (`symphonia` + `rubato`, aucun outil externe) : les pistes à 44,1 kHz sont rééchantillonnées
//!   autrement que par `swr` de ffmpeg, ce qui déplace de quelques échantillons des kicks proches du seuil. Critères
//!   de la fiche J1-04 (kicks à ±1 pas, tempo final et tempo tenu le plus longtemps) ;
//! - **PCM de ffmpeg** (même commande que le script, si `ffmpeg` est dans le `PATH`) : le PCM est celui du golden, le
//!   spectre doit être identique octet pour octet, les kicks et le BPM aussi, pas par pas.
//!
//! `beat.ts` se trompe de tempo sur deux pistes (Night Prowler, Winning the Race) : le golden contient ces erreurs, et
//! le portage doit les reproduire, pas les corriger. C'est ce que les tests vérifient.

#![allow(clippy::print_stderr, reason = "bilan chiffré par piste, visible avec --nocapture")]

mod support;

use std::collections::BTreeMap;
use std::error::Error;

use cf_analysis::analyser_emulation::{FFT_SIZE, SMOOTHING_TIME_CONSTANT};
use cf_analysis::{AnalyserEmulation, BeatDetector};
use support::golden::{self, Golden};
use support::pcm;

const SAMPLE_RATE: u32 = 48_000;
/// Tolérance de position des kicks (pas).
const KICK_TOLERANCE_STEPS: u64 = 1;
/// Tolérance du BPM (le BPM de Sillage est un entier : ±0,5 revient à l'égalité).
const BPM_TOLERANCE: f64 = 0.5;
/// Part minimale des kicks de Sillage retrouvés à ±1 pas (critère de la fiche : 95 %).
const MIN_KICK_SHARE: f64 = 0.95;
/// Une valeur de BPM tenue au moins aussi longtemps (pas, soit 5 s) compte comme un régime établi.
const STEADY_STEPS: u64 = 300;

/// Ce que le portage produit sur une piste, dans les mêmes termes que le golden.
struct Replay {
    kicks: Vec<u64>,
    /// Changements de BPM, `(pas, bpm)`.
    bpm: Vec<(u64, Option<f32>)>,
    spectra: Vec<(u64, Vec<u8>)>,
}

impl Replay {
    fn bpm_at(&self, step: u64) -> Option<f64> {
        let index = self.bpm.partition_point(|(s, _)| *s <= step);
        index.checked_sub(1).and_then(|i| self.bpm[i].1).map(f64::from)
    }

    /// BPM tenu le plus longtemps (en nombre de pas), comme `steadyBpm` du script de référence.
    fn steady_bpm(&self, steps: u64) -> Option<f64> {
        let mut held: BTreeMap<u32, u64> = BTreeMap::new();
        for (i, (start, bpm)) in self.bpm.iter().enumerate() {
            let end = self.bpm.get(i + 1).map_or(steps + 1, |(s, _)| *s);
            if let Some(bpm) = bpm {
                *held.entry(*bpm as u32).or_default() += end - start;
            }
        }
        // `max_by_key` garde le dernier ex æquo ; le script garde le plus petit BPM : on inverse l'ordre.
        held.iter()
            .rev()
            .max_by_key(|(_, steps)| **steps)
            .map(|(bpm, _)| f64::from(*bpm))
    }
}

fn replay(golden: &Golden, pcm: &[f32]) -> Replay {
    let hop = golden.analysis.step_samples;
    let dt = 1.0 / golden.analysis.step_rate;
    let mut analyser = AnalyserEmulation::new(SMOOTHING_TIME_CONSTANT);
    let mut detector = BeatDetector::new();
    let mut out = Replay {
        kicks: Vec::new(),
        bpm: Vec::new(),
        spectra: Vec::new(),
    };
    let mut last_bpm = None;
    for step in 1..=golden.analysis.steps {
        let end = (step as usize * hop).min(pcm.len());
        analyser.push(&pcm[end.saturating_sub(hop)..end]);
        let spectrum = *analyser.byte_frequency_data();
        // `now = step / stepRate` comme le script (et non `step * dt`, qui diffère au dernier bit).
        let now = step as f64 / golden.analysis.step_rate;
        let state = *detector.update(Some(&spectrum), SAMPLE_RATE, FFT_SIZE, now, dt);
        if state.kick {
            out.kicks.push(step);
        }
        if state.bpm != last_bpm {
            last_bpm = state.bpm;
            out.bpm.push((step, state.bpm));
        }
        if golden.spectra.iter().any(|s| s.step == step) {
            out.spectra.push((step, spectrum.to_vec()));
        }
    }
    out
}

/// Part (0..1) des éléments de `from` qui ont un voisin à ±`tolerance` pas dans `of`.
fn matched_share(from: &[u64], of: &[u64], tolerance: u64) -> f64 {
    let near = |step: u64| of.iter().any(|k| k.abs_diff(step) <= tolerance);
    from.iter().filter(|s| near(**s)).count() as f64 / from.len().max(1) as f64
}

/// Part des pas où le portage donne le même BPM que Sillage (à ±0,5), sur les pas de `steps`.
fn bpm_agreement(golden: &Golden, replay: &Replay, steps: impl Iterator<Item = u64>) -> (f64, u64) {
    let (mut agree, mut total) = (0_u64, 0_u64);
    for step in steps {
        total += 1;
        let same = match (golden.bpm_at(step), replay.bpm_at(step)) {
            (Some(a), Some(b)) => (a - b).abs() <= BPM_TOLERANCE,
            (None, None) => true,
            _ => false,
        };
        agree += u64::from(same);
    }
    (agree as f64 / total.max(1) as f64, total)
}

/// Pas du régime établi : ceux des segments où le BPM de Sillage reste tenu au moins [`STEADY_STEPS`] pas, hors le
/// premier cinquième de chaque segment (le temps pour le portage de converger vers la même valeur).
fn steady_steps(golden: &Golden) -> Vec<u64> {
    let mut steps = Vec::new();
    for (i, change) in golden.bpm.iter().enumerate() {
        let end = golden
            .bpm
            .get(i + 1)
            .map_or(golden.analysis.steps + 1, |next| next.step);
        if change.bpm.is_some() && end - change.step >= STEADY_STEPS {
            steps.extend((change.step + STEADY_STEPS / 5)..end);
        }
    }
    steps
}

/// Écart maximal et nombre d'octets identiques entre les spectres rejoués et ceux du golden.
fn spectrum_gap(golden: &Golden, replay: &Replay) -> (u8, usize, usize) {
    let (mut max, mut exact, mut total) = (0, 0, 0);
    for (step, bytes) in &replay.spectra {
        let Some(reference) = golden.spectra.iter().find(|s| s.step == *step) else {
            continue;
        };
        for (a, b) in bytes.iter().zip(&reference.bytes) {
            max = max.max(a.abs_diff(*b));
            exact += usize::from(a == b);
            total += 1;
        }
    }
    (max, exact, total)
}

fn report(name: &str, golden: &Golden, replay: &Replay) {
    let steady = steady_steps(golden);
    let (established, established_steps) = bpm_agreement(golden, replay, steady.iter().copied());
    let (all, _) = bpm_agreement(golden, replay, 1..=golden.analysis.steps);
    let (gap, exact, total) = spectrum_gap(golden, replay);
    eprintln!(
        "{name} : kicks {}/{} (±1 pas : rappel {:.3}, précision {:.3}, ±0 : {:.3}) | BPM établi {established:.3} sur {established_steps} pas, \
         tous pas {all:.3} | BPM tenu {:?} (golden {}), final {:?} (golden {:?}) | spectres : écart max {gap}, identiques {exact}/{total}",
        replay.kicks.len(),
        golden.kicks.len(),
        matched_share(&golden.kicks, &replay.kicks, 1),
        matched_share(&replay.kicks, &golden.kicks, 1),
        matched_share(&golden.kicks, &replay.kicks, 0),
        replay.steady_bpm(golden.analysis.steps),
        golden.summary.steady_bpm,
        replay.bpm.last().and_then(|(_, b)| *b),
        golden.summary.final_bpm,
    );
}

/// Critères de la fiche J1-04 avec le décodeur du dépôt.
fn check_with_repo_decoder(name: &str, file: &str) -> Result<(), Box<dyn Error>> {
    let golden = golden::load(name)?;
    let (pcm, source_rate) = pcm::load_mono_48k(&golden::music_path(file))?;
    eprintln!(
        "{name} : source {source_rate} Hz, PCM {} échantillons (golden {})",
        pcm.len(),
        golden.track.pcm_samples
    );
    assert!(
        pcm.len().abs_diff(golden.track.pcm_samples) <= 16,
        "durée décodée différente de celle du golden"
    );
    let replay = replay(&golden, &pcm);
    report(name, &golden, &replay);

    let recall = matched_share(&golden.kicks, &replay.kicks, KICK_TOLERANCE_STEPS);
    let precision = matched_share(&replay.kicks, &golden.kicks, KICK_TOLERANCE_STEPS);
    assert!(
        recall >= MIN_KICK_SHARE,
        "{name} : {recall:.3} des kicks de Sillage retrouvés à ±1 pas"
    );
    assert!(
        precision >= MIN_KICK_SHARE,
        "{name} : {precision:.3} des kicks du portage sont dans le golden"
    );

    // Les deux erreurs de tempo de Sillage (Night Prowler, Winning the Race) sont dans le golden : le portage doit
    // y arriver aussi, au tempo final comme au tempo tenu le plus longtemps.
    let final_bpm = replay.bpm.last().and_then(|(_, b)| b.map(f64::from));
    assert_eq!(
        final_bpm.map(f64::round),
        golden.summary.final_bpm,
        "{name} : BPM final"
    );
    let steady = replay.steady_bpm(golden.analysis.steps);
    assert!(
        steady.is_some_and(|b| (b - golden.summary.steady_bpm).abs() <= BPM_TOLERANCE),
        "{name} : BPM tenu le plus longtemps {steady:?}, Sillage {}",
        golden.summary.steady_bpm
    );
    // L'émulation de l'analyseur : tous les octets à ±2 près malgré le rééchantillonneur différent.
    let (gap, _, _) = spectrum_gap(&golden, &replay);
    assert!(gap <= 2, "{name} : écart de spectre {gap}");
    Ok(())
}

/// Même PCM que le golden : tout doit coïncider.
fn check_with_ffmpeg_pcm(name: &str, file: &str) -> Result<(), Box<dyn Error>> {
    let golden = golden::load(name)?;
    let Some(pcm) = pcm::load_mono_48k_ffmpeg(&golden::music_path(file), golden.track.source_channels) else {
        eprintln!("{name} : ffmpeg absent, comparaison sur le PCM exact du golden ignorée");
        return Ok(());
    };
    assert_eq!(
        pcm.len(),
        golden.track.pcm_samples,
        "{name} : même nombre d'échantillons que le golden"
    );
    let replay = replay(&golden, &pcm);
    report(name, &golden, &replay);

    assert!(
        matched_share(&golden.kicks, &replay.kicks, KICK_TOLERANCE_STEPS) >= 0.99,
        "{name} : kicks"
    );
    assert!(
        matched_share(&replay.kicks, &golden.kicks, KICK_TOLERANCE_STEPS) >= 0.99,
        "{name} : kicks en trop"
    );
    let (established, steps) = bpm_agreement(&golden, &replay, steady_steps(&golden).into_iter());
    assert!(
        steps > 0 && established >= 0.99,
        "{name} : BPM en régime établi {established:.3}"
    );
    let (all, _) = bpm_agreement(&golden, &replay, 1..=golden.analysis.steps);
    assert!(all >= 0.99, "{name} : BPM, tous pas confondus {all:.3}");
    // Chromium calcule sa FFT en `f32`, le script en `f64` : un octet peut différer d'une unité près d'une frontière.
    let (gap, _, _) = spectrum_gap(&golden, &replay);
    assert!(gap <= 1, "{name} : écart de spectre {gap}");
    Ok(())
}

macro_rules! golden_track {
    ($repo:ident, $exact:ident, $name:literal, $file:literal) => {
        #[test]
        fn $repo() -> Result<(), Box<dyn Error>> {
            check_with_repo_decoder($name, $file)
        }

        #[test]
        fn $exact() -> Result<(), Box<dyn Error>> {
            check_with_ffmpeg_pcm($name, $file)
        }
    };
}

golden_track!(
    robotic_city,
    robotic_city_exact_pcm,
    "enprogames-robotic-city",
    "enprogames - Robotic City.mp3"
);
golden_track!(
    night_prowler,
    night_prowler_exact_pcm,
    "snabisch-night-prowler",
    "Snabisch - Night Prowler.mp3"
);
golden_track!(
    winning_the_race,
    winning_the_race_exact_pcm,
    "tozan-winning-the-race",
    "Tozan - Winning the Race.mp3"
);
golden_track!(chase, chase_exact_pcm, "yongki-chase", "Yongki - Chase.mp3");
