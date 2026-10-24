//! Plans calés sur l'animation de la voiture (`cinema/shots/car-anim.ts`) : décollage, atterrissage, esquive.
//!
//! Leur course vient de l'animation elle-même (avancement du déploiement des roues, altitude, esquive), pas d'un
//! chronomètre. Le réalisateur y bascule dès qu'elle commence (voir [`ShotSpec::reactive`]) et y entre en panoramique
//! éclair.

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{FlightPhase, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Après le décollage, on garde la caméra ce temps (s) pour voir la voiture prendre son étage.
const TAKEOFF_TAIL: f32 = 1.8;
/// Après l'atterrissage, ce temps (s) pour voir la caisse se stabiliser.
const LANDING_TAIL: f32 = 1.7;
/// Après la fin de l'esquive, ce temps (s).
const DODGE_TAIL: f32 = 1.2;
/// Un plan calé sur une animation ne dure jamais plus que ce temps (s), même si l'animation ne finit pas.
const SAFETY: f32 = 9.0;

/// La voiture descend : roues qui se replient, ou en vol qui plonge près du sol.
fn descending(frame: &CinemaFrame<'_>) -> bool {
    let a = &frame.animation;
    a.phase == FlightPhase::Retracting || (a.phase == FlightPhase::Flying && a.climb < -2.0 && frame.altitude < 10.0)
}

/// Côté du centre de la route (ni façade ni arche dans l'objectif), au hasard si la voiture est au milieu.
fn road_side(frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> f32 {
    if frame.car.x > 1.0 {
        -1.0
    } else if frame.car.x < -1.0 {
        1.0
    } else {
        rng.coin()
    }
}

/// Décollage : bas contre la roue avant qui se déplie, puis la caméra recule et monte en arc pendant que la voiture
/// quitte le sol.
#[derive(Debug, Clone, Copy, Default)]
pub struct Takeoff;

impl ShotSpec for Takeoff {
    fn id(&self) -> &'static str {
        "takeoff"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        3.0
    }
    fn max_seconds(&self) -> f32 {
        5.5
    }
    fn scripted(&self) -> bool {
        true
    }
    fn reactive(&self) -> bool {
        true
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        25.0
    }
    fn weight(&self) -> f32 {
        2.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Whip
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.animation.phase == FlightPhase::Deploying && frame.animation.hover < 0.7 && frame.trick.is_none()
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if frame.animation.phase == FlightPhase::Deploying && frame.animation.hover < 0.5 {
            1.0
        } else {
            0.0
        }
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(TakeoffShot {
            rig: CarRig::new(7.0),
            side: rng.coin(),
            t: 0.0,
            airborne: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct TakeoffShot {
    rig: CarRig,
    side: f32,
    t: f32,
    airborne: f32,
}

impl Shot for TakeoffShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let a = frame.animation;
        if a.phase == FlightPhase::Retracting || a.phase == FlightPhase::Ground || self.t > SAFETY {
            return false;
        }
        if a.phase == FlightPhase::Flying {
            self.airborne += frame.dt;
        }
        // Course : le déploiement mène à mi-chemin, la montée vers l'étage finit le mouvement.
        let p = if a.phase == FlightPhase::Deploying {
            a.hover * 0.55
        } else {
            0.55 + 0.45 * smoothstep(self.airborne / TAKEOFF_TAIL)
        };
        let e = smootherstep(p);
        let start_eye = Vec3::new(self.side * 2.6, 0.5, -2.4);
        let end_eye = Vec3::new(self.side * 4.8, 2.4, 7.6);
        let mut eye = start_eye.lerp(end_eye, e);
        eye.y += (e * std::f32::consts::PI).sin() * 0.6;
        // En vol la caméra reste en retrait de la montée : la voiture s'élève au-dessus d'elle.
        if a.phase == FlightPhase::Flying {
            eye.y -= frame.altitude * 0.4;
        }
        let start_look = Vec3::new(self.side * 0.9, 0.45, -1.3);
        let end_look = Vec3::new(0.0, 0.9, -0.5);
        let look = start_look.lerp(end_look, e);
        let fov = 92.0 - 26.0 * e + frame.beat.pulse * 2.0;
        self.rig.track(frame, out, &RigGoal::new(eye, look, fov));
        self.airborne < TAKEOFF_TAIL
    }
}

/// Atterrissage : la caméra se pose sur la route et regarde la voiture descendre vers elle, les roues se replier.
#[derive(Debug, Clone, Copy, Default)]
pub struct Touchdown;

impl ShotSpec for Touchdown {
    fn id(&self) -> &'static str {
        "touchdown"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        3.0
    }
    fn max_seconds(&self) -> f32 {
        6.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn reactive(&self) -> bool {
        true
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        25.0
    }
    fn weight(&self) -> f32 {
        2.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Whip
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        descending(frame) && frame.trick.is_none()
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if descending(frame) { 1.0 } else { 0.0 }
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(TouchdownShot {
            rig: CarRig::new(6.0),
            side: road_side(frame, rng),
            t: 0.0,
            landed: 0.0,
            punch: 0.0,
            was_grounded: frame.animation.phase == FlightPhase::Ground,
        })
    }
}

#[derive(Debug, Clone)]
struct TouchdownShot {
    rig: CarRig,
    side: f32,
    t: f32,
    landed: f32,
    punch: f32,
    was_grounded: bool,
}

impl Shot for TouchdownShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let grounded = frame.animation.phase == FlightPhase::Ground;
        if grounded && !self.was_grounded {
            self.punch = 9.0;
        }
        self.was_grounded = grounded;
        if grounded {
            self.landed += frame.dt;
        }
        if self.landed > LANDING_TAIL || self.t > SAFETY {
            return false;
        }
        // La hauteur est relative à la voiture : on garde l'œil à ras du bitume pendant qu'elle descend.
        let near = 1.0 - smoothstep(frame.altitude / 8.0);
        let eye = Vec3::new(self.side * (4.2 - near * 0.8), 0.55 - frame.altitude, 3.2 - near * 1.2);
        let look = Vec3::new(0.0, 0.55, -0.6);
        self.punch *= (-frame.dt * 6.0).exp();
        let goal = RigGoal::new(eye, look, 54.0 + near * 24.0 + self.punch).with_roll(self.side * 0.02 * near);
        self.rig.track(frame, out, &goal);
        true
    }
}

/// Esquive : par dessus, la caméra regarde d'en bas la caisse enjamber la structure ; par dessous, d'en haut, elle
/// plonge.
#[derive(Debug, Clone, Copy, Default)]
pub struct DodgePass;

impl ShotSpec for DodgePass {
    fn id(&self) -> &'static str {
        "dodge-pass"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.1, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        3.0
    }
    fn max_seconds(&self) -> f32 {
        6.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn reactive(&self) -> bool {
        true
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        30.0
    }
    fn weight(&self) -> f32 {
        1.6
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Whip
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.animation.dodge != 0 && frame.trick.is_none()
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if frame.animation.dodge != 0 { 1.0 } else { 0.0 }
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(DodgePassShot {
            rig: CarRig::new(6.0),
            side: rng.coin(),
            over: frame.animation.dodge >= 0,
            t: 0.0,
            calm: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct DodgePassShot {
    rig: CarRig,
    side: f32,
    over: bool,
    t: f32,
    calm: f32,
}

impl Shot for DodgePassShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.t += frame.dt;
        let a = frame.animation;
        self.calm = if a.dodge == 0 { self.calm + frame.dt } else { 0.0 };
        if self.calm > DODGE_TAIL || self.t > SAFETY {
            return false;
        }
        // La caméra reste du côté de la voiture : la structure passe derrière elle.
        let eye = if self.over {
            Vec3::new(self.side * 3.4, 0.5 - frame.altitude * 0.6, 7.0)
        } else {
            Vec3::new(self.side * 3.4, 4.6, 7.4)
        };
        let look = Vec3::new(0.0, 0.9, -2.2);
        let lean = (a.roll * 1.2).clamp(-0.3, 0.3);
        let fov = (if self.over { 66.0 } else { 62.0 }) + frame.beat.pulse * 2.0;
        self.rig
            .track(frame, out, &RigGoal::new(eye, look, fov).with_roll(lean));
        true
    }
}
