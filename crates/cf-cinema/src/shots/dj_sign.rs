//! L'enseigne du DJ (`cinema/shots/dj-sign.ts`) : de face, un peu en contre-plongée, travelling lent sur le côté.

use bevy::math::Vec3;
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::pose::smootherstep;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, find_subject, nearest_subject, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

fn sign_of<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    if frame.show.dj_sign {
        nearest_subject(frame, &[SubjectKind::DjSign], 0.0, f64::INFINITY, |_| true)
    } else {
        None
    }
}

/// L'enseigne géante du DJ, de face et un peu en contre-plongée, avec un travelling lent sur le côté et un léger recul
/// avant.
#[derive(Debug, Clone, Copy, Default)]
pub struct DjSign;

impl ShotSpec for DjSign {
    fn id(&self) -> &'static str {
        "dj-sign"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        6.0
    }
    fn max_seconds(&self) -> f32 {
        11.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        40.0
    }
    fn weight(&self) -> f32 {
        0.7
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        sign_of(frame).is_some()
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let duration = 7.0 + rng.next_f32() * 3.0;
        let slide = rng.coin() * (0.12 + rng.next_f32() * 0.1);
        Box::new(DjSignShot {
            id: sign_of(frame).map(|s| s.id),
            rig: CamRig::default(),
            duration,
            slide,
            t: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct DjSignShot {
    id: Option<u64>,
    rig: CamRig,
    duration: f32,
    slide: f32,
    t: f32,
}

impl Shot for DjSignShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some(sign) = self.id.and_then(|id| find_subject(frame, SubjectKind::DjSign, id)) else {
            return false;
        };
        if self.t > self.duration {
            return false;
        }
        self.t += frame.dt;
        let p = smootherstep(self.t / self.duration);
        let anchor = sign.position;
        let face = sign
            .facing
            .map_or(Vec3::Z, |f| Vec3::new(f.x, 0.0, f.z))
            .try_normalize()
            .unwrap_or(Vec3::Z);
        let size = sign.size;
        let reach = (size * 1.15).clamp(30.0, 120.0) * (1.08 - 0.12 * p);
        let drift = f64::from(self.slide * reach * (p - 0.5) * 2.0);
        let mut spot = anchor + face.as_dvec3() * f64::from(reach);
        spot.x += -f64::from(face.z) * drift;
        spot.z += f64::from(face.x) * drift;
        spot.y = (frame.ground() + 2.5).max(anchor.y - f64::from(size * 0.16));
        safe_position(frame, &mut spot, 2.0);
        // Le champ serre l'enseigne : sa largeur remplit à peu près le cadre.
        let fov = (2.0 * (size * 0.62 / reach).atan()).to_degrees().clamp(28.0, 70.0);
        let rates = RigRates { pos: 2.5, look: 4.0 };
        self.rig.track(frame.dt, spot, anchor, fov, rates, 0.0, Vec3::Y);
        self.rig.write(out);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.rig.kick = 0.8;
        }
    }
}
