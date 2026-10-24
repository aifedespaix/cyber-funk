//! Outils communs aux plans centrés sur la voiture (`cinema/kit.ts`).

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::pose::{aim, follow, smoothstep};
use crate::rng::CinemaRng;
use crate::world::CinemaFrame;

/// Bornes du champ de vision que la caméra accepte (degrés).
pub const FOV_MIN: f32 = 20.0;
/// Voir [`FOV_MIN`].
pub const FOV_MAX: f32 = 110.0;
/// Hauteur minimale de la caméra au-dessus du sol sous la voiture (m).
pub const MIN_CLEARANCE: f64 = 0.45;

const CLEAR_STEPS: usize = 5;
/// Rayon (m) de la sphère dégagée autour d'une consigne de caméra (`keepClear` de Sillage).
pub const CLEAR_RADIUS: f64 = 1.5;

/// Vitesse ressentie, 0 (croisière) → 1 (pleine vitesse) : même courbe que le cadrage classique.
#[must_use]
pub fn rush(frame: &CinemaFrame<'_>) -> f32 {
    smoothstep((frame.speed / 40.0 - 0.8) / 0.9)
}

/// Sort un point d'un immeuble : on le rapproche de la chaussée et on le remonte jusqu'à ce qu'il soit libre.
///
/// Sillage écrasait `x` absolu vers 0, l'axe de la route ; c'est aussi l'axe ici (la route ne tourne pas en x).
pub fn keep_clear(frame: &CinemaFrame<'_>, p: &mut DVec3, radius: f64) {
    for _ in 0..CLEAR_STEPS {
        if !frame.collides(*p, radius) {
            return;
        }
        p.x *= 0.72;
        p.y += 2.5;
    }
}

/// Caméra accrochée à la voiture : le lissage se fait dans le repère de la voiture (comme `cameras.ts`), sinon la
/// caméra traînerait de vitesse / fréquence mètres derrière. Les consignes `want*` sont relatives à la voiture.
#[derive(Debug, Clone)]
pub struct CarRig {
    /// Position relative à la voiture, lissée (m).
    pub rel: Vec3,
    /// Point visé relatif à la voiture, lissé (m).
    pub rel_look: Vec3,
    /// Champ de vision lissé (degrés).
    pub fov: f32,
    /// Roulis lissé (rad).
    pub roll: f32,
    rate: f32,
    ready: bool,
}

/// Une consigne de [`CarRig::track`], relative à la voiture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RigGoal {
    /// Position voulue (m, relative à la voiture).
    pub rel: Vec3,
    /// Point visé (m, relatif à la voiture).
    pub look: Vec3,
    /// Champ de vision (degrés).
    pub fov: f32,
    /// Roulis (rad).
    pub roll: f32,
    /// Haut de la caméra.
    pub up: Vec3,
    /// Aucune inertie (coupe franche).
    pub snap: bool,
    /// Raideur de ce pas seulement (1/s) ; `None` : celle du rig (un plan qui veut un suivi plus lâche le précise).
    pub rate: Option<f32>,
}

impl RigGoal {
    /// Consigne ordinaire : sans roulis, haut = +y, avec inertie.
    #[must_use]
    pub fn new(rel: Vec3, look: Vec3, fov: f32) -> Self {
        Self {
            rel,
            look,
            fov,
            roll: 0.0,
            up: Vec3::Y,
            snap: false,
            rate: None,
        }
    }

    /// Même consigne, avec le haut `up` (vue en plongée, plan incliné).
    #[must_use]
    pub fn with_up(mut self, up: Vec3) -> Self {
        self.up = up;
        self
    }

    /// Même consigne, avec le roulis `roll` (rad).
    #[must_use]
    pub fn with_roll(mut self, roll: f32) -> Self {
        self.roll = roll;
        self
    }

    /// Même consigne, sans inertie (coupe franche).
    #[must_use]
    pub fn snapped(mut self, snap: bool) -> Self {
        self.snap = snap;
        self
    }

    /// Même consigne, avec une raideur propre à ce pas (1/s).
    #[must_use]
    pub fn with_rate(mut self, rate: f32) -> Self {
        self.rate = Some(rate);
        self
    }
}

/// Un générateur fils, graine tirée du parent : un plan garde son propre aléatoire pour les tirages qu'il fait en
/// cours de route (la place d'une caméra, un sens qui s'inverse), sans toucher à la suite du réalisateur.
#[must_use]
pub fn fork(rng: &mut CinemaRng) -> CinemaRng {
    CinemaRng::new((rng.next_f64() * f64::from(u32::MAX)) as u32)
}

impl CarRig {
    /// Rig de raideur `rate` (1/s).
    #[must_use]
    pub fn new(rate: f32) -> Self {
        Self {
            rel: Vec3::ZERO,
            rel_look: Vec3::ZERO,
            fov: 70.0,
            roll: 0.0,
            rate,
            ready: false,
        }
    }

    /// Suit la consigne. La consigne de position est dégagée des immeubles avant d'être suivie.
    pub fn track(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose, goal: &RigGoal) {
        let car = frame.car;
        let mut abs = car + goal.rel.as_dvec3();
        abs.y = abs.y.max(frame.ground() + MIN_CLEARANCE);
        keep_clear(frame, &mut abs, CLEAR_RADIUS);
        let want = (abs - car).as_vec3();
        let hard = goal.snap || !self.ready;
        let rate = goal.rate.unwrap_or(self.rate);
        let k = if hard { 1.0 } else { follow(frame.dt, rate) };
        let k_look = if hard { 1.0 } else { follow(frame.dt, rate * 1.4) };
        self.ready = true;
        self.rel = self.rel.lerp(want, k);
        self.rel_look = self.rel_look.lerp(goal.look, k_look);
        self.fov += (goal.fov - self.fov) * k;
        self.roll += (goal.roll - self.roll) * k;
        let eye = car + self.rel.as_dvec3();
        let look = car + self.rel_look.as_dvec3();
        aim(out, eye, look, self.roll, goal.up, false);
        out.fov_deg = self.fov.clamp(FOV_MIN, FOV_MAX);
    }
}
