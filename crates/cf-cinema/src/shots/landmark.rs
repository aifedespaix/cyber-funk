//! La révélation d'une structure (`cinema/shots/landmark.ts`) : grue qui monte en tournant devant un bâtiment d'événement.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::pose::smootherstep;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, ahead_of, find_subject, nearest_subject, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

fn landmark<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    nearest_subject(frame, &[SubjectKind::Landmark], 90.0, 360.0, |s| {
        let ahead = ahead_of(frame, s.position);
        ahead > 90.0 && ahead < 330.0
    })
}

/// Révèle une structure d'événement devant : grue qui monte en tournant, la voiture passe devant elle.
#[derive(Debug, Clone, Copy, Default)]
pub struct LandmarkReveal;

impl ShotSpec for LandmarkReveal {
    fn id(&self) -> &'static str {
        "landmark-reveal"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.85)
    }
    fn min_seconds(&self) -> f32 {
        5.0
    }
    fn max_seconds(&self) -> f32 {
        10.0
    }
    fn cooldown(&self) -> f32 {
        60.0
    }
    fn weight(&self) -> f32 {
        1.1
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        landmark(frame).is_some()
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let first = landmark(frame);
        let sweep = rng.coin() * (0.7 + rng.next_f32() * 0.4);
        let fov = 56.0 + rng.next_f32() * 14.0;
        let duration = 6.5 + rng.next_f32() * 3.0;
        Box::new(LandmarkShot {
            id: first.map(|s| s.id),
            rig: CamRig::default(),
            anchor: first.map_or(frame.car, |s| s.position),
            size: first.map_or(60.0, |s| s.size),
            sweep,
            fov,
            duration,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct LandmarkShot {
    id: Option<u64>,
    rig: CamRig,
    anchor: DVec3,
    size: f32,
    sweep: f32,
    fov: f32,
    duration: f32,
    elapsed: f32,
}

impl Shot for LandmarkShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let alive = self.id.and_then(|id| find_subject(frame, SubjectKind::Landmark, id));
        if alive.is_none() {
            return false;
        }
        self.elapsed += frame.dt;
        // Fin quand la voiture a dépassé la structure, ou au bout du temps imparti.
        if self.elapsed > self.duration || ahead_of(frame, self.anchor) < -50.0 {
            return false;
        }
        let t = smootherstep(self.elapsed / self.duration);
        let size = self.size;
        let radius = f64::from((size * 1.1).clamp(55.0, 120.0));
        let angle = f64::from(self.sweep * (1.0 - 2.0 * t));
        // Grue : on part bas, côté chaussée, et l'on monte pour embrasser toute la hauteur.
        let mut goal = DVec3::new(
            self.anchor.x + angle.sin() * radius,
            self.anchor.y + 6.0 + f64::from(t * (size * 0.6).clamp(15.0, 70.0)),
            self.anchor.z + angle.cos() * radius,
        );
        safe_position(frame, &mut goal, 3.0);
        let gaze = DVec3::new(
            self.anchor.x,
            self.anchor.y + f64::from((size * 0.35).clamp(8.0, 45.0) + t * size * 0.1),
            self.anchor.z,
        );
        let rates = RigRates { pos: 2.6, look: 3.5 };
        self.rig.track(frame.dt, goal, gaze, self.fov, rates, 0.0, Vec3::Y);
        self.rig.write(out);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Bar {
            self.rig.kick = 1.5;
        }
    }
}
