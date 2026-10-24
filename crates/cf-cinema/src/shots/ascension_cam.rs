//! L'Ascension vue par la caméra (`cinema/shots/ascension-cam.ts`).

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use super::sky_trip::{SkyTrip, TripGoal};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Plafond de la caméra au-dessus du sol (m) : au-delà, la ville se noie dans la brume et le réalisateur la ramène.
const CEILING: f64 = 160.0;
/// Phases de la cascade (voir `ASCENSION_PHASES`) : montée jusqu'à 0,4, suspension jusqu'à 0,5, replongée jusqu'à 0,9.
const CLIMB_END: f32 = 0.4;
const SKY_END: f32 = 0.5;
const DONE: f32 = 0.97;

/// On suit la voiture qui monte très haut, puis on la quitte pour contempler la ville d'en haut et la planète à anneau
/// (le ciel glisse vers elle), et l'on replonge derrière la voiture quand elle redescend. Tout est calé sur la
/// progression de la cascade.
#[derive(Debug, Clone, Copy, Default)]
pub struct AscensionCam;

impl ShotSpec for AscensionCam {
    fn id(&self) -> &'static str {
        "ascension-cam"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        8.0
    }
    fn max_seconds(&self) -> f32 {
        40.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        20.0
    }
    fn weight(&self) -> f32 {
        4.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.stunt.is_some_and(|s| s.kind == "ascension")
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if frame.stunt.is_some_and(|s| s.kind == "ascension" && s.progress < 0.2) {
            0.97
        } else {
            0.0
        }
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let side = rng.coin();
        let dx = rng.next_f32() - 0.5;
        let dy = rng.between(0.1, 0.5);
        let dz = rng.next_f32() - 0.5;
        Box::new(AscensionShot {
            trip: SkyTrip::default(),
            side,
            dir: Vec3::new(dx, dy, dz).try_normalize().unwrap_or(Vec3::Y),
            progress: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct AscensionShot {
    trip: SkyTrip,
    side: f32,
    /// Direction du glissement du ciel (unitaire).
    dir: Vec3,
    progress: f32,
}

impl Shot for AscensionShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some(stunt) = frame.stunt.filter(|s| s.kind == "ascension") else {
            return false;
        };
        self.progress = stunt.progress;
        let p = self.progress;
        let car = frame.car;
        let ceiling = (frame.ground() + CEILING).max(car.y + 35.0);
        // Suivi : derrière et un peu sous la voiture, qui s'éloigne vers le haut.
        let follow = DVec3::new(
            car.x * 0.5 + f64::from(self.side * 5.0),
            (car.y - 3.0).min(ceiling),
            car.z + 17.0,
        );
        // Perchoir : très haut au-dessus de la route, d'où l'on voit la ville et le ciel.
        let perch = DVec3::new(car.x * 0.3, ceiling, car.z + 24.0);
        // Replongée : même pose que la caméra de poursuite, pour que la suite se raccorde sans à-coup.
        let chase = DVec3::new(car.x * 0.55, 2.3 + f64::from(frame.altitude) * 1.04, car.z + 6.2);
        let to_perch = smootherstep((p - 0.15) / (CLIMB_END - 0.15));
        let dive = smootherstep((p - SKY_END) / (0.9 - SKY_END));
        let mut pos = follow.lerp(perch, f64::from(to_perch)).lerp(chase, f64::from(dive));
        pos.y = pos.y.min(ceiling);
        let sky = smoothstep((p - 0.36) / 0.08) * (1.0 - smoothstep((p - 0.5) / 0.14));
        let mut goal = TripGoal {
            pos,
            car_target: car,
            ..TripGoal::default()
        };
        if let Some(planet) = frame.ring_planet {
            goal.sky_target = planet.center;
            goal.up = planet.ring_normal;
            goal.drift = planet.center * (0.55 * sky) + self.dir * (30.0 * sky);
            goal.sky_look = 0.9 * sky;
        } else {
            goal.sky_target = Vec3::Y * 100.0;
            goal.drift = Vec3::ZERO;
            goal.sky_look = 0.4 * sky;
        }
        goal.roll = 0.06 * (p * std::f32::consts::TAU).sin() * self.side;
        goal.fov = (64.0 + 14.0 * to_perch - 4.0 * dive).clamp(40.0, 90.0);
        self.trip
            .step(frame, &goal, 2.5 + 5.0 * dive, 2.5 + 3.0 * dive, 3.0 + 4.0 * dive);
        self.trip.write(out);
        true
    }

    fn holding(&self) -> bool {
        self.progress < DONE
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.trip.kick = 1.0;
        }
    }
}
