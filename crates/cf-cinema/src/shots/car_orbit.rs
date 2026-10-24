//! Les plans qui tournent autour de la voiture ou la survolent (`cinema/shots/car-orbit.ts`) : orbite, vue de dessus,
//! hélice, vue de dieu.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::pose::{follow, smootherstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Une orbite autour de la voiture, à la hauteur du toit, qui change de sens sur chaque phrase.
#[derive(Debug, Clone, Copy, Default)]
pub struct Orbit;

impl ShotSpec for Orbit {
    fn id(&self) -> &'static str {
        "orbit"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        8.0
    }
    fn max_seconds(&self) -> f32 {
        18.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        20.0
    }
    fn weight(&self) -> f32 {
        3.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let angle = rng.between(0.0, TAU);
        let dir = rng.coin();
        let base = rng.between(8.0, 11.0);
        let lift = rng.between(2.2, 5.0);
        let speed = rng.between(0.7, 1.4);
        let ellipse = rng.between(0.8, 1.25);
        let fov_base = rng.between(52.0, 64.0);
        Box::new(OrbitShot {
            rig: CarRig::new(5.0),
            angle,
            dir,
            spin: 0.0,
            base,
            lift,
            speed,
            ellipse,
            fov_base,
        })
    }
}

#[derive(Debug, Clone)]
struct OrbitShot {
    rig: CarRig,
    angle: f32,
    dir: f32,
    spin: f32,
    base: f32,
    lift: f32,
    speed: f32,
    ellipse: f32,
    fov_base: f32,
}

impl Shot for OrbitShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        // Le sens s'inverse sur une phrase, mais la vitesse angulaire glisse d'une valeur à l'autre : pas de demi-tour sec.
        let target = self.dir * self.speed * (0.22 + frame.intensity * 0.5 + frame.beat.pulse * 0.1);
        self.spin += (target - self.spin) * follow(frame.dt, 1.5);
        self.angle += self.spin * frame.dt;
        let radius = (self.base * (1.0 - 0.18 * frame.beat.bass) + (frame.time as f32 * 0.2).sin() * 1.5).max(6.5);
        // Bornée à la chaussée : au pire la caméra reste à ~5 m de l'axe de la voiture, jamais dans un immeuble.
        let car_x = frame.car.x as f32;
        let x = (car_x + self.angle.cos() * radius).clamp(-9.5, 9.5) - car_x;
        let want = Vec3::new(
            x,
            self.lift + (frame.time as f32 * 0.17).sin() * 1.5 + frame.intensity,
            self.angle.sin() * radius * self.ellipse,
        );
        let look = Vec3::new(0.0, 1.1, -(0.5 + frame.intensity * 2.0));
        let fov = self.fov_base + frame.beat.pulse * 2.0 + frame.intensity * 12.0;
        self.rig.track(frame, out, &RigGoal::new(want, look, fov));
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Phrase {
            self.dir = -self.dir;
        }
    }
}

/// Vue de dessus, au-dessus des tabliers des arches (35 m) : la rotation avance d'un cran à chaque mesure.
#[derive(Debug, Clone, Copy, Default)]
pub struct TopDown;

impl ShotSpec for TopDown {
    fn id(&self) -> &'static str {
        "top-down"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
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
        45.0
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
        let height = rng.between(40.0, 52.0);
        let dir = rng.coin();
        let target = rng.between(-0.4, 0.4);
        Box::new(TopDownShot {
            rig: CarRig::new(4.0),
            height,
            dir,
            target,
            turn: target,
        })
    }
}

#[derive(Debug, Clone)]
struct TopDownShot {
    rig: CarRig,
    height: f32,
    dir: f32,
    target: f32,
    turn: f32,
}

impl Shot for TopDownShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.turn += (self.target - self.turn) * follow(frame.dt, 1.2);
        let want = Vec3::new(0.0, self.height + frame.beat.pulse * 0.8, 0.1);
        let look = Vec3::new(0.0, 0.0, -1.0);
        let up = Vec3::new(self.turn.sin(), 0.0, -self.turn.cos());
        let fov = 56.0 + frame.beat.pulse * 2.0 + frame.beat.bass * 4.0;
        self.rig.track(frame, out, &RigGoal::new(want, look, fov).with_up(up));
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Bar {
            self.target += self.dir * (PI / 8.0);
        }
    }
}

/// Hélice : elle part de derrière la voiture, monte, fait un tour complet et redescend se poser derrière.
#[derive(Debug, Clone, Copy, Default)]
pub struct Spiral;

impl ShotSpec for Spiral {
    fn id(&self) -> &'static str {
        "spiral"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.3, 1.0)
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
        60.0
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
        let total = rng.between(11.0, 14.0);
        let dir = rng.coin();
        let outer = rng.between(14.0, 17.0);
        let peak = rng.between(24.0, 34.0);
        Box::new(SpiralShot {
            rig: CarRig::new(9.0),
            total,
            dir,
            outer,
            peak,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct SpiralShot {
    rig: CarRig,
    total: f32,
    dir: f32,
    outer: f32,
    peak: f32,
    t: f32,
}

impl Shot for SpiralShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let p = (self.t / self.total).min(1.0);
        let e = smootherstep((p * PI).sin());
        let angle = FRAC_PI_2 + self.dir * TAU * p;
        let radius = 8.5 + (self.outer - 8.5) * e;
        let height = 1.8 + self.peak * e;
        let limit = if height < 30.0 { 10.5 } else { 40.0 };
        let car_x = frame.car.x as f32;
        let x = (car_x + angle.cos() * radius).clamp(-limit, limit) - car_x;
        let want = Vec3::new(x, height, angle.sin() * radius);
        let look = Vec3::new(0.0, 1.0 + height * 0.1, -2.0);
        self.rig.track(frame, out, &RigGoal::new(want, look, 62.0 + 14.0 * e));
        p < 1.0
    }
}

/// Très haut, large, en lente dérive : la route devient un ruban de lumière dans la ville. Rare.
#[derive(Debug, Clone, Copy, Default)]
pub struct GodView;

impl ShotSpec for GodView {
    fn id(&self) -> &'static str {
        "god-view"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Rare
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.6)
    }
    fn min_seconds(&self) -> f32 {
        9.0
    }
    fn max_seconds(&self) -> f32 {
        16.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        120.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let height = rng.between(120.0, 190.0);
        let phase = rng.between(0.0, TAU);
        let fov = rng.between(70.0, 85.0);
        Box::new(GodViewShot {
            rig: CarRig::new(2.0),
            height,
            phase,
            fov,
        })
    }
}

#[derive(Debug, Clone)]
struct GodViewShot {
    rig: CarRig,
    height: f32,
    phase: f32,
    fov: f32,
}

impl Shot for GodViewShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let swing = 0.35 * (frame.time as f32 * 0.05 + self.phase).sin();
        let want = Vec3::new(swing.sin() * 110.0, self.height, swing.cos() * 100.0);
        let look = Vec3::new(0.0, 0.0, -40.0);
        self.rig.track(frame, out, &RigGoal::new(want, look, self.fov));
        true
    }
}
