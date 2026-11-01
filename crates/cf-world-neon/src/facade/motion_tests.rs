//! Tests des mouvements de lumière (`facade_motion.wesl`). Pas de banc GPU : on vérifie le texte du shader (assemblage,
//! périodes) ; le rendu lui-même n'est vérifié qu'à l'œil.

use super::material::FACADE_SOURCE;

const MOTION_WESL: &str = include_str!("../../../../assets/shaders/cf-world-neon/facade_motion.wesl");
/// Durée au bout de laquelle le temps du shader reboucle, en s.
const WRAP_S: f32 = 2400.0;

#[test]
fn motion_is_part_of_the_facade_source() {
    assert!(
        FACADE_SOURCE.contains("fn motion_gain("),
        "facade_motion.wesl n'est pas concaténé"
    );
    assert!(
        FACADE_SOURCE.contains("motion_gain(temper_b.y"),
        "facade.wesl n'appelle pas motion_gain avec le mouvement du tempérament"
    );
    assert!(
        FACADE_SOURCE.contains("motion: vec4<f32>"),
        "la structure Facade n'a pas le champ motion"
    );
}

#[test]
fn motion_periods_divide_wrap() {
    let periods: Vec<f32> = MOTION_WESL
        .lines()
        .filter(|l| l.starts_with("const ") && l.contains("_PERIOD_S"))
        .filter_map(|l| l.split('=').nth(1)?.trim().trim_end_matches(';').parse().ok())
        .collect();
    assert!(periods.len() >= 3, "périodes introuvables : {periods:?}");
    for p in periods {
        let ratio = WRAP_S / p;
        assert!(
            (ratio - ratio.round()).abs() < 1e-6,
            "la période {p} s ne divise pas {WRAP_S} s"
        );
        assert!(p >= 2.0, "la période {p} s est trop rapide (au moins 2 s par cycle)");
    }
}

#[test]
fn steady_gain_is_exactly_one() {
    assert!(
        MOTION_WESL.contains("if kind < 0.5 {\n        return 1.0;"),
        "Steady doit rendre exactement 1"
    );
}
