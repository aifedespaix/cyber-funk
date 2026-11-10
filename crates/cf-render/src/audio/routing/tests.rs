//! Tests de la matrice et du moteur de routage : validation, indépendance de la cadence, bornes, limiteur.

use cf_analysis::{AudioFeatures, SubBands};
use cf_show::limiter::{MAX_FLASHES_PER_SECOND, MIN_FADE};

use super::*;

fn features(f: impl FnOnce(&mut AudioFeatures)) -> AudioFeatures {
    let mut a = AudioFeatures {
        live: true,
        bpm: Some(128.0),
        ..AudioFeatures::NEUTRAL
    };
    f(&mut a);
    a
}

#[test]
fn default_matrix_is_valid_and_covers_every_target() {
    let matrix = RoutingMatrix::default_matrix();
    assert!(matrix.validate().is_ok());
    for target in Target::ALL {
        assert!(
            matrix.routes.iter().any(|r| r.target == target),
            "cible sans route : {target:?}"
        );
    }
    // Les index sont ceux de l'ordre de l'énumération.
    for (i, target) in Target::ALL.iter().enumerate() {
        assert_eq!(target.index(), i);
    }
}

#[test]
fn default_matrix_keeps_the_constants_of_p2() {
    let matrix = RoutingMatrix::default_matrix();
    let find = |source, target| {
        matrix
            .routes
            .iter()
            .find(|r| r.source == source && r.target == target)
            .unwrap()
    };
    // SB, géométrie : 30/350 ms, ressort 3 Hz, γ = 1,6.
    let sb = find(Source::SubBass, Target::MacroGeometry);
    assert_eq!(sb.curve, Curve::Pow(1.6));
    assert!(matches!(
        sb.smooth,
        Smooth::AttackReleaseSpring { attack_s, release_s, hz, .. }
            if (attack_s - 0.03).abs() < 1e-6 && (release_s - 0.35).abs() < 1e-6 && (hz - 3.0).abs() < 1e-6
    ));
    // PB, géométrie : 8/180 ms, ressort 5 Hz, γ = 1,4.
    let pb = find(Source::Punch, Target::KickMass);
    assert_eq!(pb.curve, Curve::Pow(1.4));
    assert!(matches!(pb.smooth, Smooth::AttackReleaseSpring { hz, .. } if (hz - 5.0).abs() < 1e-6));
    // TR, émissif : 2/90 ms, γ = 0,8. Les flashs sont sous limiteur.
    let tr = find(Source::Transient, Target::ParticleRate);
    assert!(matches!(tr.smooth, Smooth::AttackRelease { attack_s, release_s }
        if (attack_s - 0.002).abs() < 1e-6 && (release_s - 0.09).abs() < 1e-6));
    assert_eq!(find(Source::Transient, Target::LaserFlash).limiter, LimiterLane::Flash);
    assert_eq!(
        find(Source::Transient, Target::LaserFlash).quantize,
        Quantize::Sixteenth
    );
    assert_eq!(find(Source::Transient, Target::Glitch).limiter, LimiterLane::Flash);
    assert_eq!(find(Source::SubBass, Target::Bloom).limiter, LimiterLane::Sweep);
}

#[test]
fn the_matrix_round_trips_through_ron() {
    let matrix = RoutingMatrix::default_matrix();
    let text = ron::ser::to_string(&matrix).unwrap();
    assert_eq!(RoutingMatrix::from_ron(&text).unwrap(), matrix);
}

#[test]
fn invalid_matrices_are_refused() {
    const FIELDS: [(&str, &str); 10] = [
        ("source", "SubBass"),
        ("detector", "Envelope"),
        ("curve", "Linear"),
        ("smooth", "None"),
        ("target", "Bloom"),
        ("gain", "1.0"),
        ("clamp", "(0.0, 1.0)"),
        ("slew", "(up: 1.0, down: 1.0)"),
        ("quantize", "None"),
        ("limiter", "None"),
    ];
    let matrix = |key: &str, value: &str| {
        let fields: Vec<String> = FIELDS
            .iter()
            .map(|&(k, v)| format!("{k}: {}", if k == key { value } else { v }))
            .collect();
        format!("(routes: [Route({})])", fields.join(", "))
    };
    assert!(RoutingMatrix::from_ron(&matrix("source", "SubBass")).is_ok());
    let bad = [
        ("detector", "Spectrum(bins: 16)"),
        ("curve", "Pow(0.0)"),
        ("curve", "Pow(-2.0)"),
        ("smooth", "AttackRelease(attack_s: -1.0, release_s: 0.1)"),
        ("smooth", "Spring(hz: 0.0, zeta: 0.5)"),
        ("smooth", "OneEuro(f_min: 0.0, beta: 0.5)"),
        ("gain", "-1.0"),
        ("gain", "99.0"),
        ("clamp", "(1.0, 0.0)"),
        ("slew", "(up: 0.0, down: 1.0)"),
        ("detector", "Peak(decay_s: 1000.0, hold_s: 0.0)"),
        ("detector", "Flux(z: 100.0)"),
    ];
    for (key, value) in bad {
        let error = RoutingMatrix::from_ron(&matrix(key, value)).unwrap_err();
        assert!(
            matches!(error, RoutingError::Invalid { .. }),
            "{key}: {value} accepté ou mal refusé : {error}"
        );
    }
    assert!(matches!(
        RoutingMatrix::from_ron("(routes: [oups"),
        Err(RoutingError::Parse(_))
    ));
    let many = RoutingMatrix {
        routes: vec![RoutingMatrix::default_matrix().routes[0].clone(); matrix::MAX_ROUTES + 1],
    };
    assert!(matches!(many.validate(), Err(RoutingError::TooMany(_))));
}

/// Fait tourner le moteur `seconds` à `rate` i/s sur un signal fonction du temps ; rend la valeur d'une cible à chaque image.
fn run(
    rate: u32,
    seconds: f32,
    target: Target,
    mut make: impl FnMut(f32) -> AudioFeatures,
    policy: (bool, bool),
) -> Vec<f32> {
    // Une seule cible à la fois : deux routes sous la même voie du limiteur se disputeraient ses départs.
    let mut matrix = RoutingMatrix::default_matrix();
    matrix.routes.retain(|r| r.target == target);
    let mut e = RoutingEngine::new(&matrix);
    let mut out = FilteredSignals::default();
    let dt = 1.0 / rate as f32;
    (0..(seconds * rate as f32) as u32)
        .map(|i| {
            let t = i as f32 * dt;
            e.update(&mut out, &make(t), dt, f64::from(t), 0.0, policy);
            out.get(target)
        })
        .collect()
}

fn loud_sub_bass(_t: f32) -> AudioFeatures {
    features(|a| {
        a.sub_bands = SubBands {
            sb: 0.5,
            pb: 0.0,
            lm: 0.0,
            hm: 0.0,
            tr: 0.0,
        }
    })
}

#[test]
fn step_response_does_not_depend_on_the_frame_rate() {
    for target in [Target::EmissiveBass, Target::FogDispersion, Target::MacroGeometry] {
        let at =
            |rate: u32, t: f32| run(rate, 2.0, target, loud_sub_bass, (true, false))[(t * rate as f32) as usize - 1];
        for t in [1.0_f32, 2.0] {
            let reference = at(60, t);
            assert!(reference > 0.5, "{target:?} réagit : {reference}");
            for rate in [30, 144] {
                let v = at(rate, t);
                assert!(
                    (v - reference).abs() < 0.01 * reference.max(1.0),
                    "{target:?} à t = {t} : {v} à {rate} i/s contre {reference} à 60 i/s"
                );
            }
        }
    }
}

#[test]
fn outputs_stay_inside_their_clamp_and_finite() {
    let matrix = RoutingMatrix::default_matrix();
    let mut e = RoutingEngine::new(&matrix);
    let mut out = FilteredSignals::default();
    let mut seed = 7_u32;
    let mut rnd = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / (1 << 24) as f32
    };
    for i in 0..4000 {
        let a = features(|a| {
            a.sub_bands = SubBands {
                sb: rnd(),
                pb: rnd(),
                lm: rnd(),
                hm: rnd(),
                tr: rnd(),
            };
            a.z_pb = rnd() * 20.0 - 4.0;
            a.z_tr = rnd() * 20.0 - 4.0;
            a.hfc_z = rnd() * 20.0 - 4.0;
            a.phase = rnd();
            a.intensity = rnd();
            a.flux = rnd();
            a.spectrum = std::array::from_fn(|_| rnd());
        });
        e.update(&mut out, &a, 1.0 / 60.0, f64::from(i) / 60.0, 0.05, (true, false));
        for target in Target::ALL {
            let v = out.get(target);
            let (lo, hi) = matrix
                .routes
                .iter()
                .filter(|r| r.target == target)
                .fold((f32::MAX, f32::MIN), |(lo, hi), r| {
                    (lo.min(r.clamp.0), hi.max(r.clamp.1))
                });
            assert!(
                v.is_finite() && v >= lo - 1e-4 && v <= hi + 1e-4,
                "{target:?} = {v} hors [{lo}, {hi}] à l'image {i}"
            );
        }
    }
}

/// Un hi-hat à chaque double croche à 128 BPM, sur une mesure qui tourne.
fn hats(t: f32) -> AudioFeatures {
    let bar_s = 4.0 * 60.0 / 128.0;
    features(|a| {
        a.phase = (t / bar_s).fract();
        a.z_tr = if (t * 16.0 / bar_s).fract() < 0.3 { 9.0 } else { 0.0 };
        a.hfc_z = a.z_tr;
        a.sub_bands.tr = 0.2;
    })
}

#[test]
fn flash_routes_never_exceed_the_limiter_rate_or_slope() {
    let rate = 60;
    for target in [Target::LaserFlash, Target::Glitch] {
        let series = run(rate, 20.0, target, hats, (true, false));
        assert!(series.iter().any(|&v| v > 0.5), "{target:?} part au moins une fois");
        // Pente : jamais plus vite qu'une course complète en MIN_FADE.
        let step = 1.0 / rate as f32 / MIN_FADE + 1e-4;
        for w in series.windows(2) {
            assert!(
                (w[1] - w[0]).abs() <= step,
                "{target:?} : saut de {} en une image",
                w[1] - w[0]
            );
        }
        // Cadence : au plus MAX_FLASHES_PER_SECOND départs sur toute seconde glissante.
        let starts: Vec<usize> = series
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w[0] < 0.05 && w[1] >= 0.05)
            .map(|(i, _)| i)
            .collect();
        for (k, &s) in starts.iter().enumerate() {
            let in_window = starts[k..].iter().take_while(|&&o| o < s + rate as usize).count();
            assert!(
                in_window <= MAX_FLASHES_PER_SECOND,
                "{target:?} : {in_window} départs en une seconde"
            );
        }
    }
}

#[test]
fn flash_routes_are_silent_when_flashes_are_off_or_breathing() {
    for policy in [(false, false), (true, true)] {
        for target in [Target::LaserFlash, Target::Glitch, Target::Bloom] {
            let series = run(
                60,
                10.0,
                target,
                |t| {
                    let mut a = hats(t);
                    a.sub_bands.sb = 0.5;
                    a.sub_bands.pb = 0.5;
                    a
                },
                policy,
            );
            assert!(
                series.iter().all(|&v| v == 0.0),
                "{target:?} brille malgré {policy:?} : première {:?}",
                series.iter().position(|&v| v != 0.0).map(|i| (i, series[i]))
            );
        }
    }
}

#[test]
fn quantized_events_wait_for_the_grid() {
    // Un seul front de flux au milieu d'une double croche : le laser attend la frontière suivante.
    let bar_s = 4.0 * 60.0 / 128.0;
    let cell_s = bar_s / 16.0;
    let series = run(
        240,
        2.0,
        Target::LaserFlash,
        |t| {
            features(|a| {
                a.phase = (t / bar_s).fract();
                // Front de 8 ms posé à t = 4,5 cellules.
                a.z_tr = if (4.5 * cell_s..4.5 * cell_s + 0.008).contains(&t) {
                    9.0
                } else {
                    0.0
                };
            })
        },
        (true, false),
    );
    let first = series.iter().position(|&v| v > 0.01).expect("le laser part");
    let t_first = first as f32 / 240.0;
    assert!(
        t_first >= 5.0 * cell_s - 1.0 / 240.0,
        "parti à {t_first} s avant la frontière {} s",
        5.0 * cell_s
    );
    assert!(
        t_first <= 5.0 * cell_s + 3.0 / 240.0,
        "parti à {t_first} s, trop tard après {} s",
        5.0 * cell_s
    );
}

#[test]
fn kick_wave_keeps_the_last_four_ages() {
    let mut w = KickWave::default();
    assert_eq!(w.radius(0, 10.0), -1.0, "aucun kick : aucune onde");
    for _ in 0..5 {
        w.push();
        w.age(0.1);
    }
    let ages = w.ages();
    for (i, expected) in [0.1, 0.2, 0.3, 0.4].iter().enumerate() {
        assert!((ages[i] - expected).abs() < 1e-5, "{ages:?}");
    }
    assert!((w.radius(0, 10.0) - 1.0).abs() < 1e-4);
    w.age(1000.0);
    assert_eq!(w.ages(), [KICK_NEVER_S; 4], "les âges sont bornés");
}

#[test]
fn visual_phase_compensates_the_latency_and_wraps() {
    assert!((visual_phase(0.9, 0.1, 1.0) - 0.0).abs() < 1e-5);
    assert!((visual_phase(0.5, 0.0, 2.0) - 0.5).abs() < 1e-6);
    assert!((visual_phase(f32::NAN, f32::NAN, 1.0)).abs() < 1e-6);
    assert!(
        (visual_phase(0.2, -1.0, 1.0) - 0.2).abs() < 1e-6,
        "une latence négative est ignorée"
    );
}
