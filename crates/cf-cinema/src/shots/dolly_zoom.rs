//! Le dolly-zoom calé sur le drop (`cinema/shots/dolly-zoom.ts`) : pendant la montée la caméra recule en refermant le
//! champ (la voiture garde sa taille, le décor s'écrase), puis sur le drop elle revient sur la voiture en ouvrant le
//! champ, et tout se détend d'un coup.

use std::f32::consts::PI;

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Distance de la voiture au début du plan, près (m).
const NEAR: f32 = 4.5;
/// Montée : durée (s) du recul avant que la caméra ne tienne en attente du drop.
const BUILD: f32 = 9.0;
/// Le drop : la caméra fonce vers la voiture et le champ s'ouvre d'un coup (s), puis on tient.
const HIT: f32 = 0.9;
/// Tenue après le drop (s).
const AFTER: f32 = 2.6;
/// Sans drop au bout de ce délai (s), le plan s'arrête.
const GIVE_UP: f32 = 18.0;

/// Champ (degrés) qui cadre une largeur `width` (m) à la distance `d` (m).
fn to_fov(width: f32, d: f32) -> f32 {
    (2.0 * (width / (2.0 * d)).atan()).to_degrees()
}

/// Dolly-zoom calé sur le drop : la voiture garde sa taille pendant que le décor se resserre, puis se détend au drop.
#[derive(Debug, Clone, Copy, Default)]
pub struct DollyZoom;

impl ShotSpec for DollyZoom {
    fn id(&self) -> &'static str {
        "dolly-zoom"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.3, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        6.0
    }
    fn max_seconds(&self) -> f32 {
        20.0
    }
    fn cooldown(&self) -> f32 {
        90.0
    }
    fn weight(&self) -> f32 {
        0.6
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.show.active && frame.show.drop_planned && !frame.show.drop_now && frame.trick.is_none()
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let far = rng.between(15.0, 17.5);
        let width = 2.0 * NEAR * ((rng.between(76.0, 86.0) * PI) / 360.0).tan();
        Box::new(DollyZoomShot {
            rig: CarRig::new(12.0),
            side,
            far,
            width,
            t: 0.0,
            hit_at: None,
        })
    }
}

#[derive(Debug, Clone)]
struct DollyZoomShot {
    rig: CarRig,
    side: f32,
    far: f32,
    width: f32,
    t: f32,
    /// Instant (s) du drop, une fois arrivé.
    hit_at: Option<f32>,
}

impl Shot for DollyZoomShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        if self.hit_at.is_none() && frame.show.drop_now {
            self.hit_at = Some(self.t);
        }
        let d = if let Some(hit) = self.hit_at {
            if self.t - hit > HIT + AFTER {
                return false;
            }
            self.far + (NEAR - self.far) * smootherstep((self.t - hit) / HIT)
        } else {
            if self.t > GIVE_UP {
                return false;
            }
            NEAR + (self.far - NEAR) * smoothstep(self.t / BUILD)
        };
        let heading = Vec3::new(self.side * 0.22, 0.16, 1.0).normalize();
        let want = heading * d;
        let look = Vec3::new(0.0, 1.0, -0.5);
        // Le saut de position au drop est coupé net, sinon le lissage le ferait glisser.
        let snap = self.hit_at.is_some_and(|hit| self.t - hit < 0.1);
        let goal = RigGoal::new(want, look, to_fov(self.width, d)).snapped(snap);
        self.rig.track(frame, out, &goal);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Drop && self.hit_at.is_none() {
            self.hit_at = Some(self.t);
        }
    }
}
