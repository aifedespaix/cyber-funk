//! Le plan `chase` (`cinema/shots/car-follow.ts`) : la caméra de poursuite, derrière la voiture, en dix variantes.

use bevy::math::Vec3;
use cf_render::CameraPose;

use crate::kit::{CarRig, RigGoal, rush};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Une variante de cadrage : hauteur, recul et décalage latéral (m), champ (degrés).
#[derive(Debug, Clone, Copy, PartialEq)]
struct ChaseVariant {
    height: f32,
    back: f32,
    offset: f32,
    fov: f32,
}

const fn v(height: f32, back: f32, offset: f32, fov: f32) -> ChaseVariant {
    ChaseVariant {
        height,
        back,
        offset,
        fov,
    }
}

/// Basse, classique, haute, décalée à gauche / à droite.
const CHASE_VARIANTS: [ChaseVariant; 10] = [
    v(1.5, 5.2, 0.0, 72.0),
    v(2.4, 6.4, 0.0, 74.0),
    v(4.6, 9.0, 0.0, 68.0),
    v(2.0, 6.0, 2.8, 72.0),
    v(2.0, 6.0, -2.8, 72.0),
    v(0.9, 4.4, 0.0, 80.0),
    v(3.4, 7.5, 3.6, 66.0),
    v(3.4, 7.5, -3.6, 66.0),
    v(1.2, 8.5, 1.6, 62.0),
    v(5.8, 11.0, 0.0, 64.0),
];

/// Probabilité de changer de variante à chaque début de phrase.
const PHRASE_SWITCH: f32 = 0.45;

/// Fiche du plan de poursuite.
#[derive(Debug, Clone, Copy, Default)]
pub struct Chase;

impl ShotSpec for Chase {
    fn id(&self) -> &'static str {
        "chase"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        7.0
    }
    fn max_seconds(&self) -> f32 {
        16.0
    }
    fn follows_car(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        18.0
    }
    fn weight(&self) -> f32 {
        3.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let variant = *rng.pick(&CHASE_VARIANTS).unwrap_or(&CHASE_VARIANTS[1]);
        // Chaque exemplaire a son tempérament : raideur du suivi, amplitude du balancement, focale de fond.
        let rig = CarRig::new(rng.between(3.8, 6.5));
        Box::new(ChaseShot {
            variant,
            rig,
            sway: rng.between(0.0, 0.9),
            sway_rate: rng.between(0.15, 0.4),
            fov_bias: rng.between(-5.0, 5.0),
            closing: rng.between(0.6, 1.5),
            rng: CinemaRng::new(rng.next_f64().to_bits() as u32),
        })
    }
}

/// Un plan de poursuite en cours.
#[derive(Debug, Clone)]
struct ChaseShot {
    variant: ChaseVariant,
    rig: CarRig,
    sway: f32,
    sway_rate: f32,
    fov_bias: f32,
    closing: f32,
    rng: CinemaRng,
}

impl Shot for ChaseShot {
    fn update(&mut self, w: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let r = rush(w);
        let car_x = w.car.x as f32;
        let v = self.variant;
        // Comme la poursuite classique : plus on va vite, plus la caméra se rapproche, et le FOV respire avec le kick.
        let sway = (w.time as f32 * self.sway_rate).sin() * self.sway;
        let want = Vec3::new(
            v.offset + sway - car_x * 0.45,
            v.height - r * 0.3 + w.beat.pulse * 0.1,
            v.back - r * 1.8 * self.closing,
        );
        let look = Vec3::new(-car_x * 0.3, 1.0, -8.0);
        let fov = v.fov + self.fov_bias + r * 26.0 + w.intensity * 5.0 + w.beat.pulse * 3.0;
        self.rig.track(w, out, &RigGoal::new(want, look, fov));
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Phrase
            && self.rng.next_f32() < PHRASE_SWITCH
            && let Some(next) = self.rng.pick(&CHASE_VARIANTS)
        {
            self.variant = *next;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::math::DVec3;

    use super::*;
    use crate::world::{CinemaWorld, NoObstacles};

    #[test]
    fn la_poursuite_est_derriere_et_regarde_devant() {
        let world = CinemaWorld {
            car: DVec3::new(0.0, 0.0, -250_000.0),
            dt: 0.016,
            speed: 40.0,
            ..CinemaWorld::default()
        };
        let frame = CinemaFrame::new(&world, &NoObstacles, &[]);
        let mut rng = CinemaRng::new(3);
        let mut shot = Chase.create(&frame, &mut rng);
        let mut pose = CameraPose::default();
        for _ in 0..60 {
            assert!(shot.update(&frame, &mut pose));
        }
        let rel = pose.position - world.car;
        assert!(rel.z > 3.0 && rel.z < 12.0, "{rel}");
        assert!(rel.y > 0.4 && rel.y < 6.5, "{rel}");
        let forward = pose.rotation * Vec3::NEG_Z;
        assert!(forward.z < -0.8, "{forward}");
    }
}
