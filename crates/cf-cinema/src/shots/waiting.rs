//! Le plan d'attente : avant `Démarrer`, la caméra est posée au ras de la route, face à l'horizon, **devant**
//! l'emplacement de la voiture. La voiture est donc hors champ, derrière elle : à l'ouverture elle entrera dans l'image
//! par l'arrière, sans jamais « apparaître » (plan `opening`). Une dérive très lente, en boucle, garde l'image vivante.
//!
//! Ce plan n'est jamais tiré par le montage automatique (`available` est faux) : le monde l'impose avec
//! `ShotOrder::Hold("waiting")` en attente. La clôture (plan `closing`) vient se poser exactement sur cette pose.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Période de la dérive (s) : la pose en `t` et en `t + PERIOD_S` est la même, la boucle est sans raccord.
pub const PERIOD_S: f32 = 60.0;
/// Dérive latérale maximale (m) : au ras du sol, quelques décimètres suffisent à faire glisser les lignes.
pub const SWAY_X_M: f32 = 0.5;
/// Dérive verticale maximale (m).
pub const BOB_Y_M: f32 = 0.08;
/// Dérive d'avance maximale (m).
pub const SWAY_Z_M: f32 = 1.2;
/// Hauteur de l'œil au-dessus de la route (m) : au ras de l'asphalte, la voiture passera à hauteur de regard.
pub const EYE_HEIGHT_M: f64 = 0.62;
/// Avance de la caméra devant l'emplacement de départ de la voiture (m) : la voiture, derrière, est hors champ.
pub const AHEAD_M: f64 = 46.0;
/// Écart latéral de la caméra par rapport à la trajectoire de la voiture (m) : elle passera tout près, sans la heurter.
pub const SIDE_M: f64 = 2.4;
/// Distance du point visé, vers l'horizon (m).
pub const LOOK_AHEAD_M: f64 = 140.0;
/// Hauteur du point visé au-dessus de la route (m) : l'horizon, un peu relevé.
pub const LOOK_UP_M: f64 = 7.0;
/// Respiration de l'angle de visée (rad) : ≤ 1°.
pub const BREATH_RAD: f32 = 0.012;
/// Focale (°).
pub const FOV_DEG: f32 = 52.0;

/// La pose de repos autour de l'emplacement `anchor` de la voiture : `(position, point visé)`. C'est la pose commune
/// à l'attente, au début de l'ouverture et à la fin de la clôture : les trois se raccordent sans saut.
#[must_use]
pub fn rest(anchor: DVec3) -> (DVec3, DVec3) {
    let eye = anchor + DVec3::new(SIDE_M, EYE_HEIGHT_M, -AHEAD_M);
    // Le regard converge vers le point de fuite de la route, pas vers le bas-côté.
    let target = anchor + DVec3::new(0.0, LOOK_UP_M, -AHEAD_M - LOOK_AHEAD_M);
    (eye, target)
}

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

    /// Court : tenu par le monde il reste à l'antenne quel que soit ce délai, mais une fois relâché le montage
    /// automatique doit pouvoir couper vite (sinon la caméra resterait ~60 s sur le point de départ).
    fn min_seconds(&self) -> f32 {
        1.0
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

    /// La caméra est loin devant la voiture : les bornes ordinaires du réalisateur ne s'appliquent pas.
    fn wide_view(&self) -> bool {
        true
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
    let (rest_eye, rest_target) = rest(anchor);
    // L'angle respire : le point visé glisse latéralement, sans jamais dépasser ~1° de visée.
    let breath = f64::from(BREATH_RAD * (phase * 2.0 + 0.7).sin()) * LOOK_AHEAD_M;
    (rest_eye + sway, rest_target + DVec3::new(breath, 0.0, 0.0))
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
            assert!(eye.y - ANCHOR.y < 4.0 && eye.y - ANCHOR.y > 0.4, "{eye:?}");
            assert!(target.z < eye.z, "le regard va vers la route, l'horizon devant");
            assert!(eye.z < ANCHOR.z - 30.0, "la voiture est derrière la caméra, hors champ");
            assert!(
                (eye.x - ANCHOR.x).abs() > 1.5,
                "la voiture passera à côté, pas à travers"
            );
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
        let (rest, _) = rest(ANCHOR);
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
        let rest = rest(ANCHOR).1.x;
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
