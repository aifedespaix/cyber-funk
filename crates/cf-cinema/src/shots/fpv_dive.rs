//! La plongée « drone FPV » (`cinema/shots/fpv-dive.ts`).

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// On tombe du ciel sur la voiture, on freine à ras du capot et le regard se redresse vers l'avant.
#[derive(Debug, Clone, Copy, Default)]
pub struct FpvDive;

impl ShotSpec for FpvDive {
    fn id(&self) -> &'static str {
        "fpv-dive"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Rare
    }
    fn energy(&self) -> (f32, f32) {
        (0.4, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        4.0
    }
    fn max_seconds(&self) -> f32 {
        7.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        200.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.altitude < 30.0 && frame.trick.is_none()
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let duration = rng.between(4.2, 5.4);
        let top = rng.between(80.0, 110.0);
        let bank = rng.between(0.15, 0.35) * side;
        Box::new(FpvDiveShot {
            rig: CarRig::new(16.0),
            side,
            duration,
            top,
            bank,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct FpvDiveShot {
    rig: CarRig,
    side: f32,
    duration: f32,
    top: f32,
    bank: f32,
    t: f32,
}

impl Shot for FpvDiveShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = (self.t / self.duration).min(1.0);
        // Chute accélérée puis freinage sec au-dessus du capot.
        let e = if p < 0.78 {
            0.86 * (p / 0.78).powi(2)
        } else {
            0.86 + 0.14 * smootherstep((p - 0.78) / 0.22)
        };
        let start = Vec3::new(self.side * 12.0, self.top, 22.0);
        let end = Vec3::new(0.0, 1.25, -1.9);
        let want = start.lerp(end, e);
        let look = Vec3::new(0.0, 0.8, 0.0).lerp(Vec3::new(0.0, 1.1, -30.0), smoothstep((p - 0.55) / 0.45));
        let fov =
            52.0 + 46.0 * ((e / 0.9).min(1.0) * std::f32::consts::FRAC_PI_2).sin() - 20.0 * smoothstep((p - 0.9) / 0.1);
        let roll = self.bank * (p * std::f32::consts::PI).sin();
        self.rig
            .track(frame, out, &RigGoal::new(want, look, fov).with_roll(roll));
        p < 1.0
    }
}
