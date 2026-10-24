//! La poursuite (`cinema/shots/pursuit.ts`) : la police (ou le fuyard) suivie par l'arrière-dessus.

use bevy::math::Vec3;
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, find_subject, heading, nearest_subject, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Le véhicule à suivre d'une poursuite : la police d'abord, sinon le fuyard.
fn chaser<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    let near = |s: &Subject| s.position.y > -5.0 && s.position.z - frame.car.z < 220.0;
    nearest_subject(frame, &[SubjectKind::Police], 0.0, 260.0, near)
        .or_else(|| nearest_subject(frame, &[SubjectKind::Quarry], 0.0, 260.0, near))
}

/// Poursuite suivie par l'arrière-dessus ; s'arrête quand elle dépasse largement la voiture ou s'éloigne.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pursuit;

impl ShotSpec for Pursuit {
    fn id(&self) -> &'static str {
        "pursuit"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.25, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        6.0
    }
    fn max_seconds(&self) -> f32 {
        16.0
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
        chaser(frame).is_some()
    }
    /// Plus la poursuite est près de la voiture, plus l'occasion est belle : elle passe juste à côté.
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        let Some(target) = chaser(frame) else { return 0.0 };
        let distance = target.position.distance(frame.car) as f32;
        if distance < 70.0 {
            0.75 + (1.0 - distance / 70.0) * 0.25
        } else {
            0.0
        }
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let first = chaser(frame);
        let side = rng.coin();
        let height = 4.0 + rng.next_f32() * 4.0;
        let fov = 62.0 + rng.next_f32() * 14.0;
        Box::new(PursuitShot {
            target: first.map(|s| (s.kind, s.id)),
            rig: CamRig::default(),
            side,
            height,
            fov,
        })
    }
}

#[derive(Debug, Clone)]
struct PursuitShot {
    target: Option<(SubjectKind, u64)>,
    rig: CamRig,
    side: f32,
    height: f32,
    fov: f32,
}

impl Shot for PursuitShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some((kind, id)) = self.target else { return false };
        let Some(target) = find_subject(frame, kind, id) else {
            return false;
        };
        // Le fuyard a pris trop d'avance, ou la poursuite est sortie du champ : on lâche.
        if target.position.distance(frame.car) > 280.0 || frame.car.z - target.position.z > 200.0 {
            return false;
        }
        let dir = heading(target).as_dvec3();
        let back = 9.0 + target.size * 1.8;
        let mut goal = target.position + dir * f64::from(-back);
        goal.x += f64::from(self.side * (3.0 + target.size * 0.4));
        goal.y += f64::from(self.height + target.size * 0.3);
        safe_position(frame, &mut goal, 2.5);
        let gaze = target.position + dir * 6.0;
        let fov = self.fov + (target.velocity.length() / 60.0).clamp(0.0, 1.0) * 6.0;
        let rates = RigRates { pos: 3.2, look: 5.0 };
        self.rig.track(frame.dt, goal, gaze, fov, rates, 0.0, Vec3::Y);
        self.rig.write(out);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        match cue.kind {
            CueKind::Beat => self.rig.kick = 1.5,
            // Le côté bascule sur une phrase : la caméra passe doucement de l'autre flanc.
            CueKind::Phrase => self.side = -self.side,
            _ => {}
        }
    }
}
