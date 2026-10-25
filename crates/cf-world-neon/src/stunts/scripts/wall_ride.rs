//! Roulage sur une façade (`scripts/wall-ride.ts`).

use super::{pick_index, side_of};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, between, lerp, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

/// Écart entre l'axe de la voiture couchée et le mur (m). Presque à la verticale : la voiture roule sur le flanc, le
/// ventre contre la façade.
const GAP: f64 = 1.15;

/// Roulis de 85° et la voiture court sur une façade d'immeuble, le ventre contre le mur, à quelques mètres du sol. Elle
/// suit les reliefs de la façade (retraits, immeubles voisins) puis se redresse et revient sur la route.
#[derive(Debug, Clone, Copy)]
pub struct WallRide;

impl StuntScript for WallRide {
    fn kind(&self) -> StuntKind {
        StuntKind::WallRide
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Uncommon
    }
    fn weight(&self) -> f64 {
        1.5
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 900.0, 30.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let beats = [16.0, 18.0, 22.0][pick_index(ctx.rng, 3)];
        let tilt = ctx.rng.range(1.3, 1.55);
        let x0 = env.car_x.clamp(-3.0, 3.0);
        Box::new(Run {
            beats,
            tilt,
            a0: env.altitude,
            x0,
            side: sign(ctx.rng),
            wall: 15.0,
            x: x0,
            height: 7.0,
            lean: Rate::default(),
            duration: beats * env.period,
        })
    }
}

struct Run {
    beats: f64,
    tilt: f64,
    a0: f64,
    x0: f64,
    side: f64,
    wall: f64,
    x: f64,
    height: f64,
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
        let side = side_of(self.side);
        if let Some(face) = ctx.world.facade(side, e.car_z - 10.0, 8.0).filter(|f| f.is_finite()) {
            self.wall += (f64::from(face) - self.wall) * (1.0 - (-e.dt * 5.0).exp());
        }
        // Hauteur de roulage : sous le sommet du mur, jamais plus de 8 m.
        if let Some(roof) = ctx
            .world
            .roof_at(self.side * (self.wall + 3.0), e.car_z - 4.0)
            .filter(|r| r.is_finite() && *r > 0.0)
        {
            self.height += ((f64::from(roof) - 3.0).clamp(4.0, 8.5) - self.height) * (1.0 - (-e.dt * 3.0).exp());
        }
        let ride = between(u, 1.8, 4.0) * (1.0 - between(u, self.beats - 4.6, self.beats - 2.2));
        let reach = between(u, 1.0, 3.4) * (1.0 - between(u, self.beats - 5.0, self.beats - 2.6));
        self.x += (lerp(self.x0, self.side * (self.wall - GAP), reach) - self.x) * (1.0 - (-e.dt * 6.0).exp());
        out.x = self.x;
        // Jamais sous la route : en fin de course l'ondulation (`ride`) survit un instant à la montée (`reach`).
        out.altitude = (lerp(self.a0, self.height, reach) + (u * 2.1).sin() * 0.25 * ride).max(0.0);
        out.roll =
            self.side * self.tilt * ride + (self.lean.follow(self.x, e.dt) * 0.03).clamp(-0.25, 0.25) * (1.0 - ride);
        out.hover = true;
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
