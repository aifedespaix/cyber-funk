//! Looping (`scripts/loop.ts`).

use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, lerp, turn};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

use super::chance;

/// Un looping : la voiture se soulève, boucle un tour complet à la verticale et se repose, le tout en trois mesures.
#[derive(Debug, Clone, Copy)]
pub struct Loop;

impl StuntScript for Loop {
    fn kind(&self) -> StuntKind {
        StuntKind::Loop
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Common
    }
    fn weight(&self) -> f64 {
        2.2
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 450.0, 25.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        // Variantes : un ou deux tours, vers l'avant ou vers l'arrière, rayon tiré.
        let twice = chance(ctx.rng, 0.3);
        let beats = if twice { 16.0 } else { 12.0 };
        let radius = ctx.rng.range(4.5, 7.0);
        let way = if chance(ctx.rng, 0.7) { 1.0 } else { -1.0 };
        Box::new(Run {
            twice,
            beats,
            radius,
            way,
            a0: env.altitude,
            x0: env.car_x.clamp(-3.0, 3.0),
            climb: Rate::default(),
            duration: beats * env.period,
        })
    }
}

struct Run {
    twice: bool,
    beats: f64,
    radius: f64,
    way: f64,
    a0: f64,
    x0: f64,
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
        let second = if self.twice { turn(u, 7.4, 11.6) } else { 0.0 };
        let angle = self.way * TAU * (turn(u, 3.0, 7.4) + second);
        // Le tour dessine un cercle : on monte avec le sinus verse, puis on redescend vers l'altitude de départ.
        let lift = lerp(self.a0, self.a0 + 3.2, between(u, 0.0, 2.6)) + self.radius * (1.0 - angle.cos());
        out.x = self.x0;
        out.altitude = lerp(lift, self.a0, between(u, self.beats - 4.0, self.beats - 1.0));
        out.pitch = angle + (self.climb.follow(out.altitude, e.dt) * 0.01).clamp(-0.2, 0.2);
        out.hover = out.altitude > 0.8;
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
