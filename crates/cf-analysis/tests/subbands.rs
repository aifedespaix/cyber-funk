//! Sous-bandes (C1) : séparation `SB`/`PB` d'un sinus à 40 et 100 Hz, équilibre sur bruit rose, transitoires.

use cf_analysis::analyser_emulation::{AnalyserEmulation, smoothing_for_step};
use cf_analysis::{SubBandAnalyzer, SubBandState};
use cf_audio::feed::HOP;

const SR: u32 = 48_000;
const DT: f32 = HOP as f32 / SR as f32;

/// Pilote une source de `steps` pas à travers l'analyseur émulé et les sous-bandes ; renvoie l'état final.
fn run(steps: usize, mut sample: impl FnMut(usize) -> f32) -> SubBandState {
    let mut analyser = AnalyserEmulation::new(smoothing_for_step(f64::from(DT)));
    let mut bands = SubBandAnalyzer::new(SR);
    let mut n = 0;
    let mut hop = [0.0_f32; HOP];
    for _ in 0..steps {
        for h in &mut hop {
            *h = sample(n);
            n += 1;
        }
        analyser.push(&hop);
        let bytes = *analyser.byte_frequency_data();
        bands.update(&hop, &bytes, DT);
    }
    *bands.state()
}

fn sine(hz: f32, amp: f32) -> impl FnMut(usize) -> f32 {
    move |n| amp * (std::f32::consts::TAU * hz * n as f32 / SR as f32).sin()
}

fn db(ratio: f32) -> f32 {
    20.0 * ratio.log10()
}

#[test]
fn sub_and_punch_separate_a_40_hz_and_a_100_hz_sine() {
    let low = run(300, sine(40.0, 0.5)).bands;
    let high = run(300, sine(100.0, 0.5)).bands;
    assert!(db(low.sb / low.pb) > 6.0, "40 Hz : SB {} PB {}", low.sb, low.pb);
    assert!(db(high.pb / high.sb) > 6.0, "100 Hz : SB {} PB {}", high.sb, high.pb);
    // Niveau : un sinus de 0,5 d'amplitude à la fréquence centrale vaut 0,35 RMS (gain crête 0 dB).
    assert!((low.sb - 0.354).abs() < 0.06, "{}", low.sb);
}

#[test]
fn fft_bands_read_rms_amplitude() {
    // Étalonnage de FFT_TO_RMS : un sinus de 0,5 à 1 kHz doit sortir en `HM` ≈ 0,354 (±25 %, la fenêtre étale).
    let hm = run(300, sine(1000.0, 0.5)).bands;
    assert!((hm.hm - 0.354).abs() < 0.09, "HM = {}", hm.hm);
    assert!(hm.lm < hm.hm * 0.1 && hm.tr < hm.hm * 0.1, "{hm:?}");
    let lm = run(300, sine(300.0, 0.5)).bands;
    assert!(lm.lm > lm.hm * 5.0, "{lm:?}");
    let tr = run(300, sine(9000.0, 0.5)).bands;
    assert!(tr.tr > tr.hm * 5.0 && tr.tr > 0.2, "{tr:?}");
}

/// Bruit rose de Voss-McCartney (déterministe) : énergie égale par octave.
fn pink() -> impl FnMut(usize) -> f32 {
    let mut seed = 0x1234_5678_u32;
    let mut rows = [0.0_f32; 16];
    let mut sum = 0.0_f32;
    move |n| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let row = (n.trailing_zeros() as usize).min(15);
        let white = (seed as f32 / u32::MAX as f32) - 0.5;
        sum += white - rows[row];
        rows[row] = white;
        sum * 0.12
    }
}

#[test]
fn pink_noise_gives_balanced_bands() {
    let s = run(900, pink());
    let b = s.bands;
    // Densité spectrale en 1/f : l'énergie par octave est égale. SB/PB/LM/HM/TR couvrent 1,6 / 1,3 / 1,7 / 3 / 1,4
    // octave ; on exige donc des RMS dans un rapport de ±8 dB autour de leur médiane, pas plus.
    let levels = [b.sb, b.pb, b.lm, b.hm, b.tr];
    let mut sorted = levels;
    sorted.sort_by(f32::total_cmp);
    let median = sorted[2];
    for (name, l) in ["SB", "PB", "LM", "HM", "TR"].iter().zip(levels) {
        assert!(l > 0.0, "{name} muet");
        assert!(db(l / median).abs() < 8.0, "{name} = {l} contre médiane {median} : {levels:?}");
    }
}

#[test]
fn a_hat_makes_a_transient_and_a_steady_tone_does_not() {
    let mut bands = SubBandAnalyzer::new(SR);
    let mut analyser = AnalyserEmulation::new(smoothing_for_step(f64::from(DT)));
    let mut tone = sine(220.0, 0.3);
    let mut hat_noise = pink();
    let mut n = 0;
    let mut hop = [0.0_f32; HOP];
    let mut fired = 0;
    let mut early = 0.0_f32;
    for step in 0..900 {
        for h in &mut hop {
            let mut v = tone(n);
            if (600..604).contains(&step) {
                v += 3.6 * hat_noise(n);
            }
            *h = v;
            n += 1;
        }
        analyser.push(&hop);
        let bytes = *analyser.byte_frequency_data();
        let s = *bands.update(&hop, &bytes, DT);
        // Les 150 premiers pas sont la mise en route : l'attaque du son tenu est, elle, un vrai transitoire.
        if (150..590).contains(&step) {
            early = early.max(s.transient);
        } else if step >= 590 && s.transient > 0.99 {
            fired += 1;
        }
    }
    assert!(early < 0.5, "un son tenu ne déclenche rien : {early}");
    assert!(fired >= 1, "le transitoire est vu");
}

#[test]
fn spectrum_has_32_bands_and_follows_the_tone() {
    let s = run(300, sine(1000.0, 0.5));
    let max = s.spectrum.iter().copied().enumerate().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap();
    // 1 kHz tombe à ln(10)/ln(160)·32 ≈ 14,5 : bande 14 ou 15.
    assert!((13..=16).contains(&max.0), "pic en bande {} : {:?}", max.0, s.spectrum);
    assert!(s.spectrum.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn rest_and_reset_return_to_zero() {
    let mut bands = SubBandAnalyzer::new(SR);
    let hop = [0.5_f32; HOP];
    bands.update(&hop, &[100; 1024], DT);
    bands.rest();
    assert_eq!(*bands.state(), SubBandState::REST);
}
