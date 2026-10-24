//! Le travelling avant de préparation (`cinema/shots/push-in.ts`) : on part en retrait derrière la voiture et on s'en
//! rapproche en resserrant la focale.

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::pose::smootherstep;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Fiche du travelling avant. Écrit à la seconde près (`scripted`) : sa durée n'est pas étirée par le rythme.
#[derive(Debug, Clone, Copy, Default)]
pub struct PushIn;

impl ShotSpec for PushIn {
    fn id(&self) -> &'static str {
        "push-in"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.3, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        8.0
    }
    fn max_seconds(&self) -> f32 {
        14.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        40.0
    }
    fn weight(&self) -> f32 {
        0.8
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.trick.is_none()
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let rig = CarRig::new(6.0);
        let side = rng.coin();
        let duration = rng.between(8.0, 12.0);
        let start_back = rng.between(13.0, 18.0);
        let end_back = rng.between(3.4, 4.6);
        let start_height = rng.between(1.2, 2.2);
        let end_height = rng.between(1.6, 2.6);
        let offset = rng.between(2.2, 3.6);
        let fov0 = rng.between(72.0, 82.0);
        let fov1 = rng.between(46.0, 56.0);
        Box::new(PushInShot {
            rig,
            side,
            duration,
            start_back,
            end_back,
            start_height,
            end_height,
            offset,
            fov0,
            fov1,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct PushInShot {
    rig: CarRig,
    side: f32,
    duration: f32,
    start_back: f32,
    end_back: f32,
    start_height: f32,
    end_height: f32,
    offset: f32,
    fov0: f32,
    fov1: f32,
    t: f32,
}

impl Shot for PushInShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = smootherstep(self.t / self.duration);
        let want = Vec3::new(
            self.side * self.offset * (1.0 - 0.55 * p),
            self.start_height + (self.end_height - self.start_height) * p,
            self.start_back + (self.end_back - self.start_back) * p,
        );
        let look = Vec3::new(0.0, 1.0, -6.0 - 4.0 * p);
        let fov = self.fov0 + (self.fov1 - self.fov0) * p + frame.beat.pulse * 1.5;
        self.rig.track(frame, out, &RigGoal::new(want, look, fov));
        self.t < self.duration + 4.0
    }
}
