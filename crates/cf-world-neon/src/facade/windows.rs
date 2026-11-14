//! Fenêtres hiérarchisées (chantier C4, DA §1.1) : le miroir Rust des fonctions `window_share`, `window_gain` et
//! `window_cap` de `facade.wesl`, pour tester sans GPU qu'une fenêtre reste au palier E1.
//!
//! - part des étages allumés : `p = 0,18 + 0,42·Δ` (plus le biais du tempérament de l'immeuble) ; les fenêtres s'allument
//!   par étages entiers ([`ON_FLOOR`] dans un étage allumé, [`OFF_FLOOR`] ailleurs), pas en confettis ;
//! - intensité log-normale `I = E1·exp(0,6·n)`, `n` gaussien tiré du hachage ;
//! - plafond : aucun canal ne dépasse `0,9·θ_b`, quoi que fasse la musique (vague de kick, pouls, pluie).
//!
//! Toute modification ici se reporte dans le shader : un test compare les constantes des deux textes.

use bevy::math::Vec3;

/// Part des étages allumés au calme (`Δ = 0`).
pub const SHARE_CALM: f32 = 0.18;
/// Ce que la densité visuelle ajoute à la part des étages allumés.
pub const SHARE_DELTA: f32 = 0.42;
/// Chance d'une fenêtre d'être allumée dans un étage allumé.
pub const ON_FLOOR: f32 = 0.9;
/// Chance d'une fenêtre d'être allumée dans un étage éteint.
pub const OFF_FLOOR: f32 = 0.03;
/// Palier E1 : gain médian d'une fenêtre.
pub const E1: f32 = 0.8;
/// Écart-type du logarithme de l'intensité.
pub const SIGMA: f32 = 0.6;
/// Plafond d'une fenêtre, en part du seuil du bloom `θ_b`.
pub const CAP_SHARE: f32 = 0.9;

/// Part des étages allumés.
#[must_use]
pub fn window_share(delta: f32, bias: f32) -> f32 {
    (SHARE_CALM + SHARE_DELTA * delta.clamp(0.0, 1.0) + bias).clamp(0.02, 0.95)
}

/// Part des fenêtres allumées attendue, tous étages confondus.
#[must_use]
pub fn lit_fraction(delta: f32, bias: f32) -> f32 {
    let p = window_share(delta, bias);
    p * ON_FLOOR + (1.0 - p) * OFF_FLOOR
}

/// Gain log-normal d'une fenêtre ; `n` est borné à ±3.
#[must_use]
pub fn window_gain(n: f32) -> f32 {
    E1 * (SIGMA * n.clamp(-3.0, 3.0)).exp()
}

/// Plafond : aucun canal ne dépasse `cap`, la teinte est gardée.
#[must_use]
pub fn window_cap(color: Vec3, cap: f32) -> Vec3 {
    let top = color.max_element();
    color * (cap / top.max(1.0e-4)).min(1.0)
}

/// Palier E2 d'un tube néon au repos, en `θ_b`.
pub const NEON_E2: f32 = 1.5;
/// Part du halo d'un tube.
pub const NEON_HALO: f32 = 0.35;

/// Profil d'un tube néon à la distance `d` de son axe (rayon `r`) : le cœur vire au blanc, le halo reste saturé
/// (DA §2.2). Miroir de `neon_tube` de `facade.wesl`.
#[must_use]
pub fn neon_tube(color: Vec3, white: Vec3, d: f32, r: f32) -> Vec3 {
    let t = ((d - 0.5 * r) / (0.5 * r)).clamp(0.0, 1.0);
    let core = 1.0 - t * t * (3.0 - 2.0 * t);
    color.lerp(white, core * core) * core + color * (-d / (4.0 * r)).exp() * NEON_HALO
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::facade::material::FACADE_SOURCE;

    const THETA_B: f32 = cf_render::postfx::BLOOM_THRESHOLD;

    proptest! {
        /// Critère de C4 : une fenêtre ne dépasse jamais `0,9·θ_b`, pour toute couleur, tout tirage et tout gain que le
        /// shader peut produire (vague de kick, pouls, pouls de la ville, mouvement du tempérament, pluie).
        #[test]
        fn une_fenetre_ne_depasse_jamais_le_plafond(
            r in 0.0_f32..1.5, g in 0.0_f32..1.5, b in 0.0_f32..1.5,
            n in -6.0_f32..6.0,
            glow in 0.0_f32..6.0,
            wet in 0.0_f32..1.0,
        ) {
            let pane = Vec3::new(r, g, b) * glow * (1.0 + 0.45 * wet);
            let out = window_cap(pane * window_gain(n), CAP_SHARE * THETA_B);
            prop_assert!(out.max_element() <= CAP_SHARE * THETA_B + 1e-5, "{out:?}");
            prop_assert!(out.min_element() >= 0.0);
        }

        #[test]
        fn le_plafond_garde_la_teinte(r in 0.05_f32..1.0, g in 0.05_f32..1.0, b in 0.05_f32..1.0, k in 1.0_f32..20.0) {
            let c = Vec3::new(r, g, b) * k;
            let out = window_cap(c, 0.9);
            prop_assert!((out.x * c.y - out.y * c.x).abs() < 1e-3 && (out.y * c.z - out.z * c.y).abs() < 1e-3);
        }
    }

    #[test]
    fn le_blanc_d_un_tube_ne_couvre_que_le_coeur() {
        let (c, white, r) = (Vec3::new(0.0, 0.8, 1.0), Vec3::ONE, 0.07);
        // Sur l'axe : le blanc, plus le halo.
        let axis = neon_tube(c, white, 0.0, r);
        assert!(axis.x > 0.9, "{axis:?}");
        // Hors du cœur (d ≥ r) : la teinte pure, sans rouge, qui s'éteint avec la distance.
        let (near, far) = (neon_tube(c, white, r, r), neon_tube(c, white, 6.0 * r, r));
        assert!(near.x.abs() < 1e-6 && far.x.abs() < 1e-6);
        assert!(near.z > far.z && far.z > 0.0 && near.z <= NEON_HALO);
        // Palier E2 : de 1,5·θ_b au repos à 3·θ_b au plus (pouls 1, basses 1), jamais un palier de plus.
        assert!((NEON_E2 + 1.0 + 0.5 - 3.0).abs() < 1e-6);
        for (name, rust) in [("NEON_E2", NEON_E2), ("NEON_HALO", NEON_HALO)] {
            assert!((wesl_const(name) - rust).abs() < 1e-9, "{name}");
        }
    }

    #[test]
    fn la_part_allumee_suit_la_densite_visuelle() {
        assert!((window_share(0.0, 0.0) - 0.18).abs() < 1e-6);
        assert!((window_share(1.0, 0.0) - 0.60).abs() < 1e-6);
        // Tous étages confondus, la part allumée reste celle de DA §1.1 à 0,02 près.
        assert!((lit_fraction(0.0, 0.0) - 0.18).abs() < 0.02);
        assert!((lit_fraction(1.0, 0.0) - 0.60).abs() < 0.05);
        // Entrées hors bornes : jamais tout allumé, jamais tout éteint.
        assert!(window_share(7.0, 3.0) <= 0.95 && window_share(-2.0, -3.0) >= 0.02);
    }

    #[test]
    fn la_mediane_d_une_fenetre_est_au_palier_e1() {
        // E1 = 0,2 à 0,7·θ_b pour la base de lumière du shader (0,55) : 0,55 × 0,8 = 0,44·θ_b.
        let median = 0.55 * window_gain(0.0);
        assert!((0.2 * THETA_B..=0.7 * THETA_B).contains(&median), "{median}");
    }

    /// Lit `const NAME: f32 = valeur;` dans le shader.
    fn wesl_const(name: &str) -> f32 {
        let key = format!("const {name}: f32 = ");
        let at = FACADE_SOURCE.find(&key).map_or(0, |i| i + key.len());
        let rest = &FACADE_SOURCE[at..];
        rest[..rest.find(';').unwrap_or(0)].trim().parse().unwrap_or(f32::NAN)
    }

    #[test]
    fn les_constantes_du_shader_sont_celles_du_miroir_rust() {
        for (name, rust) in [
            ("WINDOW_SHARE_CALM", SHARE_CALM),
            ("WINDOW_SHARE_DELTA", SHARE_DELTA),
            ("WINDOW_ON_FLOOR", ON_FLOOR),
            ("WINDOW_OFF_FLOOR", OFF_FLOOR),
            ("WINDOW_E1", E1),
            ("WINDOW_SIGMA", SIGMA),
        ] {
            assert!(
                (wesl_const(name) - rust).abs() < 1e-9,
                "{name} : shader {}, Rust {rust}",
                wesl_const(name)
            );
        }
        // Le shader plafonne bien ce qu'il ajoute aux fenêtres.
        assert!(FACADE_SOURCE.contains("window_cap(pane * window_gain(pane_n) + trail_pane, scene.c_key.w)"));
    }
}
