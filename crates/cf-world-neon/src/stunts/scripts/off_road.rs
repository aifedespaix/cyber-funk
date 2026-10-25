//! Hors route (`scripts/off-road.ts`).

use std::f64::consts::PI;

use super::{pick_index, side_of};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, between, lerp, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

/// Plus près que ça de l'axe, on est encore sur la chaussée ; la façade des premiers immeubles commence vers 13,5.
const EDGE: f64 = 9.9;

/// La voiture quitte la route : elle file hors de la chaussée, au ras du trottoir, et suit le pied des immeubles en
/// zigzaguant au tempo avant de se rabattre. Elle se tient à une distance de la façade, jamais dedans.
#[derive(Debug, Clone, Copy)]
pub struct OffRoad;

impl StuntScript for OffRoad {
    fn kind(&self) -> StuntKind {
        StuntKind::OffRoad
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
        !world.bridge_ahead(env.car_z, 900.0, 30.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let beats = [16.0, 20.0, 24.0][pick_index(ctx.rng, 3)];
        let skim_alt = ctx.rng.range(0.9, 1.5);
        let swing = ctx.rng.range(0.3, 1.0);
        let x0 = env.car_x.clamp(-3.0, 3.0);
        let side = sign(ctx.rng);
        Box::new(Run {
            beats,
            skim_alt,
            swing,
            a0: env.altitude,
            x0,
            side,
            x: x0,
            wall: 13.6,
            lean: Rate::default(),
            duration: beats * env.period,
        })
    }
}

struct Run {
    beats: f64,
    skim_alt: f64,
    swing: f64,
    a0: f64,
    x0: f64,
    side: f64,
    x: f64,
    wall: f64,
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
        if let Some(face) = ctx
            .world
            .facade(side_of(self.side), e.car_z - 8.0, 10.0)
            .filter(|f| f.is_finite())
        {
            self.wall += (f64::from(face) - self.wall) * (1.0 - (-e.dt * 4.0).exp());
        }
        let into = between(u, 1.2, 4.2);
        let back = between(u, self.beats - 5.5, self.beats - 1.8);
        let edge = self.side * (self.wall - 1.8 + (u * PI * 0.5).sin() * self.swing).clamp(EDGE, 13.4);
        self.x += (lerp(lerp(self.x0, edge, into), self.x0, back) - self.x) * (1.0 - (-e.dt * 6.0).exp());
        out.x = self.x;
        let skim = lerp(self.a0, self.skim_alt, between(u, 0.0, 2.5));
        out.altitude = lerp(skim, self.a0, back) + (0.2 + f64::from(e.beat.pulse) * 0.3) * into * (1.0 - back);
        // Penchée vers la route quand elle longe les murs, couchée dans ses virages.
        out.roll = -self.side * 0.16 * into * (1.0 - back) + (self.lean.follow(self.x, e.dt) * 0.04).clamp(-0.4, 0.4);
        out.hover = true;
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
