//! Surface de la route (chantier C4 ; P2 §1.1, DA §1.2) : le miroir Rust de `road_surface.wesl`, pour tester sans GPU la
//! rugosité réactive, les ondes de kick et la flaque de lumière.
//!
//! - **Rugosité réactive** : le kick élargit le reflet des flaques pendant 180 ms (`Target::Puddles`), la pluie aussi ;
//!   un break (`calme → 1`) aplanit leur relief et en fait un miroir net. L'asphalte hors flaque ne change pas.
//! - **Ondes de kick** : chaque kick pousse un anneau sur l'eau depuis le sujet ; la pente est la dérivée radiale d'une
//!   gaussienne, nulle sur la crête, éteinte avec l'âge.
//! - **Flaque de lumière** : `c·exp(−r²/σ²)` sous le sujet (DA §1.2, règle 6).
//!
//! Toute modification ici se reporte dans le shader : un test compare les constantes des deux textes.

use bevy::math::Vec2;

/// Part des étages allumés au calme, que le sol reflète (celle des façades).
pub const WINDOW_SHARE_CALM: f32 = crate::facade::windows::SHARE_CALM;
/// Ce que la densité visuelle y ajoute.
pub const WINDOW_SHARE_DELTA: f32 = crate::facade::windows::SHARE_DELTA;
/// Étalement d'un reflet sur l'asphalte et sur une flaque, rad.
pub const ROUGH: (f32, f32) = (0.09, 0.006);
/// Ce qu'un kick ajoute à l'étalement d'un reflet de flaque, rad.
pub const ROUGH_KICK: f32 = 0.035;
/// Part du relief de l'eau qu'un break retire.
pub const CALM_MIRROR: f32 = 0.6;
/// Écart angulaire entre le reflet rouge et le reflet bleu.
pub const ABERRATION: f32 = 0.012;
/// Rayon de la flaque de lumière sous le sujet, m.
pub const POOL_SIGMA_M: f32 = 6.0;
/// Gain de la flaque de lumière.
pub const POOL_GAIN: f32 = 0.12;
/// Ondes de kick : vitesse du front (m/s), épaisseur (m), durée de vie (s), pente maximale.
pub const KICK: (f32, f32, f32, f32) = (14.0, 0.9, 0.7, 0.05);

/// Étalement angulaire du reflet. `pudk` : part de flaque, `kick` : frémissement 0..1, `rain` : pluie 0..1.
#[must_use]
pub fn roughness(pudk: f32, kick: f32, rain: f32) -> f32 {
    ROUGH.0 + (ROUGH.1 - ROUGH.0) * pudk + pudk * (ROUGH_KICK * kick + 0.5 * ROUGH_KICK * rain)
}

/// Ce qui reste du relief de l'eau : le calme aplanit les flaques, pas l'asphalte.
#[must_use]
pub fn calm_relief(pudk: f32, calm: f32) -> f32 {
    1.0 - CALM_MIRROR * calm.clamp(0.0, 1.0) * pudk
}

/// Lumière de la flaque sous le sujet, à `r` mètres de lui (à multiplier par la lumière principale).
#[must_use]
pub fn pool_light(r: f32) -> f32 {
    let q = r / POOL_SIGMA_M;
    POOL_GAIN * (-q * q).exp()
}

/// Pente que les quatre derniers kicks (âges en s) ajoutent à la surface de l'eau au point `p`, la source étant en
/// `source`. `depth` : profondeur de la coquille (0 en respiration ou flashs coupés : aucune onde).
#[must_use]
pub fn kick_ripples(p: Vec2, source: Vec2, ages: [f32; 4], depth: f32) -> Vec2 {
    let to = p - source;
    let r = to.length().max(0.001);
    let slope: f32 = ages
        .iter()
        .map(|age| {
            let x = (r - KICK.0 * age) / KICK.1;
            -x * (-x * x).exp() * (-age / KICK.2).exp()
        })
        .sum();
    (to / r) * slope * KICK.3 * depth * 2.0
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::facade::windows;
    use crate::road::material::ROAD_SOURCE;

    #[test]
    fn le_kick_elargit_le_reflet_des_flaques_pas_celui_de_l_asphalte() {
        assert!((roughness(0.0, 1.0, 1.0) - ROUGH.0).abs() < 1e-6);
        assert!(roughness(1.0, 1.0, 0.0) > 4.0 * roughness(1.0, 0.0, 0.0));
        // Même au plus fort, une flaque reste plus nette que l'asphalte.
        assert!(roughness(1.0, 1.0, 1.0) < ROUGH.0);
    }

    #[test]
    fn un_break_fait_des_flaques_un_miroir() {
        assert!((calm_relief(1.0, 0.0) - 1.0).abs() < 1e-6 && (calm_relief(0.0, 1.0) - 1.0).abs() < 1e-6);
        assert!((calm_relief(1.0, 1.0) - (1.0 - CALM_MIRROR)).abs() < 1e-6);
        assert!(calm_relief(1.0, 9.0) > 0.0);
    }

    #[test]
    fn la_flaque_de_lumiere_tient_dans_quinze_metres() {
        assert!((pool_light(0.0) - POOL_GAIN).abs() < 1e-6);
        assert!(pool_light(15.0) < 0.002 * POOL_GAIN / 0.12 + 1e-4 && pool_light(6.0) < pool_light(3.0));
    }

    proptest! {
        /// La pente d'une onde de kick est bornée, radiale, et nulle quand la coquille est interdite.
        #[test]
        fn l_onde_de_kick_est_bornee_et_radiale(
            x in -40.0_f32..40.0, z in -40.0_f32..40.0,
            a0 in 0.0_f32..3.0, a1 in 0.0_f32..3.0, a2 in 0.0_f32..60.0, a3 in 0.0_f32..60.0,
            depth in 0.0_f32..1.0,
        ) {
            let p = Vec2::new(x, z);
            let s = kick_ripples(p, Vec2::ZERO, [a0, a1, a2, a3], depth);
            // |x·e^{−x²}| ≤ 0,43 par onde, quatre ondes.
            prop_assert!(s.length() <= 4.0 * 0.43 * KICK.3 * 2.0 * depth + 1e-6);
            if p.length() > 0.01 {
                prop_assert!(s.perp_dot(p).abs() <= 1e-4 * p.length().max(1.0));
            }
            prop_assert!(kick_ripples(p, Vec2::ZERO, [a0, a1, a2, a3], 0.0).length() < 1e-9);
        }
    }

    #[test]
    fn la_pente_s_annule_sur_la_crete_et_s_eteint_avec_l_age() {
        // Un seul kick, il y a 0,5 s : la crête est à 7 m, la pente y change de signe.
        let ages = [0.5, 60.0, 60.0, 60.0];
        let at = |r: f32| kick_ripples(Vec2::new(r, 0.0), Vec2::ZERO, ages, 1.0).x;
        assert!(at(7.0).abs() < 1e-5 && at(6.6) > 0.0 && at(7.4) < 0.0);
        // Quatre secondes plus tard (plus de cinq durées de vie), au même point de son profil : il n'en reste rien.
        let old = kick_ripples(
            Vec2::new(14.0 * 4.0 - 0.4, 0.0),
            Vec2::ZERO,
            [4.0, 60.0, 60.0, 60.0],
            1.0,
        );
        assert!(old.length() < 0.02 * at(6.6).abs() + 1e-4);
    }

    /// Lit `const NAME: f32 = valeur;` dans le shader.
    fn wesl_const(name: &str) -> f32 {
        let key = format!("const {name}: f32 = ");
        let at = ROAD_SOURCE.find(&key).map_or(0, |i| i + key.len());
        let rest = &ROAD_SOURCE[at..];
        rest[..rest.find(';').unwrap_or(0)].trim().parse().unwrap_or(f32::NAN)
    }

    #[test]
    fn les_constantes_du_shader_sont_celles_du_miroir_rust() {
        for (name, rust) in [
            ("ROAD_WINDOW_SHARE_CALM", windows::SHARE_CALM),
            ("ROAD_WINDOW_SHARE_DELTA", windows::SHARE_DELTA),
            ("ROAD_WINDOW_ON_FLOOR", windows::ON_FLOOR),
            ("ROAD_WINDOW_OFF_FLOOR", windows::OFF_FLOOR),
            ("ROAD_ROUGH_KICK", ROUGH_KICK),
            ("ROAD_CALM_MIRROR", CALM_MIRROR),
            ("ROAD_ABERRATION", ABERRATION),
            ("ROAD_POOL_SIGMA_M", POOL_SIGMA_M),
            ("ROAD_POOL_GAIN", POOL_GAIN),
            ("ROAD_KICK_SPEED", KICK.0),
            ("ROAD_KICK_WIDTH", KICK.1),
            ("ROAD_KICK_LIFE", KICK.2),
            ("ROAD_KICK_SLOPE", KICK.3),
        ] {
            assert!(
                (wesl_const(name) - rust).abs() < 1e-6,
                "{name} : shader {}, Rust {rust}",
                wesl_const(name)
            );
        }
        assert!((WINDOW_SHARE_CALM - 0.18).abs() < 1e-6 && (WINDOW_SHARE_DELTA - 0.42).abs() < 1e-6);
    }
}
