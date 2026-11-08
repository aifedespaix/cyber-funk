//! Le plan d'attente : avant `Démarrer`, la caméra reste bas sur la route, face à l'horizon, avec une dérive lente en
//! boucle. Rien ne bouge d'autre : la voiture n'est pas là, aucune musique ne joue.
//!
//! Ce plan n'est jamais tiré par le montage automatique (`available` est faux) : le monde l'impose avec
//! `ShotOrder::Hold("waiting")` en attente, et le relâche quand la voiture apparaît.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Période de la dérive (s) : la pose en `t` et en `t + PERIOD_S` est la même, la boucle est sans raccord.
pub const PERIOD_S: f32 = 60.0;
/// Dérive latérale maximale (m).
pub const SWAY_X_M: f32 = 6.0;
/// Dérive verticale maximale (m).
pub const BOB_Y_M: f32 = 0.4;
/// Dérive d'avance maximale (m).
pub const SWAY_Z_M: f32 = 3.0;
/// Hauteur de l'œil au-dessus de la route (m).
pub const EYE_HEIGHT_M: f64 = 1.1;
/// Recul de la caméra derrière l'emplacement de départ de la voiture (m).
pub const BEHIND_M: f64 = 14.0;
/// Distance du point visé, vers l'horizon (m).
pub const LOOK_AHEAD_M: f64 = 140.0;
/// Hauteur du point visé au-dessus de la route (m) : l'horizon, un peu relevé.
pub const LOOK_UP_M: f64 = 7.0;
/// Respiration de l'angle de visée (rad) : ≤ 1°.
pub const BREATH_RAD: f32 = 0.012;
/// Focale (°).
pub const FOV_DEG: f32 = 52.0;

/// La fiche du plan d'attente.
#[derive(Debug, Clone, Copy, Default)]
pub struct Waiting;

impl ShotSpec for Waiting {
    fn id(&self) -> &'static str {
        "waiting"
    }

    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }

    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }

    fn min_seconds(&self) -> f32 {
        PERIOD_S
    }

    fn max_seconds(&self) -> f32 {
        PERIOD_S
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

    fn create(&self, frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(WaitingShot {
            anchor: frame.car,
            t_s: 0.0,
        })
    }
}

/// Le plan en cours : l'emplacement de départ de la voiture, et le temps écoulé.
#[derive(Debug, Clone, Copy)]
struct WaitingShot {
    anchor: DVec3,
    t_s: f32,
}

/// La pose de la dérive à `t_s` : `(position, point visé)` autour de `anchor`.
#[must_use]
pub fn drift(anchor: DVec3, t_s: f32) -> (DVec3, DVec3) {
    let phase = std::f32::consts::TAU * t_s / PERIOD_S;
    let sway = DVec3::new(
        f64::from(SWAY_X_M * phase.sin()),
        f64::from(BOB_Y_M * (2.0 * phase).sin()),
        f64::from(SWAY_Z_M * (phase + 1.1).sin()),
    );
    let eye = anchor + DVec3::new(0.0, EYE_HEIGHT_M, BEHIND_M) + sway;
    // L'angle respire : le point visé glisse latéralement, sans jamais dépasser ~1° de visée.
    let breath = f64::from(BREATH_RAD * (phase * 2.0 + 0.7).sin()) * LOOK_AHEAD_M;
    let target = anchor + DVec3::new(breath, LOOK_UP_M, -LOOK_AHEAD_M);
    (eye, target)
}

impl Shot for WaitingShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        // Boucle sur la période : le temps ne grossit pas sans fin (précision `f32`).
        self.t_s = (self.t_s + frame.dt) % PERIOD_S;
        let (eye, target) = drift(self.anchor, self.t_s);
        aim(out, eye, target, 0.0, Vec3::Y, false);
        out.fov_deg = FOV_DEG;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANCHOR: DVec3 = DVec3::new(0.0, 0.0, -500.0);

    #[test]
    fn waiting_pose_is_low_over_road_looking_at_the_horizon() {
        for k in 0..600 {
            let (eye, target) = drift(ANCHOR, k as f32 * 0.1);
            assert!(eye.y - ANCHOR.y < 4.0 && eye.y - ANCHOR.y > 0.5, "{eye:?}");
            assert!(target.z < eye.z, "le regard va vers la route, l'horizon devant");
        }
    }

    #[test]
    fn waiting_loops_seamlessly() {
        let (a, ta) = drift(ANCHOR, 0.0);
        let (b, tb) = drift(ANCHOR, PERIOD_S);
        assert!(a.distance(b) < 1e-3 && ta.distance(tb) < 1e-3);
    }

    #[test]
    fn waiting_amplitudes_bounded() {
        let mut max = DVec3::ZERO;
        let rest = ANCHOR + DVec3::new(0.0, EYE_HEIGHT_M, BEHIND_M);
        for k in 0..6000 {
            let (eye, _) = drift(ANCHOR, k as f32 * 0.01);
            max = max.max((eye - rest).abs());
        }
        assert!(max.x <= f64::from(SWAY_X_M) + 1e-3);
        assert!(max.y <= f64::from(BOB_Y_M) + 1e-3);
        assert!(max.z <= f64::from(SWAY_Z_M) + 1e-3);
    }

    #[test]
    fn view_angle_breathes_under_one_degree() {
        let rest = (ANCHOR + DVec3::new(0.0, LOOK_UP_M, -LOOK_AHEAD_M)).x;
        for k in 0..6000 {
            let (_, target) = drift(ANCHOR, k as f32 * 0.01);
            let angle = ((target.x - rest) / LOOK_AHEAD_M).atan().to_degrees();
            assert!(angle.abs() <= 1.0, "{angle}");
        }
    }

    #[test]
    fn the_automatic_edit_never_picks_it() {
        assert_eq!(Waiting.weight(), 0.0);
    }
}
