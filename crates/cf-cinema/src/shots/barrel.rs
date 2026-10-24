//! Le plan de la figure en vol (`cinema/shots/barrel.ts`) : tonneau ou salto.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind, TrickKind};
use crate::world::CinemaFrame;

/// Durée d'un tonneau (s), repris de `TRICK_SECONDS` de Sillage.
const ROLL_SECONDS: f32 = 0.95;
/// Durée d'un salto (s).
const FLIP_SECONDS: f32 = 1.2;
/// Après la figure, on garde la caméra encore ce temps pour voir la voiture se rétablir.
const TAIL: f32 = 1.0;

/// Tonneau : caméra latérale qui tourne avec le roulis de la caisse. Salto : profil fixe, la caméra suit la voiture.
#[derive(Debug, Clone, Copy, Default)]
pub struct Barrel;

impl ShotSpec for Barrel {
    fn id(&self) -> &'static str {
        "barrel"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        1.9
    }
    fn max_seconds(&self) -> f32 {
        2.4
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        14.0
    }
    fn weight(&self) -> f32 {
        2.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Cut
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.trick.is_some_and(|t| t.progress < 0.6)
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if frame.trick.is_some_and(|t| t.progress < 0.3) {
            1.0
        } else {
            0.0
        }
    }
    fn create(&self, frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let kind = frame.trick.map_or(TrickKind::Roll, |t| t.kind);
        let rolling = kind == TrickKind::Roll;
        let left = if rolling {
            1.0 - frame.trick.map_or(0.0, |t| t.progress)
        } else {
            1.0
        };
        let seconds = if rolling { ROLL_SECONDS } else { FLIP_SECONDS };
        let gap = 6.5 + rng.next_f32() * 1.5;
        let fov = if rolling {
            58.0 + rng.next_f32() * 8.0
        } else {
            44.0 + rng.next_f32() * 6.0
        };
        // Côté du centre de la route (le plus souvent) : ni façade, ni arche dans l'objectif.
        let side = if frame.car.x > 1.0 {
            -1.0
        } else if frame.car.x < -1.0 {
            1.0
        } else {
            rng.coin()
        };
        Box::new(BarrelShot {
            rig: CamRig::default(),
            rolling,
            duration: seconds * left + TAIL,
            gap,
            fov,
            side,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct BarrelShot {
    rig: CamRig,
    rolling: bool,
    duration: f32,
    gap: f32,
    fov: f32,
    side: f32,
    elapsed: f32,
}

impl Shot for BarrelShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.elapsed += frame.dt;
        if self.elapsed > self.duration {
            return false;
        }
        if self.rolling {
            // Le décalage est exprimé dans le repère de la caisse : la caméra tourne autour de l'axe du tonneau avec
            // elle.
            let local = frame.car_rotation * Vec3::new(self.side * self.gap, 0.9, 1.2);
            let cam_up = frame.car_rotation * Vec3::Y;
            let mut goal = frame.car + local.as_dvec3();
            safe_position(frame, &mut goal, 1.5);
            // Pas de lissage de position ici : le lissage trahirait la rotation. Orientation tenue par la caisse.
            self.rig.pos = goal;
            aim(out, goal, frame.car, 0.0, cam_up, false);
            self.rig.quat = out.rotation;
            self.rig.fov = self.fov;
            self.rig.ready = true;
        } else {
            // Salto : profil de côté, légèrement devant pour voir la voiture passer de la tête aux pieds.
            let mut goal = DVec3::new(
                frame.car.x + f64::from(self.side * (self.gap + 2.0)),
                frame.car.y + 1.4,
                frame.car.z - 1.5,
            );
            safe_position(frame, &mut goal, 1.5);
            let rates = RigRates { pos: 9.0, look: 12.0 };
            self.rig
                .track(frame.dt, goal, frame.car, self.fov, rates, self.side * 0.05, Vec3::Y);
        }
        self.rig.write(out);
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.rig.kick = 1.0;
        }
    }
}
