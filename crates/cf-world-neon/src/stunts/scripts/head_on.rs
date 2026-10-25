//! Face à face (`scripts/head-on.ts`).

use super::{chance, focus_on, pick_index, release, spawn_at};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, bump, move_craft, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;
use crate::traffic::CraftId;
use crate::traffic::variant::CARGO;

/// Temps (de la ligne de temps) où le cargo croise la voiture, durée totale, distance de départ (m).
const MEET: f64 = 10.0;
const BEATS: f64 = 16.0;
const REACH: f64 = 520.0;

/// Un cargo géant fonce droit sur la voiture, au ras de la route. Au dernier moment elle passe par dessus en tonneau,
/// en croisant sur le temps fort. Le cargo arrive à vitesse relative réglée sur le tempo : il passe pile sur le temps.
#[derive(Debug, Clone, Copy)]
pub struct HeadOn;

impl StuntScript for HeadOn {
    fn kind(&self) -> StuntKind {
        StuntKind::HeadOn
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Uncommon
    }
    fn weight(&self) -> f64 {
        1.6
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, REACH + 120.0, 30.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let x0 = env.car_x.clamp(-3.2, 3.2);
        let a0 = env.altitude;
        let dir = sign(ctx.rng);
        let scale = ctx.rng.range(7.0, 11.0);
        let ship_y = ctx.rng.range(2.0, 3.6);
        let peak = a0.max(ship_y + scale * 0.85 + 2.6);
        let turns = if chance(ctx.rng, 0.25) { 2.0 } else { 1.0 };
        let variant = CARGO[pick_index(ctx.rng, 2)];
        let tint = ctx.rng.range(0.0, 8.0).floor() as i64;
        let ship = spawn_at(ctx.world, variant, (x0, ship_y, env.car_z - REACH), scale, tint);
        // Parfois un second cargo suit de biais, plus loin : la voiture passe entre les deux.
        let side = sign(ctx.rng);
        let second = if chance(ctx.rng, 0.4) {
            let tint = ctx.rng.range(0.0, 8.0).floor() as i64;
            spawn_at(
                ctx.world,
                CARGO[1],
                (x0 + side * (scale * 0.9 + 3.0), ship_y + 1.0, env.car_z - REACH * 1.12),
                scale * 0.8,
                tint,
            )
        } else {
            None
        };
        Box::new(Run {
            ship,
            second,
            x0,
            a0,
            dir,
            scale,
            ship_y,
            peak,
            turns,
            side,
            closing: REACH / (MEET * env.period),
            climb: Rate::default(),
            duration: BEATS * env.period,
        })
    }
}

struct Run {
    ship: Option<CraftId>,
    second: Option<CraftId>,
    x0: f64,
    a0: f64,
    dir: f64,
    scale: f64,
    ship_y: f64,
    peak: f64,
    turns: f64,
    side: f64,
    closing: f64,
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
        // Distance du cargo devant la voiture : elle tombe à zéro exactement sur le temps MEET.
        let gap = self.closing * (MEET - u) * e.period;
        move_craft(
            ctx.world,
            self.ship,
            self.x0 + (u * 0.8).sin() * 0.5,
            self.ship_y,
            e.car_z - gap,
            e.dt,
        );
        move_craft(
            ctx.world,
            self.second,
            self.x0 + self.side * (self.scale * 0.9 + 3.0),
            self.ship_y + 1.0,
            e.car_z - gap * 1.12,
            e.dt,
        );
        focus_on(ctx.world, self.ship, out);
        out.x = self.x0;
        out.altitude = self.a0 + (self.peak - self.a0) * bump(u, MEET - 2.4, MEET - 0.7, MEET + 0.8, MEET + 2.9);
        out.roll = self.dir * TAU * self.turns * between(u, MEET - 1.8 - (self.turns - 1.0) * 0.6, MEET + 0.9);
        out.pitch = (self.climb.follow(out.altitude, e.dt) * 0.022).clamp(-0.4, 0.4);
        out.hover = out.altitude > 0.8;
        true
    }

    fn end(&mut self, world: &mut dyn StuntWorld) {
        release(world, self.ship.take());
        release(world, self.second.take());
    }
}
