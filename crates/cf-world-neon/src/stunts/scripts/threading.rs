//! Passage dans une structure (`scripts/threading.ts`).

use super::{chance, spawn_at};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, lerp, move_craft, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;
use crate::traffic::CraftId;
use crate::traffic::variant::CARGO;

const BEATS: f64 = 18.0;

/// Un passage : temps, altitude du dessous du vaisseau, altitude de la voiture, et « par dessus » (tonneau).
#[derive(Debug, Clone, Copy)]
struct Pass {
    at: f64,
    hull: f64,
    car: f64,
    over: bool,
}

const fn pass(at: f64, hull: f64, car: f64, over: bool) -> Pass {
    Pass { at, hull, car, over }
}

/// Dessous, par dessus, dessous.
const UNDER_OVER: [Pass; 3] = [
    pass(5.0, 6.2, 2.4, false),
    pass(8.5, 0.9, 8.2, true),
    pass(12.0, 6.6, 2.6, false),
];
/// L'autre ordre : par dessus, dessous, par dessus.
const OVER_UNDER: [Pass; 3] = [
    pass(5.0, 0.9, 8.2, true),
    pass(8.5, 6.2, 2.4, false),
    pass(12.0, 0.9, 8.4, true),
];

/// Trois cargos traversent la route l'un après l'autre, à des hauteurs alternées. La voiture passe sous le premier, par
/// dessus le deuxième (en tonneau), sous le troisième : un parcours entre les vaisseaux.
#[derive(Debug, Clone, Copy)]
pub struct Threading;

impl StuntScript for Threading {
    fn kind(&self) -> StuntKind {
        StuntKind::Threading
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
        !world.bridge_ahead(env.car_z, 450.0, 25.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let first = sign(ctx.rng);
        let gates = if chance(ctx.rng, 0.65) { UNDER_OVER } else { OVER_UNDER };
        Box::new(Run {
            gates,
            ships: [None; 3],
            a0: env.altitude,
            x0: env.car_x.clamp(-2.5, 2.5),
            first,
            cross: ctx.rng.range(22.0, 34.0),
            scale: ctx.rng.range(4.8, 6.5),
            relative: env.speed,
            climb: Rate::default(),
            duration: BEATS * env.period,
        })
    }
}

struct Run {
    gates: [Pass; 3],
    ships: [Option<CraftId>; 3],
    a0: f64,
    x0: f64,
    first: f64,
    cross: f64,
    scale: f64,
    relative: f64,
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
        for (i, gate) in self.gates.iter().enumerate() {
            let dir = self.first * if i % 2 == 0 { 1.0 } else { -1.0 };
            let across = self.x0 + dir * (u - gate.at) * e.period * self.cross;
            let z = e.car_z - self.relative * e.period * (gate.at - u);
            // Le vaisseau n'apparaît que lorsqu'il est assez proche de la route : il surgit de derrière les immeubles.
            if self.ships[i].is_none() && across.abs() < 46.0 && u < gate.at {
                self.ships[i] = spawn_at(
                    ctx.world,
                    CARGO[i % 2],
                    (across, gate.hull, z),
                    self.scale,
                    (i * 3 + 1) as i64,
                );
            }
            move_craft(ctx.world, self.ships[i], across, gate.hull, z, e.dt);
        }
        let [g1, g2, g3] = self.gates;
        // Altitude : un palier par vaisseau, on glisse de l'un à l'autre entre les passages.
        let altitude = lerp(
            lerp(
                lerp(self.a0, g1.car, between(u, 0.5, g1.at - 1.2)),
                g2.car,
                between(u, g1.at + 0.4, g2.at - 1.3),
            ),
            g3.car,
            between(u, g2.at + 0.4, g3.at - 1.3),
        );
        out.altitude = lerp(altitude, self.a0, between(u, g3.at + 1.5, BEATS - 1.0));
        out.x = lerp(self.x0, 0.0, between(u, 0.0, 3.0));
        out.roll = TAU
            * self.first
            * self
                .gates
                .iter()
                .map(|g| {
                    if g.over {
                        between(u, g.at - 1.0, g.at + 1.0)
                    } else {
                        0.0
                    }
                })
                .sum::<f64>();
        out.pitch = (self.climb.follow(out.altitude, e.dt) * 0.02).clamp(-0.35, 0.35);
        out.hover = true;
        true
    }

    fn end(&mut self, world: &mut dyn StuntWorld) {
        for ship in &mut self.ships {
            if let Some(id) = ship.take() {
                world.release(id);
            }
        }
    }
}
