//! Le plan de passation de DJ : la caméra se place **devant** la voiture qui roule seule sur la route nue, la regarde
//! venir, et on aperçoit au loin, derrière elle, la voiture du DJ suivant qui se forme puis la rejoint.
//!
//! Cinq styles (bas et proche, longue focale, rasant, grue, orbite lente) et des valeurs tirées par la passation : le
//! monde les publie dans `CinemaWorld::handover`, le réalisateur ne connaît rien du séquenceur. Le plan suit la voiture
//! qui roule seule (position et vitesse publiées), jamais la voiture réelle, qui est derrière.
//!
//! Ce plan n'est jamais tiré par le montage automatique (`available` est faux) : le monde l'impose avec
//! `ShotOrder::Hold("handover")` et le relâche à la fin.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Framing, HandoverView};

use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Durée sur laquelle le balayage de l'orbite se déroule (s).
pub const SWEEP_S: f32 = 24.0;
/// Hauteur du point visé au-dessus de la route (m).
const AIM_HEIGHT_M: f64 = 1.1;
/// Part de la vitesse de la voiture que la caméra garde en reculant devant elle : elle s'éloigne un peu moins vite que la
/// voiture n'approche, le plan se resserre doucement.
const CREEP: f64 = 0.015;
/// Période (s) et amplitude (m) du balancement vertical de la caméra, très lent.
const BOB_PERIOD_S: f32 = 11.0;
const BOB_M: f32 = 0.12;

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
        60.0
    }

    fn cooldown(&self) -> f32 {
        0.0
    }

    fn weight(&self) -> f32 {
        0.0
    }

    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
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

/// La pose pour un cadrage `f` autour de la voiture `lead` roulant à `speed_mps`, `t_s` secondes après le début du plan :
/// `(position, point visé)`. La route file vers −z : « devant » est −z.
#[must_use]
pub fn pose(lead: DVec3, speed_mps: f32, f: &Framing, t_s: f32) -> (DVec3, DVec3) {
    let bob = f64::from(BOB_M * (std::f32::consts::TAU * t_s / BOB_PERIOD_S).sin());
    // Le balayage va de −sweep/2 à +sweep/2 (style orbite) puis s'arrête : la caméra ne repart jamais en arrière.
    let sweep = f64::from(f.sweep_m) * (f64::from((t_s / SWEEP_S).clamp(0.0, 1.0)) - 0.5);
    // Un peu devant, de plus en plus près, comme si la voiture se rapprochait de l'objectif.
    let ahead = f64::from(f.ahead_m) - CREEP * f64::from(speed_mps) * f64::from(t_s);
    let eye = lead
        + DVec3::new(
            f64::from(f.lateral_m) + sweep,
            f64::from(f.height_m) + bob,
            -ahead.max(6.0),
        );
    let target = lead + DVec3::new(0.0, AIM_HEIGHT_M, f64::from(f.look_back_m));
    (eye, target)
}

impl Shot for HandoverShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t_s += frame.dt;
        // Sans passation publiée (le plan est tenu un instant avant la première image du monde) : on garde la pose.
        let Some(HandoverView {
            lead,
            speed_mps,
            framing,
            ..
        }) = frame.handover
        else {
            return true;
        };
        let (eye, target) = pose(lead, speed_mps, &framing, self.t_s);
        aim(out, eye, target, 0.0, Vec3::Y, false);
        out.fov_deg = framing.fov_deg;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::FRAMING_STYLES;

    const LEAD: DVec3 = DVec3::new(0.0, 0.0, -500.0);

    fn framing(style: u8) -> Framing {
        Framing {
            style,
            ..Framing::NEUTRAL
        }
    }

    #[test]
    fn the_camera_is_ahead_of_the_lone_car_and_looks_back_at_it() {
        for style in 0..FRAMING_STYLES {
            for k in 0..300 {
                let (eye, target) = pose(LEAD, 9.0, &framing(style), k as f32 * 0.1);
                assert!(eye.z < LEAD.z, "devant la voiture (vers −z) : {eye:?}");
                assert!(
                    target.z > eye.z,
                    "le regard revient vers la voiture et la route derrière"
                );
                assert!(eye.y > 0.2, "jamais sous la chaussée : {eye:?}");
            }
        }
    }

    #[test]
    fn the_camera_never_gets_closer_than_six_metres() {
        let tight = Framing {
            ahead_m: 7.0,
            ..Framing::NEUTRAL
        };
        for k in 0..2000 {
            let (eye, _) = pose(LEAD, 30.0, &tight, k as f32 * 0.1);
            assert!(LEAD.z - eye.z >= 6.0 - 1e-9);
        }
    }

    #[test]
    fn the_orbit_sweeps_once_and_stops() {
        let orbit = Framing {
            sweep_m: 8.0,
            ..Framing::NEUTRAL
        };
        let (a, _) = pose(LEAD, 0.0, &orbit, 0.0);
        let (b, _) = pose(LEAD, 0.0, &orbit, SWEEP_S);
        let (c, _) = pose(LEAD, 0.0, &orbit, SWEEP_S * 3.0);
        assert!((b.x - a.x - 8.0).abs() < 1e-6 && (c.x - b.x).abs() < 0.2);
    }

    #[test]
    fn the_shot_is_never_picked_by_the_automatic_editing() {
        assert_eq!(Handover.weight(), 0.0);
        assert_eq!(Handover.id(), "handover");
    }
}
