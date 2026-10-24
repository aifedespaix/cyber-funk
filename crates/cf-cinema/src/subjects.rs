//! Outils des plans qui filment un sujet du monde (`cinema/shots/subject-helpers.ts`) : retrouver un sujet, le plus
//! proche d'un genre, se dégager d'un immeuble, caméra lissée. La liste des sujets vient de `cf-render`
//! ([`Subjects`](cf_render::Subjects), remplie par le monde à chaque image).

use bevy::math::{DVec3, Quat, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::kit::{FOV_MAX, FOV_MIN};
use crate::pose::{aim, follow};
use crate::world::CinemaFrame;

/// Retrouve un sujet par (genre, id) : `None` s'il a disparu.
#[must_use]
pub fn find_subject<'a>(frame: &CinemaFrame<'a>, kind: SubjectKind, id: u64) -> Option<&'a Subject> {
    frame.subjects.iter().find(|s| s.kind == kind && s.id == id)
}

/// Distance devant la voiture le long de la route (positive = devant, la route file vers −z).
#[must_use]
pub fn ahead_of(frame: &CinemaFrame<'_>, p: DVec3) -> f64 {
    frame.car.z - p.z
}

/// Le sujet le plus proche de la voiture parmi ces genres, dans une fenêtre de distance (m), filtré au besoin.
#[must_use]
pub fn nearest_subject<'a>(
    frame: &CinemaFrame<'a>,
    kinds: &[SubjectKind],
    min_distance: f64,
    max_distance: f64,
    accept: impl Fn(&Subject) -> bool,
) -> Option<&'a Subject> {
    let mut best = None;
    let mut best_distance = f64::INFINITY;
    for subject in frame.subjects {
        if !kinds.contains(&subject.kind) {
            continue;
        }
        let distance = subject.position.distance(frame.car);
        if distance < min_distance || distance > max_distance || distance >= best_distance || !accept(subject) {
            continue;
        }
        best = Some(subject);
        best_distance = distance;
    }
    best
}

/// Direction de marche d'un sujet à plat (unitaire) ; vers l'avant de la route s'il est quasi immobile.
#[must_use]
pub fn heading(subject: &Subject) -> Vec3 {
    if subject.velocity.length_squared() < 4.0 {
        Vec3::NEG_Z
    } else {
        subject.velocity.normalize()
    }
}

/// Sort un point d'un immeuble : on monte par paliers, puis en dernier recours on se rabat sur la chaussée.
pub fn safe_position(frame: &CinemaFrame<'_>, p: &mut DVec3, radius: f64) {
    if !frame.collides(*p, radius) {
        return;
    }
    let y = p.y;
    for i in 1..=6 {
        p.y = y + f64::from(i) * 9.0;
        if !frame.collides(*p, radius) {
            return;
        }
    }
    p.x = p.x.clamp(-8.0, 8.0);
    p.y = y.max(0.0) + 70.0;
}

/// Caméra lissée d'un plan : garde sa position, son orientation et son champ de vision d'une image à l'autre. Le
/// premier appel s'installe directement sur le but (le réalisateur gère l'arrivée par la transition).
#[derive(Debug, Clone)]
pub struct CamRig {
    /// Position (absolue, m).
    pub pos: DVec3,
    /// Orientation.
    pub quat: Quat,
    /// Champ de vision (degrés).
    pub fov: f32,
    /// Bouffée de champ de vision (degrés) ajoutée par les temps musicaux, qui retombe seule.
    pub kick: f32,
    /// Le rig a reçu sa première consigne.
    pub ready: bool,
    scratch: CameraPose,
}

impl Default for CamRig {
    fn default() -> Self {
        Self {
            pos: DVec3::ZERO,
            quat: Quat::IDENTITY,
            fov: 60.0,
            kick: 0.0,
            ready: false,
            scratch: CameraPose::default(),
        }
    }
}

/// Raideurs de suivi d'un [`CamRig`] (1/s).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigRates {
    /// Position.
    pub pos: f32,
    /// Regard.
    pub look: f32,
}

impl CamRig {
    /// Suit le but `goal` en regardant `look_at`.
    pub fn track(&mut self, dt: f32, goal: DVec3, look_at: DVec3, goal_fov: f32, rates: RigRates, roll: f32, up: Vec3) {
        if !self.ready {
            self.ready = true;
            self.pos = goal;
            aim(&mut self.scratch, self.pos, look_at, roll, up, false);
            self.quat = self.scratch.rotation;
            self.fov = goal_fov;
            return;
        }
        self.pos = self.pos.lerp(goal, f64::from(follow(dt, rates.pos)));
        aim(&mut self.scratch, self.pos, look_at, roll, up, false);
        self.quat = self.quat.slerp(self.scratch.rotation, follow(dt, rates.look));
        self.fov += (goal_fov - self.fov) * follow(dt, 3.0);
        self.kick *= (-dt * 5.0).exp();
    }

    /// Écrit la pose (ciel à l'infini, hors habitacle).
    pub fn write(&self, out: &mut CameraPose) {
        out.position = self.pos;
        out.rotation = self.quat;
        out.fov_deg = (self.fov + self.kick).clamp(FOV_MIN, FOV_MAX);
        out.cabin = false;
        out.sky_drift = Vec3::ZERO;
    }
}
