//! Le plan accroché (`cinema/shots/ride-along.ts`) : la caméra se pose derrière-dessus d'un véhicule du trafic, puis
//! pivote vers la voiture qui se détache en contrebas.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::pose::smoothstep;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, ahead_of, find_subject, heading, nearest_subject, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Un véhicule volant devant la voiture, à portée de panorama.
fn host<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    nearest_subject(frame, &[SubjectKind::Craft, SubjectKind::Heavy], 25.0, 170.0, |s| {
        let ahead = ahead_of(frame, s.position);
        ahead > 20.0 && ahead < 150.0 && s.position.y > -2.0
    })
}

/// Accroché derrière-dessus d'un véhicule du trafic, puis panorama vers la voiture en contrebas.
#[derive(Debug, Clone, Copy, Default)]
pub struct RideAlong;

impl ShotSpec for RideAlong {
    fn id(&self) -> &'static str {
        "ride-along"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.8)
    }
    fn min_seconds(&self) -> f32 {
        5.0
    }
    fn max_seconds(&self) -> f32 {
        9.0
    }
    fn cooldown(&self) -> f32 {
        40.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        host(frame).is_some()
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        // Le véhicule est choisi à la création et suivi par (genre, id) ensuite ; sans véhicule, le plan se termine.
        let target = host(frame).map(|s| (s.kind, s.id));
        let duration = 5.5 + rng.next_f32() * 2.5;
        let side = rng.coin();
        let rise = 3.5 + rng.next_f32() * 3.0;
        let fov = 60.0 + rng.next_f32() * 12.0;
        Box::new(RideAlongShot {
            target,
            rig: CamRig::default(),
            duration,
            side,
            rise,
            fov,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct RideAlongShot {
    target: Option<(SubjectKind, u64)>,
    rig: CamRig,
    duration: f32,
    side: f32,
    rise: f32,
    fov: f32,
    elapsed: f32,
}

impl Shot for RideAlongShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some((kind, id)) = self.target else {
            return false;
        };
        let Some(target) = find_subject(frame, kind, id) else {
            return false;
        };
        if self.elapsed > self.duration {
            return false;
        }
        self.elapsed += frame.dt;
        let dir = heading(target);
        let back = 7.0 + target.size * 1.6;
        let mut goal = target.position + dir.as_dvec3() * f64::from(-back);
        goal.x += f64::from(self.side * (2.0 + target.size * 0.3));
        goal.y += f64::from(self.rise + target.size * 0.5);
        safe_position(frame, &mut goal, 2.5);
        // D'abord l'engin, puis le regard glisse vers la voiture qui se détache en bas.
        let pan = smoothstep((self.elapsed - self.duration * 0.3) / (self.duration * 0.4));
        let gaze: DVec3 = target.position.lerp(frame.car, f64::from(pan * 0.9));
        let rates = RigRates {
            pos: 3.5,
            look: 2.2 + pan * 2.0,
        };
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
