//! Les plans à mouvement écrit (`cinema/shots/car-motion.ts`) : grue, dolly-zoom sur la voiture, bascule vers le ciel,
//! caméra à l'épaule penchée.

use std::f32::consts::PI;

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal, rush};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Grue : part bas derrière, passe au-dessus de la voiture et finit haut devant, en la regardant s'éloigner.
#[derive(Debug, Clone, Copy, Default)]
pub struct Crane;

impl ShotSpec for Crane {
    fn id(&self) -> &'static str {
        "crane"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.8)
    }
    fn min_seconds(&self) -> f32 {
        9.0
    }
    fn max_seconds(&self) -> f32 {
        13.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        50.0
    }
    fn weight(&self) -> f32 {
        1.3
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let duration = rng.between(9.0, 11.0);
        let top = rng.between(18.0, 26.0);
        let far = rng.between(14.0, 20.0);
        Box::new(CraneShot {
            rig: CarRig::new(14.0),
            side,
            duration,
            top,
            far,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct CraneShot {
    rig: CarRig,
    side: f32,
    duration: f32,
    top: f32,
    far: f32,
    t: f32,
}

impl Shot for CraneShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = smootherstep(self.t / self.duration);
        let start = Vec3::new(self.side * 3.5, 1.6, 8.0);
        let end = Vec3::new(-self.side * 2.5, self.top, -self.far);
        let mut want = start.lerp(end, p);
        want.y += (p * PI).sin() * 3.0;
        let look = Vec3::new(0.0, 1.0 + p * 0.5, -1.0);
        let fov = 72.0 - 17.0 * p + frame.beat.pulse * 2.0;
        self.rig.track(frame, out, &RigGoal::new(want, look, fov));
        self.t < self.duration + 1.0
    }
}

/// Dolly-zoom sans drop : on recule en refermant le champ, la voiture garde sa taille, le décor s'écrase derrière elle.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vertigo;

/// Distance de la caméra au début du plan (m), et largeur cadrée à la voiture (m) pour la focale.
const VERTIGO_NEAR: f32 = 4.5;

impl ShotSpec for Vertigo {
    fn id(&self) -> &'static str {
        "vertigo"
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
        9.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        50.0
    }
    fn weight(&self) -> f32 {
        1.2
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let duration = rng.between(5.5, 7.5);
        let far = rng.between(15.0, 17.0);
        let reverse = rng.next_f64() < 0.3;
        // Largeur cadrée à la voiture, gardée constante : fov(d) = 2·atan(largeur / 2d).
        let width = 2.0 * VERTIGO_NEAR * ((rng.between(78.0, 88.0) * PI) / 360.0).tan();
        Box::new(VertigoShot {
            rig: CarRig::new(12.0),
            side,
            duration,
            far,
            reverse,
            width,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct VertigoShot {
    rig: CarRig,
    side: f32,
    duration: f32,
    far: f32,
    reverse: bool,
    width: f32,
    t: f32,
}

impl Shot for VertigoShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = smoothstep(self.t / self.duration);
        let d = if self.reverse {
            self.far + (VERTIGO_NEAR - self.far) * p
        } else {
            VERTIGO_NEAR + (self.far - VERTIGO_NEAR) * p
        };
        let heading = Vec3::new(self.side * 0.22, 0.16, 1.0).normalize();
        let want = heading * d;
        let look = Vec3::new(0.0, 1.0, -0.5);
        let fov = (2.0 * (self.width / (2.0 * d)).atan()).to_degrees();
        self.rig.track(frame, out, &RigGoal::new(want, look, fov));
        self.t < self.duration + 0.8
    }
}

/// Bascule vers le ciel : on regarde la voiture, la caméra tourne vers la géante à anneau (ou le zénith), tient, puis
/// revient.
#[derive(Debug, Clone, Copy, Default)]
pub struct SkyTilt;

impl ShotSpec for SkyTilt {
    fn id(&self) -> &'static str {
        "sky-tilt"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.7)
    }
    fn min_seconds(&self) -> f32 {
        10.0
    }
    fn max_seconds(&self) -> f32 {
        14.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        70.0
    }
    fn weight(&self) -> f32 {
        0.9
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let duration = rng.between(10.0, 13.0);
        let yaw = rng.between(-0.7, 0.2);
        // Vers la géante à anneau si on la connaît (le ciel suit la caméra : sa direction est son centre), sinon vers le
        // zénith. Un centre nul (pas de direction) retombe sur le zénith plutôt que sur un NaN.
        let zenith = Vec3::new(yaw.sin() * 0.6, 0.8, -yaw.cos() * 0.6).normalize();
        let dir = frame
            .ring_planet
            .and_then(|planet| planet.center.try_normalize())
            .unwrap_or(zenith);
        Box::new(SkyTiltShot {
            rig: CarRig::new(14.0),
            duration,
            dir,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct SkyTiltShot {
    rig: CarRig,
    duration: f32,
    dir: Vec3,
    t: f32,
}

impl Shot for SkyTiltShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = self.t / self.duration;
        // Regard sur la voiture, bascule vers le ciel, tient, puis revient.
        let e = smootherstep((p - 0.2) / 0.25) - smootherstep((p - 0.7) / 0.25);
        let want = Vec3::new(0.0, 1.5, 5.5);
        let sky = want + self.dir * 90.0;
        let look = Vec3::new(0.0, 1.0, 0.0).lerp(sky, e);
        let fov = 70.0 - 10.0 * e + frame.beat.pulse * 2.0;
        self.rig.track(frame, out, &RigGoal::new(want, look, fov));
        p < 1.0
    }
}

/// Caméra à l'épaule, penchée : le roulis (11° au plus) balance sur chaque kick.
#[derive(Debug, Clone, Copy, Default)]
pub struct Dutch;

impl ShotSpec for Dutch {
    fn id(&self) -> &'static str {
        "dutch"
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
        12.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        35.0
    }
    fn weight(&self) -> f32 {
        1.3
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let tilt = rng.coin();
        Box::new(DutchShot {
            rig: CarRig::new(5.0),
            side,
            tilt,
        })
    }
}

#[derive(Debug, Clone)]
struct DutchShot {
    rig: CarRig,
    side: f32,
    tilt: f32,
}

impl Shot for DutchShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let r = rush(frame);
        let want = Vec3::new(self.side * 1.9, 1.9 + (frame.time as f32 * 0.5).sin() * 0.15, 2.9);
        let look = Vec3::new(-self.side * 0.4, 1.0, -7.0);
        let fov = 70.0 + r * 14.0 + frame.beat.pulse * 3.0;
        let roll = self.tilt * (0.09 + 0.1 * frame.beat.pulse);
        self.rig
            .track(frame, out, &RigGoal::new(want, look, fov).with_roll(roll));
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Phrase {
            self.tilt = -self.tilt;
        }
    }
}
