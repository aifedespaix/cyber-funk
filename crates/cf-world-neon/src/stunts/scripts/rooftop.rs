//! Sur les toits (`scripts/rooftop.ts`).

use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, lerp, sign, turn};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

const BEATS: f64 = 26.0;
/// Axe des toits du premier rang (leurs façades commencent vers 13,5 et leurs toits s'étendent au-delà de 21).
const LANE_RANGE: (f64, f64) = (16.5, 19.0);
/// Altitude de croisière au-dessus de tous les premiers immeubles (le plus haut fait 28 m).
const SKY_RANGE: (f64, f64) = (31.0, 38.0);
const HOVER: f64 = 1.4;
const LOOKAHEAD: f64 = 66.0;

/// La voiture s'élève au-dessus de la ville, se pose sur les toits d'un côté de la route et les parcourt : elle monte sur
/// chaque immeuble qui vient, plane au-dessus des vides, rebondit sur les temps. Puis elle s'envole, boucle un salto
/// avant et retombe sur la route. Une rare grosse cascade, lancée sur une phrase.
#[derive(Debug, Clone, Copy)]
pub struct Rooftop;

impl StuntScript for Rooftop {
    fn kind(&self) -> StuntKind {
        StuntKind::Rooftop
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Rare
    }
    fn weight(&self) -> f64 {
        1.4
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Phrase
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 1000.0, 45.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let x0 = env.car_x.clamp(-3.0, 3.0);
        let side = sign(ctx.rng);
        let lane = ctx.rng.range(LANE_RANGE.0, LANE_RANGE.1);
        let sky = ctx.rng.range(SKY_RANGE.0, SKY_RANGE.1);
        let salto = sign(ctx.rng);
        Box::new(Run {
            a0: env.altitude,
            x0,
            side,
            lane,
            sky,
            salto,
            altitude: env.altitude,
            x: x0,
            climb: Rate::default(),
            duration: BEATS * env.period,
        })
    }
}

struct Run {
    a0: f64,
    x0: f64,
    side: f64,
    lane: f64,
    sky: f64,
    salto: f64,
    altitude: f64,
    x: f64,
    climb: Rate,
    duration: f64,
}

impl Run {
    /// Le toit le plus haut à `(at, z)` ou 3 m plus loin du côté choisi ; −1 sans immeuble.
    fn roofs(&self, world: &dyn StuntWorld, z: f64, at: f64) -> f64 {
        let roof = |x: f64| world.roof_at(x, z).filter(|r| r.is_finite()).map_or(-1.0, f64::from);
        roof(at).max(roof(at + self.side * 3.0))
    }
}

impl StuntRun for Run {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let e = ctx.env;
        let u = t / e.period;
        let end = BEATS;
        let ride = between(u, 3.0, 5.0) * (1.0 - between(u, end - 6.5, end - 5.5));
        // Le toit le plus haut sur les soixante mètres à venir : la voiture le survole avant d'y arriver.
        let mut roof = -1.0_f64;
        let mut k = -1.0;
        while k * 6.0 <= LOOKAHEAD {
            roof = roof.max(self.roofs(&*ctx.world, e.car_z - k * 6.0, self.x));
            k += 1.0;
        }
        let sky = lerp(self.sky, (roof + HOVER).max(4.5), ride);
        let want = lerp(sky, self.a0, between(u, end - 3.6, end - 0.8));
        let rate = if want > self.altitude {
            lerp(1.5, 4.4, ride)
        } else if ride > 0.5 {
            1.0
        } else {
            2.2
        };
        self.altitude += (want - self.altitude) * (1.0 - (-e.dt * rate).exp());
        // Jamais dans un toit : le plancher suit l'immeuble sous la voiture.
        if ride > 0.3 {
            self.altitude = self.altitude.max(self.roofs(&*ctx.world, e.car_z, self.x) + 1.0);
        }
        let lane = lerp(
            lerp(self.x0, self.side * self.lane, between(u, 2.0, 5.0)),
            self.x0,
            between(u, end - 6.0, end - 3.0),
        );
        self.x += (lane - self.x) * (1.0 - (-e.dt * 5.0).exp());
        out.x = self.x;
        out.altitude = self.altitude + f64::from(e.beat.pulse) * 0.3 * ride;
        out.pitch = (self.climb.follow(self.altitude, e.dt) * 0.02).clamp(-0.4, 0.4)
            - self.salto * TAU * turn(u, end - 3.6, end - 1.2);
        out.roll = self.side * -0.08 * ride;
        out.hover = true;
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
