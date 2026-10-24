//! La chevauchée d'un astre (`cinema/shots/meteor.ts`) : la caméra monte, se cale derrière-dessus un astre qui tombe et
//! le suit jusqu'à l'explosion.
//!
//! Aucune lumière : le plan ne touche qu'à la pose (règle 7). L'explosion est celle du monde, filmée de loin.
//!
//! `launchMeteor` : Sillage lançait l'astre à la création du plan et en recevait l'identifiant sur le champ. Ici le plan
//! le demande par [`Shot::take_meteor_launch`] (le réalisateur en fait un message [`crate::world::LaunchMeteor`]) et
//! adopte l'astre dès qu'il paraît dans les sujets, dans le délai de grâce [`SPAWN_GRACE`].

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, find_subject, safe_position};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Hauteur du brouillard au sol : sous ce niveau l'astre se perd dans la brume.
const MIST_Y: f64 = -50.0;
/// Le temps laissé à un astre lancé pour apparaître dans la liste des sujets (s).
const SPAWN_GRACE: f32 = 1.2;
/// Combien de temps on garde l'explosion à l'écran avant de rendre la main (s).
const BLAST_HOLD: f32 = 1.5;
/// Durée maximale du plan (s).
const MAX_SECONDS: f32 = 26.0;

/// Temps de vol restant avant la brume (s) : l'astre qui a le plus de chemin à faire fait le meilleur plan.
fn flight_left(m: &Subject) -> f32 {
    if m.velocity.y < -5.0 {
        ((m.position.y - MIST_Y) / -f64::from(m.velocity.y)) as f32
    } else {
        0.0
    }
}

fn best_meteor<'a>(frame: &CinemaFrame<'a>) -> Option<&'a Subject> {
    let mut best = None;
    let mut best_left = 2.0;
    for subject in frame.subjects {
        if subject.kind != SubjectKind::Meteor {
            continue;
        }
        let left = flight_left(subject);
        if left > best_left && subject.position.distance(frame.car) < 700.0 {
            best = Some(subject);
            best_left = left;
        }
    }
    best
}

/// Monte vite, se cale derrière-dessus un astre qui tombe et le suit jusqu'à l'explosion (ou la brume).
#[derive(Debug, Clone, Copy, Default)]
pub struct MeteorRide;

impl ShotSpec for MeteorRide {
    fn id(&self) -> &'static str {
        "meteor-ride"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Rare
    }
    fn energy(&self) -> (f32, f32) {
        (0.15, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        6.0
    }
    fn max_seconds(&self) -> f32 {
        22.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        150.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    /// Sans astre en vol, on en lance un à la création ; seule la voiture très haute gâche la montée.
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.altitude < 120.0
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let found = best_meteor(frame);
        let fov = 62.0 + rng.next_f32() * 10.0;
        let side = rng.coin();
        Box::new(MeteorRideShot {
            id: found.map(|m| m.id),
            launch: found.is_none(),
            rig: CamRig::default(),
            start: frame.time,
            fov,
            side,
            offset: DVec3::ZERO,
            frozen: DVec3::ZERO,
            seen: false,
            missing: 0.0,
            blast_at: None,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct MeteorRideShot {
    /// L'astre suivi ; `None` tant que l'astre lancé n'a pas paru.
    id: Option<u64>,
    /// L'ordre de lancement n'a pas encore été lu par le réalisateur.
    launch: bool,
    rig: CamRig,
    start: f64,
    fov: f32,
    side: f32,
    /// Décalage de la caméra dans le repère de l'astre : on glisse de la voiture vers la place de suivi.
    offset: DVec3,
    /// Dernière position de la caméra, où elle se fige à l'explosion.
    frozen: DVec3,
    seen: bool,
    missing: f32,
    /// Horloge de la scène à l'explosion.
    blast_at: Option<f64>,
    elapsed: f32,
}

impl MeteorRideShot {
    /// En attendant l'astre : la caméra s'élève au-dessus de la voiture. Faux : on lâche (lancement raté, brume).
    fn wait_for_meteor(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.missing += frame.dt;
        // Pas d'astre (lancement raté) ou englouti par la brume sans explosion : fin.
        if self.missing > if self.seen { 0.3 } else { SPAWN_GRACE } {
            return false;
        }
        let car = frame.car;
        let goal = DVec3::new(car.x * 0.5, car.y + 28.0, car.z + 24.0);
        let gaze = DVec3::new(car.x, car.y + 40.0, car.z);
        let rates = RigRates { pos: 1.5, look: 2.0 };
        self.rig.track(frame.dt, goal, gaze, 70.0, rates, 0.0, Vec3::Y);
        self.frozen = self.rig.pos;
        self.rig.write(out);
        true
    }

    /// Suit l'astre : place de suivi en arrière sur la trajectoire, un peu de côté pour voir la boule de feu.
    fn ride(&mut self, frame: &CinemaFrame<'_>, meteor: &Subject, out: &mut CameraPose) {
        if !self.seen {
            self.seen = true;
            // Point de départ : la position actuelle de la caméra, exprimée par rapport à l'astre.
            let from = if self.rig.ready { self.rig.pos } else { frame.car };
            self.offset = from - meteor.position;
        }
        let dir = if meteor.velocity.length_squared() < 1.0 {
            Vec3::NEG_Y
        } else {
            meteor.velocity.normalize()
        };
        let r = meteor.size;
        let back = 12.0 + r * 3.5;
        let mut rel = dir.as_dvec3() * f64::from(-back);
        rel.x += f64::from(self.side * (3.0 + r * 1.2));
        rel.y += f64::from(2.0 + r * 0.6);
        // L'écart se résorbe en glissant : monter vite au début, tenir ensuite sans retard sur un astre à 100 m/s.
        let rate = (1.4 + self.elapsed * 0.9).clamp(1.4, 5.0);
        self.offset = self.offset.lerp(rel, 1.0 - f64::from((-frame.dt * rate).exp()));
        let mut goal = meteor.position + self.offset;
        safe_position(frame, &mut goal, 2.0);
        let gaze = meteor.position + dir.as_dvec3() * f64::from(r * 2.0);
        let fov = self.fov + (meteor.velocity.length() / 130.0).clamp(0.0, 1.0) * 8.0;
        let rates = RigRates { pos: 40.0, look: 7.0 };
        self.rig.track(frame.dt, goal, gaze, fov, rates, 0.0, Vec3::Y);
        self.frozen = self.rig.pos;
        self.rig.write(out);
    }
}

impl Shot for MeteorRideShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.elapsed += frame.dt;
        if self.elapsed > MAX_SECONDS {
            return false;
        }
        // L'astre lancé par ce plan : le premier à paraître dans la fenêtre de grâce.
        if self.id.is_none() && !self.launch {
            self.id = best_meteor(frame).map(|m| m.id);
        }
        let meteor = self.id.and_then(|id| find_subject(frame, SubjectKind::Meteor, id));
        let blast = frame
            .last_blast
            .filter(|b| Some(b.meteor_id) == self.id && b.time >= self.start - 0.5);
        if self.blast_at.is_none() && blast.is_some() {
            self.blast_at = Some(frame.time);
        }
        if let Some(at) = self.blast_at {
            // Explosion : la caméra se fige à sa place et regarde le point d'impact quelques instants.
            if frame.time - at > f64::from(BLAST_HOLD) {
                return false;
            }
            let gaze = blast.map_or(self.frozen, |b| b.at);
            let rates = RigRates { pos: 2.0, look: 4.0 };
            self.rig
                .track(frame.dt, self.frozen, gaze, self.fov + 8.0, rates, 0.0, Vec3::Y);
            self.rig.write(out);
            return true;
        }
        match meteor {
            Some(meteor) => {
                self.ride(frame, meteor, out);
                true
            }
            None => self.wait_for_meteor(frame, out),
        }
    }

    fn holding(&self) -> bool {
        self.blast_at.is_none() && self.elapsed < MAX_SECONDS
    }

    fn take_meteor_launch(&mut self) -> bool {
        // Une fois l'ordre lu, le plan cherche l'astre dans les sujets (voir `update`).
        std::mem::replace(&mut self.launch, false)
    }
}
