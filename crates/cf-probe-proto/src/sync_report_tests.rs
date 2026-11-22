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
                wall_s: t_s,
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

#[test]
fn un_rapport_sim_sur_mural_a_un_ne_porte_aucune_note() {
    let report = analyze(&trace(10.0, |_| Some(30.0), 2.0), &SyncThresholds::default());
    let ratio = report.sim_to_wall_ratio.unwrap();
    assert!((ratio - 1.0).abs() < 1e-9, "{ratio}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
    assert_eq!(report.verdict, Verdict::Ok);
}

#[test]
fn un_temps_simule_decouple_du_mural_n_est_jamais_ok() {
    // Le temps mural court deux fois plus vite que le temps simulé : ratio 0,5.
    let mut slow = trace(10.0, |_| Some(30.0), 2.0);
    for sample in &mut slow.samples {
        sample.wall_s *= 2.0;
    }
    let report = analyze(&slow, &SyncThresholds::default());
    assert!((report.sim_to_wall_ratio.unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(report.verdict, Verdict::Unreliable);
    assert!(
        report
            .notes
            .iter()
            .any(|n| n.contains("pas de temps fixe découplé de l'horloge audio")),
        "{:?}",
        report.notes
    );
    // Les chiffres restent rendus, seul le verdict dégrade.
    assert!(report.mean_delay_ms.is_some());
}

#[test]
fn sans_horodatage_mural_le_ratio_est_inconnu_et_le_verdict_inchange() {
    let mut old = trace(10.0, |_| Some(30.0), 2.0);
    for sample in &mut old.samples {
        sample.wall_s = 0.0;
    }
    let report = analyze(&old, &SyncThresholds::default());
    assert_eq!(report.sim_to_wall_ratio, None);
    assert_eq!(report.verdict, Verdict::Ok);
}

#[test]
fn une_lueur_qui_decroit_sans_nouveau_pic_ne_compte_pas_comme_un_retard_nul() {
    // Un seul kick, la lumière redescend depuis un niveau haut : aucune montée, donc aucun retard à mesurer.
    let mut glow = trace(2.0, |_| None, 2.0);
    for (f, sample) in glow.samples.iter_mut().enumerate() {
        sample.luminance = (0.9 - f as f32 * 0.01).max(0.0);
        sample.kick_now = f == 50;
    }
    let report = analyze(&glow, &SyncThresholds::default());
    assert_eq!(report.kicks, 0);
    assert_eq!(report.verdict, Verdict::NoData);
}

#[test]
fn un_kick_unique_donne_un_retard_sans_gigue_ni_derive() {
    let mut one = trace(3.0, |_| Some(40.0), 2.0);
    for sample in &mut one.samples {
        sample.kick_now = sample.t_s.abs() < 1e-9;
    }
    let report = analyze(&one, &SyncThresholds::default());
    assert_eq!(report.kicks, 1);
    assert!((report.mean_delay_ms.unwrap() - 40.0).abs() <= 3.0);
    assert_eq!((report.jitter_ms, report.phase_drift_ms_per_min), (None, None));
}

#[test]
fn des_echantillons_non_finis_ou_non_tries_sont_ecartes_avec_une_note() {
    let mut bad = trace(10.0, |_| Some(30.0), 2.0);
    bad.samples[100].t_s = f64::NAN;
    bad.samples[200].luminance = f32::INFINITY;
    bad.samples[300].t_s = 0.5; // recule dans le temps
    let report = analyze(&bad, &SyncThresholds::default());
    assert!(report.mean_delay_ms.is_some_and(f64::is_finite), "{report:?}");
    assert!(report.notes.iter().any(|n| n.contains("écarté")), "{:?}", report.notes);
}

#[test]
fn le_ratio_ignore_le_temps_mort_avant_le_premier_echantillon() {
    // L'horloge murale a déjà 100 s au premier échantillon (attente avant la première image) : seul l'écart entre le
    // premier et le dernier échantillon compte.
    let mut late = trace(10.0, |_| Some(30.0), 2.0);
    for sample in &mut late.samples {
        sample.wall_s += 100.0;
    }
    let report = analyze(&late, &SyncThresholds::default());
    assert!((report.sim_to_wall_ratio.unwrap() - 1.0).abs() < 1e-9);
    assert_eq!(report.verdict, Verdict::Ok);
    assert_eq!(report.notes, Vec::<String>::new());
}

#[test]
fn la_vraie_trace_de_chase_est_mesuree_avec_un_retard_dans_l_image_du_kick() {
    // Extrait réel (Chase, temps réel, ~17 ms par image) : la luminance saute de 0,454 à 0,55 sur l'image même du
    // kick. Avant la correction, aucune montée n'était cherchée *avant* le pic et les kicks étaient tous écartés.
    let trace: SyncTrace = serde_json::from_str(include_str!("real_trace_fixture.json")).unwrap();
    let report = analyze(&trace, &SyncThresholds::default());
    assert!(report.kicks >= 3, "{report:?}");
    assert!(report.mean_delay_ms.unwrap().abs() < 1.0, "{report:?}");
    assert_eq!(report.verdict, Verdict::Ok, "{report:?}");
}
