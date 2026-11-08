//! Miroir Rust de la chute des tours (`assets/shaders/cf-world-neon/facade_collapse.wesl`) : les formules vivent dans le
//! shader, ici elles sont testables sans carte graphique. Toute modification se fait des deux côtés.
//!
//! À l'arrêt du show, chaque tour rétrécit vers sa base enfouie (`-BASE_DEPTH_M`), à son propre rythme : c'est la
//! désynchronisation. Le rétrécissement se fait autour de la base, jamais autour de `y = 0`, pour qu'aucune base ne
//! remonte au-dessus de la route.

/// Période du décalage d'origine du monde (m) : `ORIGIN_STEP` en est un multiple, le hasard d'une tour reste stable
/// quand l'origine flottante saute.
const CELL_PERIOD_M: f32 = 1200.0;

fn hash(x: f32, y: f32) -> f32 {
    let d = x * 0.067_110_56 + y * 0.005_837_15;
    (52.982_92 * d.fract()).fract()
}

/// Décalage propre à une tour, 0 à 1, tiré de la position de sa base.
#[must_use]
pub fn tower_offset(x: f32, z: f32) -> f32 {
    let wrap = |v: f32| v - CELL_PERIOD_M * (v / CELL_PERIOD_M).floor();
    hash((wrap(x) / 2.0 + 0.5).floor(), (wrap(z) / 2.0 + 0.5).floor())
}

/// Avancement de la chute de cette tour, 0 à 1, pour une chute globale `collapse` : les tours au fort décalage partent
/// plus tard et finissent à temps.
#[must_use]
pub fn collapse_t(collapse: f32, offset: f32) -> f32 {
    (collapse * 1.6 - offset * 0.6).clamp(0.0, 1.0)
}

/// Échelle verticale de la tour, 1 puis 0. Avec `glitch`, elle saute par paliers à 8 Hz.
#[must_use]
pub fn collapse_scale(t: f32, offset: f32, time_s: f32, glitch: bool) -> f32 {
    let mut s = 1.0 - t * t * (3.0 - 2.0 * t);
    if glitch && t > 0.0 && t < 1.0 {
        let flick = hash((time_s * 8.0).floor(), offset * 91.0);
        let stepped = ((s * 7.0 + 0.5).floor() / 7.0 + (flick - 0.5) * 0.28).clamp(0.0, 1.0);
        if flick >= 0.55 {
            s = stepped;
        }
    }
    s
}

/// Altitude d'un sommet après la chute : l'échelle s'applique autour de la base enfouie `base_y`.
#[must_use]
pub fn scaled_y(y: f32, scale: f32, base_y: f32) -> f32 {
    base_y + (y - base_y) * scale
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenery::layout::BASE_DEPTH_M;

    #[test]
    fn scale_is_one_at_zero_collapse() {
        for k in 0..50 {
            let off = tower_offset(k as f32 * 37.0, 12.0);
            assert!((collapse_scale(collapse_t(0.0, off), off, 1.3, true) - 1.0).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn scale_is_zero_at_full_collapse_for_every_offset() {
        for k in 0..200 {
            let off = k as f32 / 199.0;
            let t = collapse_t(1.0, off);
            assert!((t - 1.0).abs() < f32::EPSILON, "{off}");
            assert!(collapse_scale(t, off, 0.4, true).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn offsets_differ_between_towers_and_stay_in_unit_range() {
        let offsets: Vec<f32> = (0..64)
            .map(|k| tower_offset(k as f32 * 24.0, -k as f32 * 9.0))
            .collect();
        assert!(offsets.iter().all(|o| (0.0..1.0).contains(o)));
        let distinct = offsets.windows(2).filter(|w| (w[0] - w[1]).abs() > 1e-3).count();
        assert!(distinct > 50, "{distinct}");
    }

    #[test]
    fn offset_is_stable_when_the_floating_origin_jumps() {
        let step = cf_render::ORIGIN_STEP as f32;
        let a = tower_offset(310.0, -48.0);
        assert!((a - tower_offset(310.0 + step, -48.0 - step)).abs() < 1e-5);
    }

    #[test]
    fn base_never_rises_above_the_buried_base() {
        let base = -BASE_DEPTH_M;
        for k in 0..=100 {
            let off = (k % 10) as f32 / 10.0;
            let t = collapse_t(k as f32 / 100.0, off);
            for glitch in [false, true] {
                let s = collapse_scale(t, off, k as f32 * 0.07, glitch);
                assert!((0.0..=1.0).contains(&s));
                assert!(scaled_y(base, s, base) <= base + 1e-3, "la base ne doit pas bouger");
                assert!(
                    scaled_y(120.0, s, base) <= 120.0 + 1e-3,
                    "une tour ne grandit jamais en chutant"
                );
            }
        }
    }

    #[test]
    fn without_glitch_the_scale_is_continuous() {
        let off = 0.4;
        let mut prev = 1.0_f32;
        for k in 0..=1000 {
            let s = collapse_scale(collapse_t(k as f32 / 1000.0, off), off, 0.0, false);
            assert!(prev - s < 0.02 && s <= prev + 1e-6, "{prev} -> {s}");
            prev = s;
        }
    }

    #[test]
    fn glitch_steps_the_scale_in_palettes() {
        let off = 0.3;
        let t = 0.5;
        let seen: std::collections::BTreeSet<u32> = (0..200)
            .map(|k| (collapse_scale(t, off, k as f32 / 8.0, true) * 1000.0) as u32)
            .collect();
        assert!(seen.len() > 1, "le glitch doit varier avec le temps");
    }

    #[test]
    fn shader_constants_match_rust() {
        let shader = include_str!("../../../../assets/shaders/cf-world-neon/facade_collapse.wesl");
        assert!(shader.contains(&format!("const COLLAPSE_BASE_DEPTH_M: f32 = {BASE_DEPTH_M:.1};")));
        assert!(shader.contains(&format!("const COLLAPSE_CELL_PERIOD_M: f32 = {CELL_PERIOD_M:.1};")));
        assert_eq!(cf_render::ORIGIN_STEP % f64::from(CELL_PERIOD_M), 0.0);
    }

    #[test]
    fn both_shaders_apply_the_collapse() {
        for src in [
            super::super::material::FACADE_SOURCE,
            super::super::material::PREPASS_SOURCE,
        ] {
            assert!(src.contains("fn collapse_apply"), "def {}", src.len());
            assert!(src.contains("collapse_apply(wp"), "call {}", src.len());
        }
    }
}
