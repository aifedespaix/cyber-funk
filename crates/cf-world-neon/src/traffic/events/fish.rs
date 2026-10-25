//! Banc de poissons-cerfs-volants (`events/fish.ts`) : trois bancs de quinze lueurs qui nagent dans le ciel de la ville en
//! files sinueuses, la tête en avant et la queue qui s'amenuise. Chaque poisson suit le tracé de la tête avec un léger
//! retard, d'où l'ondulation. Ils vivent au-dessus de 60 m ; leur tranche d'altitude est quand même réservée tant qu'ils
//! sont près de la route.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use super::common::{envelope, pick_tone, range};
use crate::pilot::flight::Band;
use crate::traffic::event::{EventCtx, TrafficEvent};

const LIFE_S: f32 = 26.0;
const SCHOOLS: usize = 3;
const FISH: usize = 15;
/// Les bancs nagent haut (au-dessus du dernier étage, 38,5 m, plus la coque) : la voiture ne les croise jamais.
const LOW_M: f64 = 64.0;
const GAP_S: f32 = 0.2;

#[derive(Debug, Clone, Copy)]
struct School {
    /// Centre du banc, absolu (f64 : la voiture peut être loin de l'origine).
    centre: DVec3,
    /// Amplitudes et pulsations de la lissajous que suit la tête.
    amp: Vec3,
    freq: Vec3,
    phase: Vec3,
    tint: LinearRgba,
    drift: f32,
}

#[derive(Debug)]
pub(crate) struct FishEvent {
    age: f32,
    schools: [School; SCHOOLS],
}

fn school(ctx: &mut EventCtx<'_>) -> School {
    let car_z = ctx.frame.car.z;
    let centre = DVec3::new(
        f64::from(range(ctx.rng, -40.0, 40.0)),
        f64::from(range(ctx.rng, LOW_M as f32 + 14.0, LOW_M as f32 + 34.0)),
        car_z - f64::from(range(ctx.rng, 260.0, 420.0)),
    );
    let amp = Vec3::new(
        range(ctx.rng, 30.0, 55.0),
        range(ctx.rng, 7.0, 14.0),
        range(ctx.rng, 35.0, 70.0),
    );
    let freq = Vec3::new(
        range(ctx.rng, 0.35, 0.6),
        range(ctx.rng, 0.5, 0.9),
        range(ctx.rng, 0.25, 0.5),
    );
    let tau = std::f32::consts::TAU;
    let phase = Vec3::new(
        range(ctx.rng, 0.0, tau),
        range(ctx.rng, 0.0, tau),
        range(ctx.rng, 0.0, tau),
    );
    School {
        centre,
        amp,
        freq,
        phase,
        tint: pick_tone(ctx),
        drift: range(ctx.rng, 0.4, 0.62),
    }
}

impl FishEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let schools = [school(ctx), school(ctx), school(ctx)];
        Self { age: 0.0, schools }
    }
}

impl TrafficEvent for FishEvent {
    fn claim(&self, ctx: &EventCtx<'_>) -> Option<Band> {
        let car_z = ctx.frame.car.z;
        let near = self
            .schools
            .iter()
            .any(|s| s.centre.z < car_z + 90.0 && s.centre.z > car_z - 300.0);
        near.then_some(Band {
            lo: LOW_M - 8.0,
            hi: LOW_M + 52.0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > LIFE_S {
            return false;
        }
        let envelope = envelope(self.age, LIFE_S, 2.5);
        let t = self.age;
        let time = ctx.frame.time as f32;
        let speed = ctx.frame.speed;
        for school in &mut self.schools {
            // Le banc avance un peu moins vite que la voiture : elle le rattrape et passe dessous.
            school.centre.z -= f64::from(speed * school.drift * dt);
            for i in 0..FISH {
                let u = t - i as f32 * GAP_S;
                let offset = Vec3::new(
                    (u * school.freq.x + school.phase.x).sin() * school.amp.x,
                    (u * school.freq.y + school.phase.y).sin() * school.amp.y,
                    (u * school.freq.z + school.phase.z).sin() * school.amp.z,
                );
                let taper = 1.0 - (i as f32 / FISH as f32) * 0.6;
                let flick = 0.75 + 0.25 * (time * 9.0 + i as f32 * 1.7).sin();
                let (size, base) = if i == 0 { (15.0, 3.4) } else { (9.0, 2.2) };
                ctx.glints.add(
                    school.centre + offset.as_dvec3(),
                    size * taper,
                    school.tint,
                    base * taper * flick * envelope,
                );
            }
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}
