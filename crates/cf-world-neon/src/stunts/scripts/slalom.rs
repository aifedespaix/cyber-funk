//! Slalom (`scripts/slalom.ts`).

use std::f64::consts::PI;

use super::{pick_index, release, spawn_at};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, between, lerp, move_craft, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;
use crate::traffic::CraftId;
use crate::traffic::variant::SLEEK;

/// Temps du premier portillon.
const FIRST: f64 = 3.0;

/// Une rangée de petits vaisseaux, un tous les deux temps, de part et d'autre de l'axe : la voiture serpente entre eux,
/// au plus large de chaque côté pile sur le temps où elle passe devant l'un d'eux, couchée dans ses virages.
#[derive(Debug, Clone, Copy)]
pub struct Slalom;

impl StuntScript for Slalom {
    fn kind(&self) -> StuntKind {
        StuntKind::Slalom
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Common
    }
    fn weight(&self) -> f64 {
        3.0
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 420.0, 25.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        // Variantes : 5 à 8 portillons, espacés de 2 ou 3 temps, amplitude et taille tirées.
        let count = 5 + pick_index(ctx.rng, 4);
        let every = if ctx.rng.unit() < 0.3 { 3.0 } else { 2.0 };
        let amplitude = ctx.rng.range(2.8, 4.2);
        let scale = ctx.rng.range(2.0, 2.8);
        let beats = FIRST + (count as f64 - 1.0) * every + 7.0;
        let a0 = env.altitude;
        let flight_level = a0.max(1.3);
        let first = sign(ctx.rng);
        let relative = env.speed * 0.88;
        let mut gates = Vec::with_capacity(count);
        for i in 0..count {
            let side = first * if i % 2 == 0 { 1.0 } else { -1.0 };
            let at = FIRST + i as f64 * every;
            let craft = spawn_at(
                ctx.world,
                SLEEK[i % SLEEK.len()],
                (-side * 1.7, flight_level - 0.2, env.car_z - relative * env.period * at),
                scale,
                (i * 2) as i64,
            );
            gates.push(Gate {
                craft,
                side,
                at,
                drift: ctx.rng.range(0.0, 6.0),
            });
        }
        Box::new(Run {
            gates,
            count,
            every,
            amplitude,
            beats,
            a0,
            x0: env.car_x.clamp(-2.5, 2.5),
            flight_level,
            first,
            relative,
            lean: Rate::default(),
            duration: beats * env.period,
        })
    }
}

struct Gate {
    craft: Option<CraftId>,
    side: f64,
    at: f64,
    drift: f64,
}

struct Run {
    gates: Vec<Gate>,
    count: usize,
    every: f64,
    amplitude: f64,
    beats: f64,
    a0: f64,
    x0: f64,
    flight_level: f64,
    first: f64,
    relative: f64,
    lean: Rate,
    duration: f64,
}

impl StuntRun for Run {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let e = ctx.env;
        let u = t / e.period;
        for gate in &self.gates {
            let swing = (u * 0.9 + gate.drift).sin() * 0.5;
            move_craft(
                ctx.world,
                gate.craft,
                -gate.side * 1.7 + swing,
                self.flight_level - 0.2 + (u * 1.3 + gate.drift).sin() * 0.25,
                e.car_z - self.relative * e.period * (gate.at - u),
                e.dt,
            );
        }
        // Amplitude : s'installe avant le premier portillon, retombe après le dernier ; le cosinus tombe au plus large
        // sur chaque passage.
        let last = FIRST + (self.count as f64 - 1.0) * self.every;
        let swell = between(u, 0.4, FIRST) * (1.0 - between(u, last + 0.4, last + 3.2));
        let wave = (((u - FIRST) / self.every) * PI).cos();
        out.x = lerp(self.x0, self.first * self.amplitude * wave, swell);
        out.roll = (self.lean.follow(out.x, e.dt) * 0.045).clamp(-0.7, 0.7);
        // Le rebond sur les temps (`sin(u π) × 0,3`) peut passer sous la route quand la voiture en part ou y revient : Sillage
        // le laissait au pilote, ici la pose reste au-dessus du sol (voir « Écarts » de la PR).
        out.altitude = (lerp(
            lerp(self.a0, self.flight_level, between(u, 0.0, 2.0)),
            self.a0,
            between(u, self.beats - 3.0, self.beats - 1.0),
        ) + (u * PI).sin() * 0.3)
            .max(0.0);
        out.hover = out.altitude > 0.8;
        true
    }

    fn end(&mut self, world: &mut dyn StuntWorld) {
        for gate in &mut self.gates {
            release(world, gate.craft.take());
        }
    }
}
