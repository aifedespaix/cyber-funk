//! Le plan de coupe de la voiture (`cinema/shots/car-cut.ts`) : caméra fixe au bord de la route, la voiture passe devant
//! elle plusieurs fois. Le « strobe » de sommet n'est pas porté ici (J6-02 le traite avec le limiteur de cadence).

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::kit::{FOV_MAX, FOV_MIN, fork};
use crate::pose::{aim, follow, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Au-delà de cette avance (m) la voiture a dépassé la caméra : on peut la replacer.
const PASSED: f64 = 40.0;
/// Au-delà de cette distance (m) la voiture est très loin : on replace la caméra sans attendre la mesure.
const RESET_FAR: f64 = 130.0;

/// Caméra sur le bas-côté qui laisse passer la voiture, de 2 à 3 fois, et se replace à chaque mesure.
#[derive(Debug, Clone, Copy, Default)]
pub struct Flyby;

impl ShotSpec for Flyby {
    fn id(&self) -> &'static str {
        "flyby"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.25, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        8.0
    }
    fn max_seconds(&self) -> f32 {
        16.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        35.0
    }
    fn weight(&self) -> f32 {
        1.5
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Whip
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.trick.is_none()
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let goal = 2 + (rng.next_f64() * 2.0) as u32;
        let side = rng.coin();
        Box::new(FlybyShot {
            rng: fork(rng),
            goal,
            side,
            passes: 0,
            waiting: false,
            placed: false,
            bar: false,
            fov: 48.0,
            cam: DVec3::ZERO,
            focus: DVec3::ZERO,
        })
    }
}

#[derive(Debug, Clone)]
struct FlybyShot {
    /// Tirages faits en cours de route (la place de la caméra à chaque passage).
    rng: CinemaRng,
    goal: u32,
    side: f32,
    passes: u32,
    waiting: bool,
    placed: bool,
    bar: bool,
    fov: f32,
    cam: DVec3,
    focus: DVec3,
}

impl FlybyShot {
    /// Pose la caméra devant la voiture sur le bas-côté (|x| ≈ 7–8), basse, hors de tout immeuble.
    fn place(&mut self, frame: &CinemaFrame<'_>) {
        let ahead = f64::from(self.rng.between(55.0, 80.0));
        for tries in 0..4 {
            let x = self.side * self.rng.between(7.0, 8.0);
            let y = frame.car.y + f64::from(self.rng.between(0.8, 2.2));
            self.cam = DVec3::new(f64::from(x), y, frame.car.z - ahead);
            if !frame.collides(self.cam, 1.5) {
                break;
            }
            self.side = -self.side;
            if tries == 3 {
                self.cam.x *= 0.6;
                self.cam.y += 6.0;
            }
        }
        self.focus = frame.car;
        self.placed = true;
        self.waiting = false;
        self.bar = false;
    }
}

impl Shot for FlybyShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        if !self.placed {
            self.place(frame);
        }
        // Elle pivote pour suivre la voiture (légèrement en avance), avec un peu de retard : le passage reste vivant.
        let target = frame.car + frame.car_velocity.as_dvec3() * 0.1;
        self.focus = self.focus.lerp(target, f64::from(follow(frame.dt, 7.0)));
        aim(out, self.cam, self.focus, 0.0, Vec3::Y, false);
        let past = self.cam.z - frame.car.z;
        let distance = (frame.car - self.cam).length() as f32;
        let closeness = smoothstep(1.0 - distance / 45.0);
        self.fov += (42.0 + 26.0 * closeness - self.fov) * follow(frame.dt, 6.0);
        out.fov_deg = self.fov.clamp(FOV_MIN, FOV_MAX);
        if !self.waiting && past > PASSED {
            self.passes += 1;
            self.waiting = true;
        }
        if self.waiting {
            if self.passes >= self.goal {
                return false;
            }
            // Repositionnée sur la mesure suivante (ou tout de suite si la voiture est déjà très loin).
            if self.bar || past > RESET_FAR {
                self.place(frame);
            }
        }
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Bar {
            self.bar = true;
        }
    }
}
