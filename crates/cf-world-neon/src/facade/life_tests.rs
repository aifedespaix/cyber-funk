//! Tests des fenêtres à vie propre (`facade_life.wesl`). Pas de banc GPU : on vérifie le texte du shader (assemblage,
//! périodes, constantes) et, sur un MIROIR Rust des maths pures (marqué ci-dessous, à tenir à l'identique du WGSL), le
//! déterminisme, la part de cellules concernées, la neutralité à `allow = 0` et la vitesse de variation.

use cf_show::limiter::{MAX_FLASHES_PER_SECOND, MIN_FADE};

use super::material::FACADE_SOURCE;

const LIFE_WESL: &str = include_str!("../../../../assets/shaders/cf-world-neon/facade_life.wesl");
/// Durée au bout de laquelle le temps du shader reboucle, en s.
const WRAP_S: f32 = 2400.0;

/// Lit une constante WGSL `const NAME: f32 = valeur;` du fichier.
fn wesl_const(name: &str) -> f32 {
    LIFE_WESL
        .lines()
        .find(|l| l.starts_with(&format!("const {name}:")))
        .and_then(|l| l.split('=').nth(1)?.trim().trim_end_matches(';').parse().ok())
        .unwrap_or(f32::NAN)
}

#[test]
fn life_is_part_of_the_facade_source() {
    assert!(
        FACADE_SOURCE.contains("fn window_life("),
        "facade_life.wesl n'est pas concaténé"
    );
    assert!(
        FACADE_SOURCE.contains("window_life(cell, seed, time, temper_b.z * facade.motion.z, facade.motion.y)"),
        "facade.wesl n'appelle pas window_life"
    );
    assert!(
        FACADE_SOURCE.contains("lit *= window_life(") && FACADE_SOURCE.contains("if !far {"),
        "les immeubles lointains doivent être exclus"
    );
}

#[test]
fn life_constants_are_found() {
    for name in [
        "TV_SHARE",
        "REST_SHARE",
        "REST_START_S",
        "REST_END_S",
        "REST_FADE_S",
        "BUZZ_DEPTH",
        "BUZZ_FADE_S",
    ] {
        assert!(wesl_const(name).is_finite(), "constante {name} introuvable");
    }
    assert!(
        wesl_const("BUZZ_FADE_S") >= MIN_FADE,
        "bord du néon plus raide que MIN_FADE"
    );
}

#[test]
fn life_periods_divide_wrap() {
    let periods: Vec<f32> = LIFE_WESL
        .lines()
        .filter(|l| l.starts_with("const ") && l.contains("_PERIOD_S"))
        .filter_map(|l| l.split('=').nth(1)?.trim().trim_end_matches(';').parse().ok())
        .collect();
    assert!(periods.len() >= 4, "périodes introuvables : {periods:?}");
    for p in periods {
        let ratio = WRAP_S / p;
        assert!(
            (ratio - ratio.round()).abs() < 1e-6,
            "la période {p} s ne divise pas {WRAP_S} s"
        );
    }
}

// ---- MIROIR Rust de `facade_life.wesl` (à garder identique) ----

fn fract(x: f32) -> f32 {
    x - x.floor()
}
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn h21(p: (f32, f32)) -> f32 {
    let (mut x, mut y) = (fract(p.0 * 123.34), fract(p.1 * 456.21));
    let d = x * (x + 45.32) + y * (y + 45.32);
    x += d;
    y += d;
    fract(x * y)
}
fn dip(q: f32, start: f32, end: f32, fade: f32) -> f32 {
    smoothstep(start, start + fade, q) * (1.0 - smoothstep(end, end + fade, q))
}
fn window_life(cell: (f32, f32), seed: f32, time: f32, spark_rate: f32, allow: f32) -> f32 {
    let pick = h21((cell.0 + seed * 0.53 + 101.3, cell.1 + seed * 0.19));
    if pick >= spark_rate {
        return 1.0;
    }
    let kind = h21((cell.0 + 17.7, cell.1 + seed * 0.41 + 3.3));
    let phase = h21((cell.0 + 29.1 + seed * 0.07, cell.1 + 8.8));
    let tau = std::f32::consts::TAU;
    if kind < wesl_const("TV_SHARE") {
        let slow = 0.5 + 0.5 * (tau * (time / wesl_const("TV_PERIOD_S") + phase)).sin();
        let fine = 0.5 + 0.5 * (tau * (time / wesl_const("TV_FINE_PERIOD_S") + phase * 3.0)).sin();
        return 1.0 - 0.3 * slow - 0.12 * fine;
    }
    if kind < wesl_const("TV_SHARE") + wesl_const("REST_SHARE") {
        let period = wesl_const("REST_PERIOD_S");
        let q = fract(time / period + phase) * period;
        let off = dip(
            q,
            wesl_const("REST_START_S"),
            wesl_const("REST_END_S"),
            wesl_const("REST_FADE_S"),
        );
        return 1.0 - off;
    }
    let period = wesl_const("BUZZ_PERIOD_S");
    let q = fract(time / period + phase) * period;
    let fade = wesl_const("BUZZ_FADE_S");
    let d = dip(q, 0.2, 0.5, fade) + dip(q, 1.4, 1.7, fade) + dip(q, 2.6, 2.9, fade);
    1.0 - allow * wesl_const("BUZZ_DEPTH") * d.min(1.0)
}

fn cells() -> impl Iterator<Item = ((f32, f32), f32)> {
    (0..80).flat_map(|x| (0..40).map(move |y| ((x as f32, y as f32), (x * 7 + y) as f32 % 997.0)))
}

#[test]
fn life_is_deterministic_and_wraps() {
    for (c, seed) in cells().take(400) {
        for t in [0.0_f32, 3.7, 91.3, 1000.1] {
            let a = window_life(c, seed, t, 0.3, 1.0);
            assert_eq!(a, window_life(c, seed, t, 0.3, 1.0));
            let b = window_life(c, seed, t + WRAP_S, 0.3, 1.0);
            assert!((a - b).abs() < 2e-3, "{c:?} t={t} : {a} contre {b} après le rebouclage");
        }
    }
}

#[test]
fn life_share_is_bounded_by_spark_rate() {
    for rate in [0.0_f32, 0.05, 0.2] {
        let total = cells().count() as f32;
        // Une cellule « concernée » a au moins un instant où le facteur n'est pas 1.
        let touched = cells()
            .filter(|(c, s)| (0..4800).any(|i| window_life(*c, *s, i as f32 * 0.5, rate, 1.0) < 0.999))
            .count() as f32;
        assert!(
            touched / total <= rate + 0.03,
            "part {} pour spark_rate {rate}",
            touched / total
        );
        if rate == 0.0 {
            assert_eq!(touched, 0.0, "tempérament 0 : look d'avant exactement");
        }
    }
}

#[test]
fn zero_allow_leaves_only_slow_behaviours() {
    let (tv, rest) = (wesl_const("TV_SHARE"), wesl_const("REST_SHARE"));
    assert!(tv + rest < 1.0, "il faut une part de néons grésillants");
    for (c, seed) in cells().take(800) {
        for i in 0..200 {
            let t = i as f32 * 0.37;
            let with = window_life(c, seed, t, 0.5, 0.0);
            // Sans part autorisée, le néon ne dévie jamais de 1 ; le reste est lent et identique à allow = 1.
            let full = window_life(c, seed, t, 0.5, 1.0);
            assert!(with >= full - 1e-6);
            if with < 1.0 {
                assert_eq!(with, full, "un comportement lent dépend de allow");
            }
        }
    }
}

#[test]
fn life_changes_are_slow_and_rare() {
    let dt = 1.0 / 60.0;
    // Un front plus raide que 1/MIN_FADE par seconde est un flash non limité ; les TV et repos sont bien plus lents.
    let max_step = dt / MIN_FADE * 0.75 + 1e-3;
    for (c, seed) in cells().take(600) {
        let mut prev = window_life(c, seed, 0.0, 1.0, 1.0);
        let mut dips = 0;
        let mut below = prev < 0.7;
        for i in 1..=(60 * 12) {
            let v = window_life(c, seed, i as f32 * dt, 1.0, 1.0);
            assert!(
                (v - prev).abs() <= max_step,
                "saut de {} en une image",
                (v - prev).abs()
            );
            let now = v < 0.7;
            if now && !below {
                dips += 1;
            }
            below = now;
            prev = v;
        }
        // 12 s : jamais plus de MAX_FLASHES_PER_SECOND extinctions par seconde.
        assert!(
            dips as f32 <= MAX_FLASHES_PER_SECOND as f32 * 12.0 / 3.0,
            "{dips} creux en 12 s"
        );
    }
}
