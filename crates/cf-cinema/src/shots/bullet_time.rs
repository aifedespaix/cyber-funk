//! Le bullet-time (`cinema/shots/bullet-time.ts`).

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::pose::smootherstep;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Temps (s) après le ralenti pendant lesquels la caméra finit son tour, le monde reprenant son allure.
const TAIL: f32 = 1.6;
/// Niveau minimal du spectacle pour jouer ce plan.
const MIN_LEVEL: u8 = 4;

/// Sur un drop, le monde ralentit (0,15 à 0,25) pendant une ou deux mesures et la caméra, qui garde le temps réel,
/// tourne autour de la voiture. Réservé aux hauts niveaux, et seulement sur un drop.
///
/// Le ralenti lui-même est demandé au chef du spectacle par [`Shot::take_time_scale`].
#[derive(Debug, Clone, Copy, Default)]
pub struct BulletTime;

impl ShotSpec for BulletTime {
    fn id(&self) -> &'static str {
        "bullet-time"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Epic
    }
    fn energy(&self) -> (f32, f32) {
        (0.4, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        3.5
    }
    fn max_seconds(&self) -> f32 {
        9.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        300.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Cut
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.show.active
            && frame.show.level >= MIN_LEVEL
            && (frame.show.drop_now || frame.drop)
            && frame.altitude < 80.0
            && frame.trick.is_none()
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let bpm = frame.beat.bpm.filter(|b| *b > 0.0).unwrap_or(120.0).clamp(90.0, 180.0);
        let bars = if rng.next_f32() < 0.5 { 1.0 } else { 2.0 };
        let slow = (bars * (240.0 / bpm)).clamp(1.6, 4.5);
        let scale = rng.between(0.15, 0.25);
        let dir = rng.coin();
        let radius = rng.between(6.5, 8.5);
        let height = rng.between(1.4, 2.6);
        let turns = rng.between(0.85, 1.1);
        let start = rng.between(0.4, 1.2);
        Box::new(BulletTimeShot {
            rig: CarRig::new(10.0),
            request: Some((scale, slow)),
            total: slow + TAIL,
            dir,
            radius,
            height,
            turns,
            start,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct BulletTimeShot {
    rig: CarRig,
    /// Le ralenti à demander, tant qu'il n'a pas été pris.
    request: Option<(f32, f32)>,
    total: f32,
    dir: f32,
    radius: f32,
    height: f32,
    turns: f32,
    start: f32,
    t: f32,
}

impl Shot for BulletTimeShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = smootherstep(self.t / self.total);
        let angle = std::f32::consts::FRAC_PI_2 + self.start + self.dir * std::f32::consts::TAU * self.turns * p;
        let car_x = frame.car.x as f32;
        let x = (car_x + angle.cos() * self.radius).clamp(-9.5, 9.5) - car_x;
        let swell = (p * std::f32::consts::PI).sin();
        let want = Vec3::new(x, self.height + 0.8 * swell, angle.sin() * self.radius);
        let look = Vec3::new(0.0, 1.0, -0.3);
        let goal = RigGoal::new(want, look, 58.0 - 12.0 * swell).with_roll(0.08 * self.dir * swell);
        self.rig.track(frame, out, &goal);
        self.t < self.total
    }

    fn holding(&self) -> bool {
        self.t < self.total
    }

    fn take_time_scale(&mut self) -> Option<(f32, f32)> {
        self.request.take()
    }
}
