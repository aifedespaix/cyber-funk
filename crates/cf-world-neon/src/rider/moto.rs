//! Les trois motos du trafic (`assets/models/motos/moto-N-low.glb`, moto et pilote figé) : leur chargement à l'ouverture
//! et le calage qui les met à la place de la moto procédurale (`traffic/model/bike.rs`).
//!
//! La moto procédurale a son origine à l'axe de la roue arrière (le wheeling pivote autour de la roue qui touche le sol),
//! regarde vers −Z et pose ses roues à `-WHEEL` sous l'origine. Les GLB ont leur origine au sol, au centre, et regardent
//! vers +Z : [`fit`] retourne le modèle, le grossit à la taille de la procédurale et recale son origine.

use std::f32::consts::PI;
use std::path::Path;

use bevy::math::{Quat, Vec3};
use bevy::transform::components::Transform;

use crate::car::model::CarModelAsset;
use crate::traffic::model::bike::WHEEL;

/// Nombre de modèles de moto.
pub const MOTO_COUNT: usize = 3;

/// Les fichiers, dans le dossier `models/motos` : la variante légère (textures 512 à 1k, ≈ 35 000 triangles).
pub const MOTO_FILES: [&str; MOTO_COUNT] = ["moto-1-low.glb", "moto-2-low.glb", "moto-3-low.glb"];

/// Matériaux de carrosserie, teintés sur toute leur surface (le pilote l'est par son masque de peinture). Les noms sont ceux
/// des matériaux du GLB, sans le suffixe de doublon de Blender.
pub const BODY_MATERIALS: [&[&str]; MOTO_COUNT] = [
    &["Carcass1", "Rear_frame1", "Rule1"],
    &["przod", "owiewki"],
    &["cor_base02"],
];

/// Longueur du modèle (m) : 2,2 m, voir `tools/moto/normalize.py`.
const MODEL_LENGTH_M: f32 = 2.2;
/// Longueur de la moto procédurale : empattement plus les deux rayons de roue.
const PROCEDURAL_LENGTH_M: f32 = 1.9 + 2.0 * WHEEL;
/// Axe de la roue arrière de chaque modèle, mesuré sur ses vues de profil : distance derrière le centre (m).
const REAR_AXLE_BACK_M: [f32; MOTO_COUNT] = [0.85, 0.80, 0.80];

/// Le facteur qui amène un modèle à la taille de la moto procédurale.
#[must_use]
pub fn fit_scale() -> f32 {
    PROCEDURAL_LENGTH_M / MODEL_LENGTH_M
}

/// Le calage de la moto `index` sous l'entité du véhicule : retournée vers −Z, à la taille de la procédurale, l'axe de la
/// roue arrière à l'origine et le sol à `-WHEEL`.
#[must_use]
pub fn fit(index: usize) -> Transform {
    let scale = fit_scale();
    let back = REAR_AXLE_BACK_M[index % MOTO_COUNT];
    Transform {
        // Après le demi-tour, la roue arrière est à +back en z, grossie : on la ramène à l'origine.
        translation: Vec3::new(0.0, -WHEEL, -back * scale),
        rotation: Quat::from_rotation_y(PI),
        scale: Vec3::splat(scale),
    }
}

/// Lit et décode les trois motos dans `dir`. `None` (et un avertissement) si l'une manque ou est illisible : le trafic garde
/// alors sa moto procédurale.
#[must_use]
pub fn load_motos(dir: &Path) -> Option<Vec<CarModelAsset>> {
    let mut models = Vec::with_capacity(MOTO_COUNT);
    for file in MOTO_FILES {
        let path = dir.join(file);
        let decoded = std::fs::read(&path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| CarModelAsset::from_glb(&bytes).map_err(|error| error.to_string()));
        match decoded {
            Ok(model) => models.push(model),
            Err(error) => {
                tracing::warn!(%error, path = %path.display(), "moto illisible : le trafic garde sa moto procédurale");
                return None;
            }
        }
    }
    Some(models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_modele_est_grossi_a_la_taille_de_la_moto_procedurale() {
        assert!((fit_scale() - 2.62 / 2.2).abs() < 0.01, "{}", fit_scale());
    }

    /// Le sol du modèle (y = 0 avant calage) tombe à `-WHEEL` sous l'origine, comme les roues de la procédurale.
    #[test]
    fn le_sol_est_a_la_hauteur_des_roues_de_la_procedurale() {
        let t = fit(0);
        let ground = t.transform_point(Vec3::ZERO);
        assert!((ground.y + WHEEL).abs() < 1e-6, "{ground}");
    }

    /// Le modèle regarde +Z : après calage, son avant (z = +1,1) est à −Z, devant la roue arrière à l'origine.
    #[test]
    fn la_moto_regarde_vers_moins_z_et_la_roue_arriere_est_a_l_origine() {
        for (index, back) in REAR_AXLE_BACK_M.into_iter().enumerate() {
            let t = fit(index);
            let front = t.transform_point(Vec3::new(0.0, 0.3, 1.1));
            assert!(front.z < -1.5, "avant à {front}");
            let rear_axle = t.transform_point(Vec3::new(0.0, 0.3, -back));
            assert!(rear_axle.z.abs() < 1e-5, "roue arrière {rear_axle}");
        }
    }
}
