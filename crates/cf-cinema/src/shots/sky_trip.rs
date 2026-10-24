//! La caméra d'un voyage dans le ciel (`cinema/shots/sky-trip.ts`) : planète, lune.
//!
//! Rappel : le ciel est dessiné en `position_caméra - sky_drift` ; l'œil est donc, vu du ciel, au point `sky_drift`, et
//! la direction vers un point `p` du ciel (repère local) est `p - sky_drift`.

use bevy::math::{DVec3, Quat, Vec3};
use cf_render::CameraPose;

use crate::kit::{FOV_MAX, FOV_MIN};
use crate::pose::{aim, follow};
use crate::world::CinemaFrame;

/// Ce que la caméra voudrait cette image pendant un voyage dans le ciel (`TripGoal`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TripGoal {
    /// Position réelle de la caméra dans la ville (absolue, m).
    pub pos: DVec3,
    /// Position virtuelle dans le ciel (m, repère du ciel).
    pub drift: Vec3,
    /// Point regardé, repère du ciel.
    pub sky_target: Vec3,
    /// Point regardé, repère du monde (la voiture, absolu) : sert quand `sky_look` est faible.
    pub car_target: DVec3,
    /// 0 : on regarde `car_target` ; 1 : on regarde `sky_target`.
    pub sky_look: f32,
    /// « Haut » de la caméra quand elle regarde le ciel (la normale de l'anneau, par exemple).
    pub up: Vec3,
    /// Roulis (rad).
    pub roll: f32,
    /// Champ de vision (degrés).
    pub fov: f32,
}

impl Default for TripGoal {
    fn default() -> Self {
        Self {
            pos: DVec3::ZERO,
            drift: Vec3::ZERO,
            sky_target: Vec3::ZERO,
            car_target: DVec3::ZERO,
            sky_look: 0.0,
            up: Vec3::Y,
            roll: 0.0,
            fov: 60.0,
        }
    }
}

/// Caméra lissée d'un voyage dans le ciel : position réelle, ciel décalé, regard et FOV avancent d'un seul tenant.
#[derive(Debug, Clone)]
pub struct SkyTrip {
    /// Position réelle (absolue, m).
    pub pos: DVec3,
    /// Décalage du ciel (m).
    pub drift: Vec3,
    /// Orientation.
    pub quat: Quat,
    /// Champ de vision (degrés).
    pub fov: f32,
    /// Bouffée de champ de vision (degrés) ajoutée par les temps musicaux, qui retombe seule.
    pub kick: f32,
    ready: bool,
    scratch: CameraPose,
}

impl Default for SkyTrip {
    fn default() -> Self {
        Self {
            pos: DVec3::ZERO,
            drift: Vec3::ZERO,
            quat: Quat::IDENTITY,
            fov: 60.0,
            kick: 0.0,
            ready: false,
            scratch: CameraPose::default(),
        }
    }
}

impl SkyTrip {
    /// Avance d'une image vers `g`, avec les raideurs `pos_rate`, `drift_rate`, `look_rate` (1/s).
    pub fn step(&mut self, frame: &CinemaFrame<'_>, g: &TripGoal, pos_rate: f32, drift_rate: f32, look_rate: f32) {
        let dt = frame.dt;
        if self.ready {
            self.pos = self.pos.lerp(g.pos, f64::from(follow(dt, pos_rate)));
            self.drift = self.drift.lerp(g.drift, follow(dt, drift_rate));
            self.fov += (g.fov - self.fov) * follow(dt, 2.5);
            self.kick *= (-dt * 4.0).exp();
        } else {
            self.pos = g.pos;
            self.drift = g.drift;
            self.fov = g.fov;
        }
        // Le regard mêle deux directions : vers la voiture (monde) et vers le point du ciel (en tenant compte du
        // décalage). Une direction nulle retombe sur l'avant de la route : jamais de NaN.
        let sky_dir = (g.sky_target - self.drift).try_normalize().unwrap_or(Vec3::NEG_Z);
        let car_dir = (g.car_target - self.pos)
            .as_vec3()
            .try_normalize()
            .unwrap_or(Vec3::NEG_Z);
        let mix = g.sky_look.clamp(0.0, 1.0);
        let dir = car_dir.lerp(sky_dir, mix).try_normalize().unwrap_or(car_dir);
        let up = Vec3::Y.lerp(g.up, mix).try_normalize().unwrap_or(Vec3::Y);
        aim(
            &mut self.scratch,
            self.pos,
            self.pos + dir.as_dvec3() * 100.0,
            g.roll,
            up,
            false,
        );
        self.quat = if self.ready {
            self.quat.slerp(self.scratch.rotation, follow(dt, look_rate))
        } else {
            self.scratch.rotation
        };
        self.ready = true;
    }

    /// Écrit la pose, avec le ciel décalé.
    pub fn write(&self, out: &mut CameraPose) {
        out.position = self.pos;
        out.rotation = self.quat;
        out.fov_deg = (self.fov + self.kick).clamp(FOV_MIN, FOV_MAX);
        out.cabin = false;
        out.sky_drift = self.drift;
    }
}
