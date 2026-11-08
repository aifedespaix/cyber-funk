//! Le plan de passation de DJ : la caméra se tient derrière la voiture réelle (la nouvelle), un peu à gauche et plus haut
//! qu'une poursuite ordinaire, et regarde loin devant : on voit l'ancienne voiture, déportée à droite, que la nouvelle
//! rattrape puis double.
//!
//! Ce plan n'est jamais tiré par le montage automatique (`available` est faux) : le monde l'impose avec
//! `ShotOrder::Hold("handover")` de l'échange des voitures au dépassement, puis le relâche.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Écart latéral de l'œil à gauche de la voiture (m).
pub const LEFT_M: f64 = 2.8;
/// Hauteur de l'œil au-dessus de la route (m).
pub const EYE_HEIGHT_M: f64 = 2.6;
/// Recul de l'œil derrière la voiture (m).
pub const BEHIND_M: f64 = 11.0;
/// Distance du point visé devant la voiture (m).
pub const LOOK_AHEAD_M: f64 = 38.0;
/// Décalage latéral du point visé vers la droite, où se tient l'ancienne voiture (m).
pub const LOOK_RIGHT_M: f64 = 1.6;
/// Période du balancement lent de l'œil (s).
pub const SWAY_PERIOD_S: f32 = 14.0;
/// Amplitude du balancement latéral (m).
pub const SWAY_M: f32 = 0.7;
/// Focale (°).
pub const FOV_DEG: f32 = 56.0;

/// La fiche du plan de passation.
#[derive(Debug, Clone, Copy, Default)]
pub struct Handover;

impl ShotSpec for Handover {
    fn id(&self) -> &'static str {
        "handover"
    }

    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }

    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }

    fn min_seconds(&self) -> f32 {
        1.0
    }

    fn max_seconds(&self) -> f32 {
        30.0
    }

    fn cooldown(&self) -> f32 {
        0.0
    }

    fn weight(&self) -> f32 {
        0.0
    }

    fn entry(&self) -> TransitionKind {
        TransitionKind::Cut
    }

    /// Jamais tiré par le montage automatique : seul le monde l'impose.
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        false
    }

    fn create(&self, _frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(HandoverShot { t_s: 0.0 })
    }
}

#[derive(Debug, Clone, Copy)]
struct HandoverShot {
    t_s: f32,
}

/// La pose à `t_s` autour de la voiture `car` : `(position, point visé)`.
#[must_use]
pub fn pose(car: DVec3, t_s: f32) -> (DVec3, DVec3) {
    let sway = f64::from(SWAY_M * (std::f32::consts::TAU * t_s / SWAY_PERIOD_S).sin());
    let eye = car + DVec3::new(-LEFT_M + sway, EYE_HEIGHT_M, BEHIND_M);
    let target = car + DVec3::new(LOOK_RIGHT_M, 1.2, -LOOK_AHEAD_M);
    (eye, target)
}

impl Shot for HandoverShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t_s = (self.t_s + frame.dt) % SWAY_PERIOD_S;
        let (eye, target) = pose(frame.car, self.t_s);
        aim(out, eye, target, 0.0, Vec3::Y, false);
        out.fov_deg = FOV_DEG;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CAR: DVec3 = DVec3::new(0.0, 0.0, -500.0);

    #[test]
    fn the_camera_is_behind_and_above_the_car_looking_down_the_road() {
        for k in 0..300 {
            let (eye, target) = pose(CAR, k as f32 * 0.1);
            assert!(eye.z > CAR.z && eye.y > CAR.y, "{eye:?}");
            assert!(target.z < CAR.z, "le regard va vers l'avant");
        }
    }

    #[test]
    fn the_sway_is_bounded_and_loops() {
        let (a, _) = pose(CAR, 0.0);
        let (b, _) = pose(CAR, SWAY_PERIOD_S);
        assert!(a.distance(b) < 1e-3);
        for k in 0..1400 {
            let (eye, _) = pose(CAR, k as f32 * 0.01);
            assert!((eye.x + LEFT_M).abs() <= f64::from(SWAY_M) + 1e-6);
        }
    }

    #[test]
    fn the_shot_is_never_picked_by_the_automatic_editing() {
        assert_eq!(Handover.weight(), 0.0);
        assert_eq!(Handover.id(), "handover");
    }
}
