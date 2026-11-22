//! Tests de l'analyse sur des traces synthétiques : un kick toutes les 0,5 s, une réponse lumineuse en triangle.

use super::*;

const DT: f64 = 0.01;

/// Trace de `seconds` s à 100 images/s. Le kick `i` tombe à `i × 0,5 s` ; sa lumière culmine `delay_ms(i)` plus tard
/// (`None` : l'image ne réagit pas à ce kick). La confiance monte linéairement de 0 à 1 sur `trust_ramp_s`.
fn trace(seconds: f64, delay_ms: impl Fn(usize) -> Option<f64>, trust_ramp_s: f64) -> SyncTrace {
    let frames = (seconds / DT).round() as usize;
    let kick_frames: Vec<usize> = (0..frames)
        .map(|i| (i as f64 * 0.5 / DT).round() as usize)
        .take_while(|f| *f < frames)
        .collect();
    let samples = (0..frames)
        .map(|f| {
            let t_s = f as f64 * DT;
            let luminance = kick_frames
                .iter()
                .enumerate()
                .filter_map(|(i, kf)| {
                    let peak_at = *kf as f64 * DT + delay_ms(i)? / 1000.0;
                    Some((1.0 - (t_s - peak_at).abs() / 0.05).max(0.0))
                })
                .fold(0.0_f64, f64::max);
            SyncSample {
                t_s,
                kick_now: kick_frames.contains(&f),
                beat_phase: 0.0,
                grid_trust: (t_s / trust_ramp_s).min(1.0) as f32,
                luminance: luminance as f32,
            }
        })
        .collect();
    SyncTrace { samples }
}

fn alternating(i: usize) -> f64 {
    if i.is_multiple_of(2) { 40.0 } else { 60.0 }
}

#[test]
fn retrouve_un_retard_connu() {
    let report = analyze(&trace(10.0, |_| Some(80.0), 2.0), &SyncThresholds::default());
    let mean = report.mean_delay_ms.unwrap();
    assert!((mean - 80.0).abs() <= 3.0, "retard trouvé : {mean}");
    assert_eq!(report.kicks, 20);
    assert_eq!(report.verdict, Verdict::TooLate);
}

#[test]
fn la_gigue_est_l_ecart_type_des_retards() {
    // Retards alternés 40 / 60 ms : moyenne 50, écart-type de population 10.
    let report = analyze(&trace(10.0, |i| Some(alternating(i)), 2.0), &SyncThresholds::default());
    let jitter = report.jitter_ms.unwrap();
    assert!((jitter - 10.0).abs() <= 3.0, "gigue trouvée : {jitter}");
    assert_eq!(report.verdict, Verdict::Ok);
    let strict = SyncThresholds {
        max_jitter_ms: 5.0,
        ..SyncThresholds::default()
    };
    assert_eq!(
        analyze(&trace(10.0, |i| Some(alternating(i)), 2.0), &strict).verdict,
        Verdict::TooJittery
    );
}

#[test]
fn une_derive_de_phase_est_detectee() {
    // Le retard passe de 10 à 40 ms en 30 s : 60 ms par minute.
    let report = analyze(
        &trace(30.0, |i| Some(10.0 + i as f64 * 0.5), 2.0),
        &SyncThresholds::default(),
    );
    let drift = report.phase_drift_ms_per_min.unwrap();
    assert!((drift - 60.0).abs() <= 6.0, "dérive trouvée : {drift}");
    assert_eq!(report.verdict, Verdict::Drifting);
    let steady = analyze(&trace(30.0, |_| Some(30.0), 2.0), &SyncThresholds::default());
    assert!(steady.phase_drift_ms_per_min.unwrap().abs() < 1.0);
    assert_eq!(steady.verdict, Verdict::Ok);
}

#[test]
fn sans_kick_le_verdict_est_pas_de_donnees() {
    let mut silent = trace(10.0, |_| Some(80.0), 2.0);
    for sample in &mut silent.samples {
        sample.kick_now = false;
    }
    let report = analyze(&silent, &SyncThresholds::default());
    assert_eq!(report.verdict, Verdict::NoData);
    assert_eq!(report.kicks, 0);
    assert_eq!(report.mean_delay_ms, None, "jamais un retard nul");
    assert_eq!(report.jitter_ms, None);
    assert_eq!(report.phase_drift_ms_per_min, None);
    let empty = analyze(&SyncTrace::default(), &SyncThresholds::default());
    assert_eq!(empty.verdict, Verdict::NoData);
    assert_eq!(empty.trust_rise_s, None);
}

#[test]
fn trust_rise_est_le_premier_instant_au_dessus_de_0_6() {
    // Rampe de 0 à 1 en 5 s : 0,6 est atteint à 3 s, et il faut le dépasser (> 0,6).
    let report = analyze(&trace(10.0, |_| Some(30.0), 5.0), &SyncThresholds::default());
    let rise = report.trust_rise_s.unwrap();
    assert!((rise - 3.0).abs() <= 0.02, "montée trouvée : {rise}");
    let never = analyze(&trace(2.0, |_| Some(30.0), 100.0), &SyncThresholds::default());
    assert_eq!(never.trust_rise_s, None);
}

#[test]
fn un_pic_de_luminance_manquant_ignore_ce_kick_sans_fausser_la_moyenne() {
    // Un kick sur deux n'allume rien : seuls les 10 autres comptent, à 50 ms.
    let report = analyze(
        &trace(10.0, |i| i.is_multiple_of(2).then_some(50.0), 2.0),
        &SyncThresholds::default(),
    );
    assert_eq!(report.kicks, 10);
    let mean = report.mean_delay_ms.unwrap();
    assert!((mean - 50.0).abs() <= 3.0, "retard trouvé : {mean}");
    assert_eq!(report.verdict, Verdict::Ok);
}
