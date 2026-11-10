//! Tests des filtres : réponse à un échelon indépendante de la cadence, déclencheurs.

use super::*;

const RATES: [u32; 3] = [30, 60, 144];

/// Valeur d'un filtre à `t` secondes après un échelon 0 → 1, pour une cadence donnée. `t` doit tomber sur un
/// instant d'image des trois cadences (multiple de 1/30, 1/60 et 1/144 s : 1/6 s ne l'est pas, 0,5 s l'est).
fn at(rate: u32, t: f32, mut step: impl FnMut(f32, f32) -> f32) -> f32 {
    let dt = 1.0 / rate as f32;
    let n = (t * rate as f32).round() as u32;
    let mut y = 0.0;
    for _ in 0..n {
        y = step(1.0, dt);
    }
    y
}

fn assert_rate_independent(name: &str, tolerance: f32, make: impl Fn() -> Box<dyn FnMut(f32, f32) -> f32>) {
    for t in [0.5_f32, 1.0, 2.0] {
        let values: Vec<f32> = RATES.iter().map(|&r| at(r, t, make())).collect();
        let reference = values[1];
        for (rate, v) in RATES.iter().zip(&values) {
            assert!(
                (v - reference).abs() <= tolerance * reference.abs().max(1.0),
                "{name} à t = {t} s : {v} à {rate} i/s contre {reference} à 60 i/s"
            );
        }
    }
}

#[test]
fn attack_release_step_response_is_rate_independent() {
    assert_rate_independent("attaque/relâchement", 0.01, || {
        let mut f = AttackRelease::new(0.2, 0.5);
        Box::new(move |x, dt| f.step(x, dt))
    });
}

#[test]
fn attack_release_matches_the_exponential() {
    let y = at(60, 0.5, {
        let mut f = AttackRelease::new(0.2, 0.5);
        move |x, dt| f.step(x, dt)
    });
    assert!((y - (1.0 - (-0.5_f32 / 0.2).exp())).abs() < 1e-4, "{y}");
}

#[test]
fn peak_hold_holds_then_decays_independently_of_rate() {
    // Crête à 1 puis silence : après le maintien (0,3 s), décroissance de constante 0,5 s.
    for t in [0.5_f32, 1.0, 2.0] {
        let values: Vec<f32> = RATES
            .iter()
            .map(|&rate| {
                let dt = 1.0 / rate as f32;
                let mut p = PeakHold::new(0.5, 0.3);
                p.step(1.0, dt);
                let n = (t * rate as f32).round() as u32;
                let mut y = 1.0;
                for _ in 0..n {
                    y = p.step(0.0, dt);
                }
                y
            })
            .collect();
        let expected = (-(t - 0.3) / 0.5_f32).exp();
        for (rate, v) in RATES.iter().zip(&values) {
            // L'échelon d'entrée est vu une image plus tôt ou plus tard selon la cadence : tolérance d'une image.
            let one_frame = (-(t - 0.3 - 1.0 / *rate as f32) / 0.5_f32).exp();
            assert!(
                *v <= one_frame + 1e-3 && *v >= expected - 0.02,
                "t = {t} s, {rate} i/s : {v} (attendu {expected})"
            );
        }
    }
    let mut hold = PeakHold::new(0.5, 0.3);
    hold.step(1.0, 0.01);
    for _ in 0..29 {
        assert!(
            (hold.step(0.0, 0.01) - 1.0).abs() < 1e-6,
            "aucune décroissance pendant le maintien"
        );
    }
}

#[test]
fn slew_limiter_ramps_at_a_fixed_rate() {
    assert_rate_independent("borne de pente", 0.005, || {
        let mut f = SlewLimiter::new(0.8, 0.8);
        Box::new(move |x, dt| f.step(x, dt))
    });
}

#[test]
fn critical_spring_step_response_is_exact_at_any_rate() {
    assert_rate_independent("ressort critique", 0.01, || {
        let mut s = Spring::new();
        Box::new(move |x, dt| s.step_critical(x, 3.0, dt))
    });
    // Forme close : y = 1 − (1 + ωt)·e^(−ωt).
    let w = std::f32::consts::TAU * 3.0;
    let y = at(30, 0.5, {
        let mut s = Spring::new();
        move |x, dt| s.step_critical(x, 3.0, dt)
    });
    assert!((y - (1.0 - (1.0 + w * 0.5) * (-w * 0.5).exp())).abs() < 1e-4, "{y}");
}

#[test]
fn underdamped_spring_overshoots_and_is_rate_independent() {
    assert_rate_independent("ressort sous-amorti", 0.01, || {
        let mut s = Spring::new();
        Box::new(move |x, dt| s.step(x, 2.0, 0.4, dt))
    });
    let mut s = Spring::new();
    let peak = (0..400)
        .map(|_| s.step(1.0, 2.0, 0.4, 1.0 / 60.0))
        .fold(0.0_f32, f32::max);
    // Dépassement théorique exp(−πζ/√(1−ζ²)) ≈ 0,254.
    assert!((peak - 1.254).abs() < 0.01, "{peak}");
    assert!((s.value() - 1.0).abs() < 1e-3, "converge vers la cible");
}

#[test]
fn springs_stay_stable_on_huge_steps() {
    for zeta in [0.2, 0.5, 1.0] {
        let mut s = Spring::new();
        for _ in 0..100 {
            let y = s.step(1.0, 5.0, zeta, 0.9);
            assert!(y.is_finite() && y.abs() < 2.0, "ζ = {zeta} : {y}");
        }
    }
}

#[test]
fn one_euro_is_slow_at_rest_and_fast_on_fronts() {
    // Mesuré : moins de 1 % d'écart entre 30, 60 et 144 i/s (écart d'Euler écarté, voir `OneEuro`).
    assert_rate_independent("One-Euro", 0.01, || {
        let mut f = OneEuro::new(1.0, 0.5);
        f.step(0.0, 0.01);
        Box::new(move |x, dt| f.step(x, dt))
    });
    // Repos : un bruit de ±0,02 est nettement atténué.
    let mut f = OneEuro::new(1.0, 0.5);
    f.step(0.0, 1.0 / 60.0);
    let mut worst = 0.0_f32;
    for i in 0..600 {
        let noise = if i % 2 == 0 { 0.02 } else { -0.02 };
        worst = worst.max(f.step(noise, 1.0 / 60.0).abs());
    }
    assert!(worst < 0.01, "{worst}");
}

#[test]
fn schmitt_has_hysteresis() {
    let mut s = Schmitt::new(0.12, 0.08);
    assert!(!s.update(0.10));
    assert!(s.update(0.13));
    assert!(s.update(0.10), "reste haut dans la bande");
    assert!(!s.update(0.07));
    assert!(!s.update(0.10), "reste bas dans la bande");
}

#[test]
fn dead_zone_rescales() {
    assert!(dead_zone(0.1, 0.12).abs() < f32::EPSILON);
    assert!((dead_zone(1.0, 0.12) - 1.0).abs() < 1e-6);
    assert!((dead_zone(0.56, 0.12) - 0.5).abs() < 1e-6);
}

#[test]
fn adaptive_z_flags_a_spike_only() {
    let mut z = AdaptiveZ::new(2.0, 0.02);
    let mut last = 0.0;
    for i in 0..600 {
        last = z.update(0.5 + 0.02 * ((i % 7) as f32 - 3.0) / 3.0, 1.0 / 60.0);
    }
    assert!(last.abs() < 2.0, "régime établi : {last}");
    assert!(z.update(1.5, 1.0 / 60.0) > 4.0, "un pic ressort");
}

#[test]
fn level_agc_normalises_and_follows_the_peak() {
    let mut agc = LevelAgc::new();
    let loud = agc.step(1.0, 1.0 / 60.0);
    assert!((loud - 1.0).abs() < 1e-3);
    // −18 dB sous la crête : au milieu de la plage de 36 dB.
    let half = agc.step(10.0_f32.powf(-1.8), 1.0 / 60.0);
    assert!((half - 0.5).abs() < 0.02, "{half}");
    // Au silence, la crête retombe : un signal faible finit par remonter.
    for _ in 0..(60 * 20) {
        agc.step(1e-6, 1.0 / 60.0);
    }
    assert!(agc.step(1e-6, 1.0 / 60.0) > 0.9);
    assert!((curve_pow(0.5, 2.0) - 0.25).abs() < 1e-6);
}
