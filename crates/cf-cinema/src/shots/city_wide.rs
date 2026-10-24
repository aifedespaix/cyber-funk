//! Le plan très large (`cinema/shots/city-wide.ts`).

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Très large : on est loin et haut derrière la voiture, la ville s'étale jusqu'à l'horizon et le ciel prend la moitié
/// du cadre. Pour les breaks.
#[derive(Debug, Clone, Copy, Default)]
pub struct CityWide;

impl ShotSpec for CityWide {
    fn id(&self) -> &'static str {
        "city-wide"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.5)
    }
    fn min_seconds(&self) -> f32 {
        9.0
    }
    fn max_seconds(&self) -> f32 {
        16.0
    }
    fn cooldown(&self) -> f32 {
        90.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.altitude < 60.0
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(CityWideShot {
            rig: CarRig::new(2.2),
            side: rng.coin(),
            lateral: rng.between(22.0, 46.0),
            height: rng.between(44.0, 70.0),
            behind: rng.between(55.0, 76.0),
            fov: rng.between(76.0, 88.0),
            phase: rng.between(0.0, std::f32::consts::TAU),
            dip: rng.between(6.0, 20.0),
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct CityWideShot {
    rig: CarRig,
    side: f32,
    lateral: f32,
    height: f32,
    behind: f32,
    fov: f32,
    phase: f32,
    dip: f32,
    t: f32,
}

impl Shot for CityWideShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let t = self.t;
        let want = Vec3::new(
            self.side * self.lateral * (1.0 + 0.2 * (t * 0.12 + self.phase).sin()),
            self.height + 3.0 * (t * 0.09 + self.phase).sin(),
            self.behind - t * 0.5,
        );
        let look = Vec3::new(-self.side * 6.0, self.dip, -110.0);
        self.rig
            .track(frame, out, &RigGoal::new(want, look, self.fov + frame.beat.pulse * 1.5));
        true
    }
}
