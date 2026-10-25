//! Grand saut (`scripts/big-jump.ts`).

use std::f64::consts::PI;

use super::chance;
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

const BEATS: f64 = 16.0;
/// Temps de l'atterrissage : pile sur un temps, deux avant la fin.
const LAND: f64 = 14.0;

/// Au drop, la voiture saute : un grand arc, une vrille lente de tout le vol, un atterrissage sur le temps. La plus
/// spectaculaire des cascades, donc réservée au drop.
#[derive(Debug, Clone, Copy)]
pub struct BigJump;

impl StuntScript for BigJump {
    fn kind(&self) -> StuntKind {
        StuntKind::BigJump
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Rare
    }
    fn weight(&self) -> f64 {
        2.0
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Drop
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 900.0, 45.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let x0 = env.car_x.clamp(-3.5, 3.5) + ctx.rng.range(-0.8, 0.8);
        let peak = ctx.rng.range(18.0, 36.0);
        let dir = sign(ctx.rng);
        // Variantes : vrille (tonneau), salto, ou les deux ; un ou deux tours ; envol plus ou moins tardif.
        let style = ctx.rng.unit();
        let turns = if chance(ctx.rng, 0.3) { 2.0 } else { 1.0 };
        let launch = ctx.rng.range(0.6, 1.2);
        Box::new(Run {
            a0: env.altitude,
            x0,
            peak,
            dir,
            style,
            turns,
            launch,
            climb: Rate::default(),
            duration: BEATS * env.period,
        })
    }
}

struct Run {
    a0: f64,
    x0: f64,
    peak: f64,
    dir: f64,
    style: f64,
    turns: f64,
    launch: f64,
    climb: Rate,
    duration: f64,
}

impl StuntRun for Run {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let e = ctx.env;
        let u = t / e.period;
        let flight = ((u - self.launch) / (LAND - self.launch)).clamp(0.0, 1.0);
        out.x = self.x0;
        out.altitude = self.a0 + self.peak * (PI * flight).sin().max(0.0).powf(0.85);
        out.roll = if self.style < 0.75 {
            self.dir * TAU * self.turns * between(u, 2.0, 11.5)
        } else {
            0.0
        };
        let salto = if self.style > 0.45 {
            self.dir * TAU * between(u, 3.0, 11.0)
        } else {
            0.0
        };
        out.pitch = (self.climb.follow(out.altitude, e.dt) * 0.024).clamp(-0.5, 0.5) + salto;
        out.hover = out.altitude > 0.9;
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
