//! `CarAnimation` : ce que la voiture fait en l'air, à l'image. Publié par le pilote, lu par le réalisateur (J6) pour
//! cadrer un envol, une figure, une esquive, et par la mise en scène de la voiture pour l'incliner.

use bevy::math::Quat;
use bevy::prelude::Resource;

use super::flight::{DodgeSide, FlightFrame, FlightPhase, TrickKind};

/// L'animation de vol de la voiture à l'image courante (`FlightFrame` de Sillage, en `f32` de rendu).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct CarAnimation {
    /// Phase du vol : au sol, déploiement des roues, en vol, repli.
    pub phase: FlightPhase,
    /// Roues en répulseurs (déploiement ou vol).
    pub hover: bool,
    /// Avancement du déploiement (ou du repli) des roues, 0 → 1.
    pub hover_amount: f32,
    /// Altitude au-dessus de la route, m, esquives et figures comprises.
    pub altitude: f32,
    /// Étage visé, 0 = sol.
    pub floor: u8,
    /// Vitesse verticale lissée, m/s (+ = monte).
    pub climb: f32,
    /// Inclinaison due à la montée, rad (+ = nez en l'air).
    pub pitch: f32,
    /// Roulis dû à l'esquive, rad (+ = roulis à droite).
    pub roll: f32,
    /// Rotation de tonneau en cours, rad (0 à ±2π ; + = vers la droite).
    pub trick_roll: f32,
    /// Rotation de salto en cours, rad (0 à ±2π ; + = arrière).
    pub trick_pitch: f32,
    /// La figure en cours, ou `None`.
    pub trick: Option<TrickKind>,
    /// Rebond de la caisse sur ses répulseurs, m.
    pub bounce: f32,
    /// Esquive en cours (par dessous, par dessus).
    pub dodge: DodgeSide,
}

impl From<&FlightFrame> for CarAnimation {
    fn from(f: &FlightFrame) -> Self {
        Self {
            phase: f.phase,
            hover: f.hover,
            hover_amount: f.hover_amount as f32,
            altitude: f.altitude as f32,
            floor: f.floor,
            climb: f.climb as f32,
            pitch: f.pitch as f32,
            roll: f.roll as f32,
            trick_roll: f.trick_roll as f32,
            trick_pitch: f.trick_pitch as f32,
            trick: f.trick,
            bounce: f.bounce as f32,
            dodge: f.dodge,
        }
    }
}

impl CarAnimation {
    /// Vrai quand la voiture n'est pas (ou plus) au sol.
    #[must_use]
    pub fn airborne(&self) -> bool {
        self.phase != FlightPhase::Ground
    }

    /// Orientation de la caisse dans le repère de la voiture (nez vers −z) : tangage de la montée et du salto, roulis de
    /// l'esquive et du tonneau. Se compose avec le lacet de la voiture : `lacet * orientation`.
    #[must_use]
    pub fn orientation(&self) -> Quat {
        // Nez en l'air = rotation positive autour de x ; un roulis à droite couche le côté droit, soit une rotation
        // négative autour de z.
        Quat::from_rotation_x(self.pitch + self.trick_pitch) * Quat::from_rotation_z(-(self.roll + self.trick_roll))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;

    #[test]
    fn au_sol_l_orientation_est_neutre() {
        let a = CarAnimation::default();
        assert!(!a.airborne());
        assert!(a.orientation().angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn nez_en_l_air_et_roulis_a_droite() {
        let climbing = CarAnimation {
            pitch: 0.2,
            ..CarAnimation::default()
        };
        let nose = climbing.orientation() * Vec3::NEG_Z;
        assert!(nose.y > 0.19, "le nez monte : {nose}");
        let banking = CarAnimation {
            roll: 0.3,
            ..CarAnimation::default()
        };
        let right = banking.orientation() * Vec3::X;
        assert!(right.y < -0.29, "le côté droit descend : {right}");
    }
}
