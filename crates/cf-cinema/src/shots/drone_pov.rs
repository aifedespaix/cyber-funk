//! La vue du drone (événement « visite en drone ») : la caméra **est** le drone. Le monde publie les yeux du drone sous la
//! forme d'un sujet [`SubjectKind::Drone`] (position, regard, vitesse, champ de vision) ; ce plan les prend tels quels, y
//! ajoute le penchant d'un drone qui vire (roulis d'après la vitesse de rotation du regard) et ne s'arrête qu'avec
//! l'événement.
//!
//! La trajectoire (arrivée, tour de la voiture, entrée par une vitre, habitacle, sortie, vue arrière) est écrite par
//! l'événement, qui connaît la voiture ; ce plan n'a donc ni minuterie ni cadrage propre. Il est `holding` : tant qu'il est à
//! l'antenne, le réalisateur ne coupe pas (couper au milieu du passage par la vitre gâcherait tout).

use bevy::math::{Quat, Vec3};
use bevy::prelude::Transform;
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::nearest_subject;
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Roulis maximal d'un drone qui vire, rad.
const MAX_ROLL: f32 = 0.42;

fn drone<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    nearest_subject(frame, &[SubjectKind::Drone], 0.0, f64::INFINITY, |_| true)
}

/// Caméra embarquée sur le drone de l'événement « visite en drone ».
#[derive(Debug, Clone, Copy, Default)]
pub struct DronePov;

impl ShotSpec for DronePov {
    fn id(&self) -> &'static str {
        "drone-pov"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        30.0
    }
    fn max_seconds(&self) -> f32 {
        60.0
    }
    /// Un plan écrit par l'événement : le rythme du morceau ne l'étire pas.
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        30.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        drone(frame).is_some()
    }
    /// La visite est un moment à voir en entier : dès qu'elle existe, c'est l'occasion à saisir.
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if drone(frame).is_some() { 0.98 } else { 0.0 }
    }
    fn create(&self, frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(DronePovShot {
            id: drone(frame).map(|d| d.id),
            heading: None,
            roll: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct DronePovShot {
    id: Option<u64>,
    /// Cap du regard à l'image précédente (rad), pour la vitesse de virage.
    heading: Option<f32>,
    roll: f32,
}

/// Écart entre deux angles, ramené à `-π..π`.
fn wrap(angle: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    (angle + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI
}

impl Shot for DronePovShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some(id) = self.id else { return false };
        let Some(eyes) = frame
            .subjects
            .iter()
            .find(|s| s.kind == SubjectKind::Drone && s.id == id)
        else {
            return false;
        };
        let forward = eyes.facing.unwrap_or(Vec3::NEG_Z).normalize_or(Vec3::NEG_Z);
        // Le cap : à gauche ou à droite quand le regard tourne ; le drone se penche dans le virage et se redresse après.
        let heading = (-forward.x).atan2(-forward.z);
        let rate = match self.heading {
            Some(before) if frame.dt > 1e-4 => wrap(heading - before) / frame.dt,
            _ => 0.0,
        };
        self.heading = Some(heading);
        let target = (rate * 0.16).clamp(-MAX_ROLL, MAX_ROLL);
        self.roll += (target - self.roll) * (1.0 - (-frame.dt * 4.0).exp());

        out.position = eyes.position;
        out.rotation = Transform::IDENTITY.looking_to(forward, Vec3::Y).rotation * Quat::from_rotation_z(self.roll);
        out.fov_deg = eyes.size.clamp(crate::kit::FOV_MIN, crate::kit::FOV_MAX);
        out.cabin = false;
        out.sky_drift = Vec3::ZERO;
        true
    }

    /// Couper au milieu de la visite la gâcherait : seule la fin de l'événement ramène un autre plan.
    fn holding(&self) -> bool {
        true
    }
}
