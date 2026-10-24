//! Le coup d'œil sur un panneau (`cinema/shots/ad.ts`) : la caméra quitte la voiture pour lire une enseigne.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::pose::smootherstep;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, ahead_of, find_subject, nearest_subject, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

const MIN_AHEAD: f64 = 80.0;
const MAX_AHEAD: f64 = 250.0;

/// Un panneau devant, dont la face lisible regarde vers la route (là où la caméra arrivera).
fn readable_sign<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    nearest_subject(frame, &[SubjectKind::Sign], MIN_AHEAD, MAX_AHEAD * 1.3, |sign| {
        let ahead = ahead_of(frame, sign.position);
        let Some(facing) = sign.facing else { return false };
        if !(MIN_AHEAD..=MAX_AHEAD).contains(&ahead) {
            return false;
        }
        // La face doit être tournée du côté de la voiture : sinon on la verrait par derrière.
        let to_car = frame.car - sign.position;
        if f64::from(facing.x) * to_car.x + f64::from(facing.z) * to_car.z <= 0.0 {
            return false;
        }
        let reach = (sign.size * 1.5).clamp(14.0, 60.0);
        !frame.collides(sign.position + facing.as_dvec3() * f64::from(reach), 2.5)
    })
}

/// Quitte la voiture pour lire un panneau : arrivée de face (ou en biais), léger dolly-in, 3 à 5 s.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdGlance;

impl ShotSpec for AdGlance {
    fn id(&self) -> &'static str {
        "ad-glance"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.75)
    }
    fn min_seconds(&self) -> f32 {
        3.0
    }
    fn max_seconds(&self) -> f32 {
        5.5
    }
    fn cooldown(&self) -> f32 {
        45.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.altitude < 40.0 && readable_sign(frame).is_some()
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let sign = readable_sign(frame);
        let duration = 3.2 + rng.next_f32() * 1.8;
        let lateral = rng.coin() * (0.2 + rng.next_f32() * 0.25);
        let stand = 1.35 + rng.next_f32() * 0.6;
        let fov = 36.0 + rng.next_f32() * 8.0;
        // Position figée au premier tour : le panneau est fixe, seule la caméra bouge.
        let anchor = sign.map_or(frame.car, |s| s.position);
        let facing = sign
            .and_then(|s| s.facing)
            .map(|f| Vec3::new(f.x, 0.0, f.z))
            .and_then(Vec3::try_normalize)
            .unwrap_or(Vec3::Z);
        Box::new(AdGlanceShot {
            id: sign.map(|s| s.id),
            rig: CamRig::default(),
            anchor,
            facing,
            size: sign.map_or(20.0, |s| s.size),
            duration,
            lateral,
            stand,
            fov,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct AdGlanceShot {
    id: Option<u64>,
    rig: CamRig,
    anchor: DVec3,
    facing: Vec3,
    size: f32,
    duration: f32,
    lateral: f32,
    stand: f32,
    fov: f32,
    elapsed: f32,
}

impl Shot for AdGlanceShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let live = self.id.and_then(|id| find_subject(frame, SubjectKind::Sign, id));
        if live.is_none() || self.elapsed > self.duration {
            return false;
        }
        self.elapsed += frame.dt;
        let t = smootherstep(self.elapsed / self.duration);
        // Dolly-in lent : on part un tiers plus loin, légèrement en biais, et l'on se redresse en face.
        let reach = (self.size * self.stand).clamp(14.0, 60.0) * (1.3 - 0.3 * t);
        let side = self.lateral * reach * (1.0 - 0.6 * t);
        let mut spot = self.anchor + self.facing.as_dvec3() * f64::from(reach);
        spot.x += f64::from(-self.facing.z * side);
        spot.z += f64::from(self.facing.x * side);
        spot.y += f64::from(reach * 0.06);
        safe_position(frame, &mut spot, 2.5);
        let rates = RigRates { pos: 3.5, look: 5.0 };
        self.rig
            .track(frame.dt, spot, self.anchor, self.fov - 2.0 * t, rates, 0.0, Vec3::Y);
        self.rig.write(out);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.rig.kick = 1.2;
        }
    }
}
