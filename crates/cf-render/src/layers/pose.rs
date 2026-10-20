//! La pose de caméra du réalisateur et ce que chaque couche en retient.

use bevy::math::DVec3;
use bevy::prelude::*;

use super::Layer;

/// FOV vertical de la caméra principale de Sillage (`cameras.ts`, `new PerspectiveCamera(62, …)`), en degrés.
pub const DEFAULT_FOV_DEG: f32 = 62.0;
/// Bornes du FOV appliqué, en degrés : au-delà, la projection dégénère.
const FOV_RANGE_DEG: (f32, f32) = (1.0, 170.0);

/// La pose que le réalisateur impose aux caméras (`CameraPose` de `cinema/types.ts`).
///
/// Écrite par un seul système (le réalisateur, en `CfSet::Direct`), lue par le rig de caméras en `CfSet::Present`.
/// Repère de Sillage : −z devant, y vers le haut, mètres.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CameraPose {
    /// Position **absolue** (m, `f64`) : à 400 km du départ un `f32` n'a plus que le centimètre. Le rig la ramène à
    /// l'origine flottante ([`FloatingOrigin`](crate::FloatingOrigin)) pour la caméra du monde.
    pub position: DVec3,
    /// Orientation de la caméra.
    pub rotation: Quat,
    /// FOV vertical, en degrés (comme Sillage).
    pub fov_deg: f32,
    /// Vue au volant : la scène affiche l'habitacle et son rétroviseur (lu par les mondes, sans effet sur le rig).
    pub cabin: bool,
    /// Décalage du ciel : à zéro il est à l'infini ; en le faisant tendre vers le centre d'une planète, la caméra
    /// « voyage » jusqu'à elle. Le ciel est dessiné en `position_caméra - sky_drift` ; dans le repère du ciel, la
    /// caméra de fond est donc en `+sky_drift`.
    pub sky_drift: Vec3,
}

impl Default for CameraPose {
    fn default() -> Self {
        Self {
            position: DVec3::ZERO,
            rotation: Quat::IDENTITY,
            fov_deg: DEFAULT_FOV_DEG,
            cabin: false,
            sky_drift: Vec3::ZERO,
        }
    }
}

/// Où une caméra de couche se place, en coordonnées rendues (`f32`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerView {
    /// Position.
    pub translation: Vec3,
    /// Orientation.
    pub rotation: Quat,
}

impl CameraPose {
    /// FOV vertical à appliquer, en radians : borné, et le FOV par défaut si la valeur n'est pas un nombre.
    #[must_use]
    pub fn fov_rad(&self) -> f32 {
        let deg = if self.fov_deg.is_finite() { self.fov_deg } else { DEFAULT_FOV_DEG };
        deg.clamp(FOV_RANGE_DEG.0, FOV_RANGE_DEG.1).to_radians()
    }

    /// Orientation normalisée ; l'identité si le quaternion est dégénéré (une pose corrompue ne doit pas coucher
    /// l'image, ni produire de NaN dans le rendu).
    fn safe_rotation(&self) -> Quat {
        let q = self.rotation;
        if q.is_finite() && q.length_squared() > 1e-12 {
            q.normalize()
        } else {
            Quat::IDENTITY
        }
    }

    /// Place la caméra de `layer`, ou `None` pour la couche 2D qui n'a pas de pose.
    ///
    /// `origin` est l'origine flottante (m, absolu) : la caméra du monde est rendue relativement à elle.
    ///
    /// - fond : même orientation, position = `sky_drift` (le ciel est centré sur l'origine de son repère) ;
    /// - monde : la pose, relative à l'origine flottante ;
    /// - avant-plan : à l'origine, regard −z. Son contenu est en repère caméra, il suit donc l'objectif au pixel près
    ///   sans jamais porter de grandes coordonnées.
    #[must_use]
    pub fn view_for(&self, layer: Layer, origin: DVec3) -> Option<LayerView> {
        match layer {
            Layer::Background => Some(LayerView {
                translation: if self.sky_drift.is_finite() { self.sky_drift } else { Vec3::ZERO },
                rotation: self.safe_rotation(),
            }),
            Layer::World => {
                let relative = self.position - origin;
                Some(LayerView {
                    translation: if relative.is_finite() { relative.as_vec3() } else { Vec3::ZERO },
                    rotation: self.safe_rotation(),
                })
            }
            Layer::Foreground => Some(LayerView {
                translation: Vec3::ZERO,
                rotation: Quat::IDENTITY,
            }),
            Layer::Overlay => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pose() -> CameraPose {
        CameraPose {
            position: DVec3::new(10.0, 2.0, -400_000.25),
            rotation: Quat::from_rotation_y(0.7),
            fov_deg: 50.0,
            cabin: false,
            sky_drift: Vec3::new(1.0, 2.0, 3.0),
        }
    }

    #[test]
    fn le_fond_suit_l_orientation_et_se_decale_du_ciel() {
        let view = pose().view_for(Layer::Background, DVec3::ZERO).unwrap();
        assert_eq!(view.translation, Vec3::new(1.0, 2.0, 3.0));
        assert!(view.rotation.angle_between(Quat::from_rotation_y(0.7)) < 1e-6);
    }

    #[test]
    fn le_monde_est_rendu_relativement_a_l_origine_flottante() {
        let origin = DVec3::new(0.0, 0.0, -403_200.0);
        let view = pose().view_for(Layer::World, origin).unwrap();
        // 400 000,25 m sur −z, ramenés à l'origine : 3 199,75 m, représentable au millimètre.
        assert_eq!(view.translation, Vec3::new(10.0, 2.0, 3_199.75));
    }

    #[test]
    fn l_avant_plan_est_en_repere_camera_et_la_surcouche_n_a_pas_de_pose() {
        let view = pose().view_for(Layer::Foreground, DVec3::new(1.0e6, 0.0, 0.0)).unwrap();
        assert_eq!(view.translation, Vec3::ZERO);
        assert_eq!(view.rotation, Quat::IDENTITY);
        assert!(pose().view_for(Layer::Overlay, DVec3::ZERO).is_none());
    }

    #[test]
    fn une_pose_corrompue_ne_produit_aucun_nan() {
        let bad = CameraPose {
            position: DVec3::new(f64::NAN, 0.0, 0.0),
            rotation: Quat::from_xyzw(f32::NAN, 0.0, 0.0, 1.0),
            fov_deg: f32::NAN,
            cabin: false,
            sky_drift: Vec3::splat(f32::INFINITY),
        };
        for layer in [Layer::Background, Layer::World, Layer::Foreground] {
            let view = bad.view_for(layer, DVec3::ZERO).unwrap();
            assert!(view.translation.is_finite() && view.rotation.is_finite(), "{layer:?}");
        }
        assert!((bad.fov_rad() - DEFAULT_FOV_DEG.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn le_fov_est_borne() {
        let mut pose = pose();
        pose.fov_deg = 400.0;
        assert!((pose.fov_rad() - 170.0_f32.to_radians()).abs() < 1e-6);
        pose.fov_deg = -3.0;
        assert!((pose.fov_rad() - 1.0_f32.to_radians()).abs() < 1e-6);
    }
}
