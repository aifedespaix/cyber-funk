//! Densité visuelle `Δ` (C1) sur un morceau de club simulé : l'instantané de la courbe, les cinq états atteints, le
//! creux d'avant-drop borné par le limiteur, le contraste du climax et la respiration imposée.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use std::fmt::Write as _;

use cf_analysis::Section;
use cf_show::density::{DensityBand, DensityConductor, DensityInput, EPURE_LEVEL, PREDROP_LEVEL};
use cf_show::director::{ShowDirector, ShowInput};
use cf_show::limiter::MIN_FADE;
use cf_show::sim::SyntheticMusic;
use director_rig::{cue, event, slot, titles};
use rand_pcg::Pcg32;

const DT: f32 = 1.0 / 60.0;
const BPM: f32 = 125.0;

struct Sample {
    time_s: f64,
    delta: f32,
    band: DensityBand,
    predrop: bool,
    epure: bool,
    section: Section,
}

/// Le morceau de club (120 mesures) joué dans le vrai chef du spectacle, une image à 60 i/s.
fn club_run(flashes: bool) -> Vec<Sample> {
    let cues = cue("c0", "niveau-1-ville", "Cut", "Manual");
    let show = event(&slot("a", "Auto", "", &cues));
    let mut director: ShowDirector<Pcg32> = ShowDirector::new(&show, titles(), 5).unwrap();
    let mut music = SyntheticMusic::club(BPM, true);
    let flash_off = [cf_control::Action::FlashKill(cf_control::Switch::On)];
    let mut samples = Vec::new();
    let bar_s = 4.0 * 60.0 / f64::from(BPM);
    let frames = (120.0 * bar_s / f64::from(DT)) as u64;
    for i in 0..frames {
        let frame = music.frame(DT);
        let actions: &[cf_control::Action] = if !flashes && i == 0 { &flash_off } else { &[] };
        let state = director.update(&ShowInput {
            dt: DT,
            now: i as f64 * f64::from(DT),
            local_time_s: 0.0,
            audio: &frame.audio,
            real_drop: frame.real_drop,
            actions,
        });
        let d = state.density;
        samples.push(Sample {
            time_s: i as f64 * f64::from(DT),
            delta: d.delta,
            band: d.band,
            predrop: d.predrop,
            epure: d.epure,
            section: frame.audio.section,
        });
        let _ = director.drain_events().count();
    }
    samples
}

#[test]
fn density_curve_snapshot_and_the_five_states() {
    let samples = club_run(true);
    let bar_s = 4.0 * 60.0 / f64::from(BPM);
    let mut text = String::new();
    // Une ligne par mesure : Δ au début de la mesure, le plus bas et le plus haut de la mesure, l'état à la fin.
    for (bar, chunk) in samples.chunks((bar_s / f64::from(DT)).round() as usize).enumerate() {
        let lo = chunk.iter().map(|s| s.delta).fold(f32::MAX, f32::min);
        let hi = chunk.iter().map(|s| s.delta).fold(f32::MIN, f32::max);
        let last = chunk.last().unwrap();
        let _ = writeln!(
            text,
            "mesure {bar:3}  {:?}  Δ {:.2} [{lo:.2} .. {hi:.2}]  {:?}{}{}",
            last.section,
            chunk[0].delta,
            last.band,
            if chunk.iter().any(|s| s.predrop) {
                "  pré-drop"
            } else {
                ""
            },
            if chunk.iter().any(|s| s.epure) { "  épure" } else { "" },
        );
    }
    insta::assert_snapshot!(text);

    for band in [
        DensityBand::Apnea,
        DensityBand::Calm,
        DensityBand::Groove,
        DensityBand::Rise,
        DensityBand::Climax,
    ] {
        assert!(samples.iter().any(|s| s.band == band), "état jamais atteint : {band:?}");
    }
    assert!(samples.iter().all(|s| (0.0..=1.0).contains(&s.delta)));
}

#[test]
fn predrop_dips_are_bounded_by_the_limiter() {
    let samples = club_run(true);
    assert!(samples.iter().any(|s| s.predrop), "un creux d'avant-drop a lieu");
    // Δ ne varie jamais plus vite qu'une course complète en MIN_FADE.
    let max_step = DT / MIN_FADE + 1e-4;
    for w in samples.windows(2) {
        assert!(
            (w[1].delta - w[0].delta).abs() <= max_step,
            "saut de Δ de {} en une image",
            w[1].delta - w[0].delta
        );
    }
    // Le creux atteint bien le niveau d'avant-drop, jamais en dessous.
    let floor = samples
        .iter()
        .filter(|s| s.predrop)
        .map(|s| s.delta)
        .fold(f32::MAX, f32::min);
    assert!((PREDROP_LEVEL - 0.02..=PREDROP_LEVEL + 0.2).contains(&floor), "{floor}");
    // Jamais deux creux à moins d'une seconde l'un de l'autre.
    let mut last_end: Option<f64> = None;
    for w in samples.windows(2) {
        if !w[0].predrop
            && w[1].predrop
            && let Some(end) = last_end
        {
            assert!(
                w[1].time_s - end >= 1.0,
                "deux creux à {:.2} s d'écart",
                w[1].time_s - end
            );
        }
        if w[0].predrop && !w[1].predrop {
            last_end = Some(w[1].time_s);
        }
    }
}

#[test]
fn no_predrop_when_flashes_are_killed() {
    let samples = club_run(false);
    assert!(
        samples.iter().all(|s| !s.predrop),
        "aucun creux d'avant-drop sans flashs"
    );
}

#[test]
fn a_climax_is_worth_what_precedes_it() {
    let samples = club_run(true);
    // Moyenne de Δ sur les 8 mesures d'avant chaque drop, contre le maximum du corps du drop (8 mesures après).
    let bar = (4.0 * 60.0 / f64::from(BPM) / f64::from(DT)).round() as usize;
    // Les drops du morceau de club commencent aux mesures 24 et 80.
    for start in [24_usize, 80] {
        let before: f32 = samples[(start - 8) * bar..start * bar]
            .iter()
            .map(|s| s.delta)
            .sum::<f32>()
            / (8 * bar) as f32;
        let peak = samples[start * bar..(start + 8) * bar]
            .iter()
            .map(|s| s.delta)
            .fold(0.0, f32::max);
        assert!(
            peak / before >= 1.6,
            "drop de la mesure {start} : crête {peak:.2} pour {before:.2} avant"
        );
    }
}

fn input(section: Section, count: u32) -> DensityInput {
    DensityInput {
        dt: DT,
        now: 0.0,
        bpm: Some(120.0),
        section,
        intensity: 1.0,
        count,
        strike_in_beats: None,
        drop_now: false,
        flashes: true,
        breathing: false,
    }
}

#[test]
fn a_bar_of_rest_is_forced_after_64_beats_without_dropping_below_0_4() {
    let mut c = DensityConductor::default();
    let mut count = 0_u32;
    let mut t = 0.0_f64;
    let mut epure_start = None;
    let beat_s = 0.5;
    // Sommet ininterrompu à 120 BPM : un temps toutes les 0,5 s.
    for i in 0..(200.0 / f64::from(DT)) as u64 {
        t = i as f64 * f64::from(DT);
        count = (t / beat_s) as u32;
        let state = *c.update(&DensityInput {
            now: t,
            ..input(Section::Peak, count)
        });
        if state.epure && epure_start.is_none() {
            epure_start = Some((t, count));
        }
    }
    let (start, at_count) = epure_start.expect("une mesure d'épure est imposée");
    assert!(start > 64.0 * beat_s, "pas avant 64 temps : {start}");
    assert_eq!(at_count % 16, 0, "elle commence au début d'une phrase");
    assert!(c.state().delta <= 1.0 && count > 0 && t > 0.0);
    const { assert!(EPURE_LEVEL < 0.4) };
}

#[test]
fn fatigue_lowers_the_ceiling_after_32_high_beats() {
    let mut c = DensityConductor::default();
    let mut max_late = 0.0_f32;
    for i in 0..(150.0 / f64::from(DT)) as u64 {
        let t = i as f64 * f64::from(DT);
        // L'épure est repoussée en gardant `count` figé (aucune phrase ne commence) : on isole la fatigue.
        let s = c.update(&DensityInput {
            now: t,
            ..input(Section::Peak, 0)
        });
        if t > 60.0 {
            max_late = max_late.max(s.delta);
        }
    }
    assert!(max_late <= 0.71 + 0.02, "plafond après fatigue : {max_late}");
    assert!(max_late > 0.6);
}

#[test]
fn breathing_mode_caps_the_density() {
    let mut c = DensityConductor::default();
    let mut max = 0.0_f32;
    for i in 0..600 {
        let s = c.update(&DensityInput {
            now: f64::from(i) * f64::from(DT),
            breathing: true,
            ..input(Section::Peak, 0)
        });
        max = max.max(s.delta);
    }
    assert!(max <= c.tuning().breathing_cap + 1e-4, "{max}");
}

#[test]
fn bands_have_hysteresis() {
    assert_eq!(
        DensityBand::of(0.36, DensityBand::Calm),
        DensityBand::Calm,
        "reste dans la marge"
    );
    assert_eq!(DensityBand::of(0.38, DensityBand::Calm), DensityBand::Groove);
    assert_eq!(DensityBand::of(0.34, DensityBand::Groove), DensityBand::Groove);
    assert_eq!(DensityBand::of(0.32, DensityBand::Groove), DensityBand::Calm);
    assert_eq!(DensityBand::of(0.99, DensityBand::Apnea), DensityBand::Climax);
}
