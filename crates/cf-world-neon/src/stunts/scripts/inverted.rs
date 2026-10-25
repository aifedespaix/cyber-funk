//! Vol sur le dos (`scripts/inverted.ts`).

use std::f64::consts::PI;

use super::{focus_on, pick_index, release, spawn_at};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, between, bump, lerp, move_craft, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;
use crate::traffic::CraftId;
use crate::traffic::variant::CARGO;

/// Temps où le centre du cargo passe au-dessus de la voiture.
const CENTER: f64 = 7.0;
const REACH: f64 = 150.0;
/// Vitesse à laquelle la voiture double le cargo une fois dessous (m/s) : lente, pour rester longtemps à l'envers.
const OVERTAKE_RANGE: (f64, f64) = (4.5, 8.0);

/// Un cargo géant plane au-dessus de la route, à peine plus lent que la voiture. Elle se retourne, passe à l'envers sous
/// sa coque, le dos à quelques mètres du ventre du vaisseau, puis se remet à l'endroit une fois sortie de dessous.
#[derive(Debug, Clone, Copy)]
pub struct Inverted;

impl StuntScript for Inverted {
    fn kind(&self) -> StuntKind {
        StuntKind::Inverted
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Uncommon
    }
    fn weight(&self) -> f64 {
        1.4
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 700.0, 30.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let period = env.period;
        // Variantes : taille du cargo, hauteur de passage, lenteur du dépassement, voiture décalée.
        let scale = ctx.rng.range(7.5, 10.0);
        let overtake = ctx.rng.range(OVERTAKE_RANGE.0, OVERTAKE_RANGE.1);
        let a0 = env.altitude;
        let x0 = env.car_x.clamp(-3.0, 3.0) + ctx.rng.range(-1.0, 1.0);
        let flip = sign(ctx.rng);
        let level = ctx.rng.range(8.0, 13.0).max(a0);
        let hull_y = level + 3.6;
        let half = scale * 1.2;
        let variant = CARGO[pick_index(ctx.rng, 2)];
        let tint = ctx.rng.range(0.0, 8.0).floor() as i64;
        let ship = spawn_at(ctx.world, variant, (x0, hull_y, env.car_z - REACH), scale, tint);
        // Le cargo arrive vite puis ralentit pour glisser au-dessus de la voiture : distance devant elle selon le temps
        // qui reste.
        let lead = CENTER * period;
        let slow = (REACH - overtake * lead) / (lead * lead);
        // La voiture se remet à l'endroit quand le cargo a dépassé son arrière, avec une marge.
        let leave = CENTER + (half + 4.0) / (overtake * period);
        let beats = (leave + 4.5).ceil();
        Box::new(Run {
            ship,
            overtake,
            slow,
            leave,
            a0,
            x0,
            flip,
            level,
            hull_y,
            climb: Rate::default(),
            duration: beats * period,
        })
    }
}

struct Run {
    ship: Option<CraftId>,
    overtake: f64,
    slow: f64,
    leave: f64,
    a0: f64,
    x0: f64,
    flip: f64,
    level: f64,
    hull_y: f64,
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
        let s = (CENTER - u) * e.period;
        let ahead = if s > 0.0 {
            self.overtake * s + self.slow * s * s
        } else {
            self.overtake * s
        };
        move_craft(ctx.world, self.ship, self.x0, self.hull_y, e.car_z - ahead, e.dt);
        focus_on(ctx.world, self.ship, out);
        let leave = self.leave;
        out.x = self.x0;
        out.altitude = lerp(self.a0, self.level, bump(u, 1.0, 3.5, leave + 1.0, leave + 3.2));
        out.roll = self.flip * PI * (between(u, CENTER - 3.8, CENTER - 2.2) + between(u, leave, leave + 1.6));
        out.pitch = (self.climb.follow(out.altitude, e.dt) * 0.02).clamp(-0.3, 0.3);
        out.hover = out.altitude > 0.8;
        true
    }

    fn end(&mut self, world: &mut dyn StuntWorld) {
        release(world, self.ship.take());
    }
}
