//! Les plans qui accompagnent la voiture (`cinema/shots/car-follow.ts`, hors poursuite) : habitacle, pare-chocs, travelling
//! latéral, roue. La poursuite classique vit dans [`chase`](super::chase).

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, FOV_MAX, FOV_MIN, RigGoal, rush};
use crate::pose::{aim, follow};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Vue au volant : les yeux de la voiture, regard en coin qui dérive lentement.
#[derive(Debug, Clone, Copy, Default)]
pub struct Cockpit;

impl ShotSpec for Cockpit {
    fn id(&self) -> &'static str {
        "cockpit"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        6.0
    }
    fn max_seconds(&self) -> f32 {
        12.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        28.0
    }
    fn weight(&self) -> f32 {
        2.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Cut
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let base = rng.between(72.0, 86.0);
        // Chaque habitacle a son regard : un coup d'œil plus ou moins appuyé, qui dérive avec la caisse.
        let glance = rng.coin() * rng.between(0.0, 7.0);
        let glance_rate = rng.between(0.2, 0.5);
        Box::new(CockpitShot {
            base,
            fov: base,
            glance,
            glance_rate,
        })
    }
}

#[derive(Debug, Clone)]
struct CockpitShot {
    base: f32,
    fov: f32,
    glance: f32,
    glance_rate: f32,
}

impl Shot for CockpitShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let cabin = frame.cabin;
        // Les yeux sont ceux de la voiture : la tête penche, se cabre et dérive avec la caisse.
        let mut look = cabin.position + cabin.forward.as_dvec3() * 30.0;
        look.x += f64::from((frame.time as f32 * self.glance_rate).sin() * self.glance);
        aim(out, cabin.position, look, 0.0, cabin.up, false);
        out.cabin = true;
        let target = self.base + rush(frame) * 24.0 + frame.intensity * 5.0 + frame.beat.pulse * 2.0;
        self.fov += (target - self.fov) * follow(frame.dt, 3.0);
        out.fov_deg = self.fov.clamp(FOV_MIN, FOV_MAX);
        true
    }
}

/// Devant la voiture, regard vers l'arrière : elle fonce vers nous, la ville file derrière elle.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bumper;

impl ShotSpec for Bumper {
    fn id(&self) -> &'static str {
        "bumper"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }
    fn energy(&self) -> (f32, f32) {
        (0.15, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        5.0
    }
    fn max_seconds(&self) -> f32 {
        10.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        30.0
    }
    fn weight(&self) -> f32 {
        1.5
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let distance = rng.between(8.0, 12.0);
        let height = rng.between(0.8, 1.6);
        let lateral = rng.between(-1.4, 1.4);
        let lean = rng.coin() * rng.between(0.01, 0.06);
        let fov_base = rng.between(64.0, 80.0);
        let bob = rng.between(0.0, 0.35);
        Box::new(BumperShot {
            rig: CarRig::new(6.0),
            distance,
            height,
            lateral,
            lean,
            fov_base,
            bob,
        })
    }
}

#[derive(Debug, Clone)]
struct BumperShot {
    rig: CarRig,
    distance: f32,
    height: f32,
    lateral: f32,
    lean: f32,
    fov_base: f32,
    bob: f32,
}

impl Shot for BumperShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let r = rush(frame);
        let want = Vec3::new(
            self.lateral,
            self.height + (frame.time as f32 * 0.7).sin() * self.bob,
            -(self.distance - frame.beat.pulse * 0.6 - r * 1.5),
        );
        let look = Vec3::new(0.0, 0.9, 0.0);
        let fov = self.fov_base + r * 14.0 + frame.beat.pulse * 3.0;
        let roll = self.lean * (0.5 + frame.beat.bass * 2.0);
        self.rig
            .track(frame, out, &RigGoal::new(want, look, fov).with_roll(roll));
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Phrase {
            self.lateral = -self.lateral;
        }
    }
}

/// Travelling latéral : la caméra suit la voiture à la hauteur du bitume, de côté, et passe au-dessus d'elle au
/// changement de côté.
#[derive(Debug, Clone, Copy, Default)]
pub struct SideTrack;

impl ShotSpec for SideTrack {
    fn id(&self) -> &'static str {
        "side-track"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }
    fn energy(&self) -> (f32, f32) {
        (0.2, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        7.0
    }
    fn max_seconds(&self) -> f32 {
        14.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        22.0
    }
    fn weight(&self) -> f32 {
        2.5
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let distance = rng.between(6.5, 8.0);
        let height = rng.between(0.9, 1.6);
        let focal = rng.between(24.0, 40.0);
        let lead = rng.between(-3.0, 3.0);
        let swing = rng.between(1.5, 4.5);
        let swing_rate = rng.between(0.2, 0.45);
        Box::new(SideTrackShot {
            rig: CarRig::new(5.0),
            side,
            distance,
            height,
            focal,
            lead,
            swing,
            swing_rate,
        })
    }
}

#[derive(Debug, Clone)]
struct SideTrackShot {
    rig: CarRig,
    side: f32,
    distance: f32,
    height: f32,
    focal: f32,
    lead: f32,
    swing: f32,
    swing_rate: f32,
}

impl Shot for SideTrackShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        // Jamais au-delà du bord de chaussée : un immeuble commence vers |x| = 11.
        let car_x = frame.car.x as f32;
        let x = (car_x + self.side * self.distance).clamp(-8.6, 8.6) - car_x;
        // Au changement de côté la caméra passe au-dessus de la voiture au lieu de la traverser.
        let over = (5.0 - self.rig.rel.x.abs()).max(0.0);
        let want = Vec3::new(
            x,
            self.height + over,
            self.lead + (frame.time as f32 * self.swing_rate).sin() * self.swing,
        );
        let look = Vec3::new(0.0, 0.9, -1.0);
        let goal = RigGoal::new(want, look, self.focal + over * 5.0).with_rate(4.0);
        self.rig.track(frame, out, &goal);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Phrase {
            self.side = -self.side;
        }
    }
}

/// Près de la roue avant, au ras du bitume, grand-angle : le sol défile à toute vitesse.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wheel;

impl ShotSpec for Wheel {
    fn id(&self) -> &'static str {
        "wheel"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.3, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        4.0
    }
    fn max_seconds(&self) -> f32 {
        8.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        40.0
    }
    fn weight(&self) -> f32 {
        1.2
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Whip
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.trick.is_none()
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let behind = rng.between(2.0, 3.2);
        let fov = rng.between(88.0, 100.0);
        Box::new(WheelShot {
            rig: CarRig::new(7.0),
            side,
            behind,
            fov,
        })
    }
}

#[derive(Debug, Clone)]
struct WheelShot {
    rig: CarRig,
    side: f32,
    behind: f32,
    fov: f32,
}

impl Shot for WheelShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let eye = Vec3::new(
            self.side * 2.7,
            0.55,
            -self.behind + (frame.time as f32 * 0.9).sin() * 0.4,
        );
        let look = Vec3::new(self.side * 0.9, 0.5, -1.3);
        let fov = self.fov + frame.beat.pulse * 3.0;
        let roll = self.side * frame.beat.bass * 0.05;
        self.rig
            .track(frame, out, &RigGoal::new(eye, look, fov).with_roll(roll));
        true
    }
}
