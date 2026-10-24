//! La moto lumineuse (`cinema/shots/bikers.ts`) : filmée de côté au ras du sol, ses murs de lumière dans le cadre.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, ahead_of, find_subject, heading, nearest_subject, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

fn biker<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    nearest_subject(frame, &[SubjectKind::Bike], 0.0, 240.0, |s| {
        let ahead = ahead_of(frame, s.position);
        ahead > -60.0 && ahead < 220.0
    })
}

/// Moto lumineuse filmée de côté au ras du sol : ses murs de lumière restent dans le cadre.
#[derive(Debug, Clone, Copy, Default)]
pub struct Bikers;

impl ShotSpec for Bikers {
    fn id(&self) -> &'static str {
        "bikers"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.3, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        5.0
    }
    fn max_seconds(&self) -> f32 {
        11.0
    }
    fn cooldown(&self) -> f32 {
        50.0
    }
    fn weight(&self) -> f32 {
        1.2
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        biker(frame).is_some()
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        let Some(target) = biker(frame) else { return 0.0 };
        let distance = target.position.distance(frame.car) as f32;
        if distance < 55.0 {
            0.72 + (1.0 - distance / 55.0) * 0.2
        } else {
            0.0
        }
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let first = biker(frame);
        // Côté du centre de la route : la caméra reste sur la chaussée, loin des façades.
        let side = first.map_or(1.0, |b| {
            if b.position.x.abs() < 1.0 {
                rng.coin()
            } else {
                -(b.position.x.signum() as f32)
            }
        });
        let gap = 5.0 + rng.next_f32() * 2.5;
        let low = 0.9 + rng.next_f32() * 0.8;
        let fov = 50.0 + rng.next_f32() * 14.0;
        Box::new(BikersShot {
            id: first.map(|b| b.id),
            rig: CamRig::default(),
            side,
            gap,
            low,
            fov,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct BikersShot {
    id: Option<u64>,
    rig: CamRig,
    side: f32,
    gap: f32,
    low: f32,
    fov: f32,
    elapsed: f32,
}

impl Shot for BikersShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some(target) = self.id.and_then(|id| find_subject(frame, SubjectKind::Bike, id)) else {
            return false;
        };
        self.elapsed += frame.dt;
        let dir = heading(target).as_dvec3();
        // Légèrement en avant de la moto : on la voit venir, ses traînées de lumière derrière elle.
        let mut goal = target.position + dir * f64::from(2.5 - self.elapsed * 0.15);
        goal.x += f64::from(self.side * self.gap);
        goal.y = (target.position.y + f64::from(self.low)).max(0.6);
        safe_position(frame, &mut goal, 1.5);
        let gaze = target.position + dir * -3.0 + DVec3::Y * 0.8;
        let fov = self.fov + (self.elapsed * 0.4).clamp(0.0, 4.0);
        let rates = RigRates { pos: 5.0, look: 6.0 };
        self.rig.track(frame.dt, goal, gaze, fov, rates, 0.0, Vec3::Y);
        self.rig.write(out);
        self.elapsed < 30.0
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.rig.kick = 1.2;
        }
    }
}
