//! Le plan fixe sur un impact (`cinema/shots/impact.ts`) : un astre dont la trajectoire finit dans un immeuble, filmé
//! de loin, jusqu'à l'explosion. Aucune lumière touchée (règle 7) : l'explosion appartient au monde.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, ahead_of, find_subject, safe_position};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Pas de la projection de trajectoire (m) et portée maximale (m).
const STEP: f64 = 6.0;
const REACH: f64 = 900.0;
const MIN_RADIUS: f32 = 2.5;
/// L'explosion reste à l'écran : on regarde la façade s'effondrer (s).
const BLAST_HOLD: f32 = 2.6;
/// Durée maximale du plan (s).
const MAX_SECONDS: f32 = 14.0;
/// Le plan s'arrête si l'astre disparaît sans explosion enregistrée, après ce délai (s).
const MISSING_GRACE: f32 = 0.6;

/// Projette la trajectoire de l'astre : temps avant l'écrasement sur un immeuble (s) et point d'impact, ou `None`.
fn time_to_impact(frame: &CinemaFrame<'_>, m: &Subject) -> Option<(f32, DVec3)> {
    let speed = f64::from(m.velocity.length());
    if speed < 20.0 {
        return None;
    }
    let velocity = m.velocity.as_dvec3();
    let mut d = STEP;
    while d < REACH {
        let march = m.position + velocity * (d / speed);
        if march.y < -45.0 {
            return None;
        }
        if frame.collides(march, f64::from(m.size) * 0.8) {
            return Some(((d / speed) as f32, march));
        }
        d += STEP;
    }
    None
}

/// Un gros astre dont la trajectoire finit dans un immeuble, assez loin devant pour le regarder tomber : l'astre, le
/// temps avant l'impact (s) et le point d'impact.
fn threat<'a>(frame: &CinemaFrame<'a>) -> Option<(&'a Subject, f32, DVec3)> {
    let mut best: Option<(&Subject, f32, DVec3)> = None;
    for subject in frame.subjects {
        if subject.kind != SubjectKind::Meteor || subject.size < MIN_RADIUS {
            continue;
        }
        let Some((time, hit)) = time_to_impact(frame, subject) else {
            continue;
        };
        let ahead = ahead_of(frame, hit);
        if !(1.2..=9.0).contains(&time) || !(80.0..=650.0).contains(&ahead) || hit.y < 0.0 {
            continue;
        }
        if best.is_none_or(|(_, best_time, _)| time < best_time) {
            best = Some((subject, time, hit));
        }
    }
    best
}

/// Plan large et fixe sur un immeuble qui va être percuté ; la voiture file devant, l'explosion suit.
#[derive(Debug, Clone, Copy, Default)]
pub struct MeteorImpactWatch;

impl ShotSpec for MeteorImpactWatch {
    fn id(&self) -> &'static str {
        "meteor-impact-watch"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        4.0
    }
    fn max_seconds(&self) -> f32 {
        14.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        70.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Cut
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        threat(frame).is_some()
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        match threat(frame) {
            Some((_, time, _)) if time < 6.0 => 0.85,
            _ => 0.0,
        }
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let target = threat(frame);
        let impact = target.map_or(frame.car, |(_, _, hit)| hit);
        let fov = 66.0 + rng.next_f32() * 10.0;
        // Caméra sur le côté opposé de la chaussée, devant la voiture, assez haut pour voir la façade entière.
        let sx = if impact.x > 0.0 { -1.0 } else { 1.0 };
        let mut spot = DVec3::new(
            f64::from(sx * (4.0 + rng.next_f32() * 4.0)),
            (impact.y * 0.5 + 8.0).clamp(10.0, 60.0),
            (impact.z + f64::from(55.0 + rng.next_f32() * 25.0)).min(frame.car.z - 40.0),
        );
        safe_position(frame, &mut spot, 3.0);
        Box::new(ImpactShot {
            id: target.map(|(m, _, _)| m.id),
            impact,
            spot,
            rig: CamRig::default(),
            fov,
            start: frame.time,
            blast_at: None,
            elapsed: 0.0,
            missing: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct ImpactShot {
    id: Option<u64>,
    impact: DVec3,
    spot: DVec3,
    rig: CamRig,
    fov: f32,
    start: f64,
    /// Horloge de la scène à l'explosion.
    blast_at: Option<f64>,
    elapsed: f32,
    missing: f32,
}

impl Shot for ImpactShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.elapsed += frame.dt;
        let meteor = self.id.and_then(|id| find_subject(frame, SubjectKind::Meteor, id));
        let blast = frame
            .last_blast
            .filter(|b| Some(b.meteor_id) == self.id && b.time >= self.start - 0.5);
        if self.blast_at.is_none() && blast.is_some() {
            self.blast_at = Some(frame.time);
        }
        let gaze = if let Some(at) = self.blast_at {
            if frame.time - at > f64::from(BLAST_HOLD) {
                return false;
            }
            blast.map_or(self.impact, |b| b.at)
        } else if let Some(meteor) = meteor {
            // On regarde un point entre l'astre et la façade : l'astre entre par le haut du cadre.
            meteor.position.lerp(self.impact, 0.55)
        } else {
            // L'astre a disparu sans explosion enregistrée : un court délai pour sa gerbe, puis fin.
            self.missing += frame.dt;
            if self.missing > MISSING_GRACE {
                return false;
            }
            self.impact
        };
        if self.elapsed > MAX_SECONDS {
            return false;
        }
        let rates = RigRates { pos: 4.0, look: 2.5 };
        self.rig.track(frame.dt, self.spot, gaze, self.fov, rates, 0.0, Vec3::Y);
        self.rig.write(out);
        true
    }

    fn holding(&self) -> bool {
        self.blast_at.is_none() && self.elapsed < MAX_SECONDS
    }
}
