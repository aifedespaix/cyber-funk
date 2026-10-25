//! Frôlement (`scripts/near-miss.ts`).

use super::{focus_on, pick_index, release, spawn_at};
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, bump, move_craft, sign};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;
use crate::traffic::CraftId;
use crate::traffic::variant::{POLICE, SLEEK};

/// Retards (temps) des policiers derrière le fuyard : on en tire un à trois.
const DELAYS: [f64; 3] = [0.8, 1.5, 2.2];

/// Une poursuite arrive par l'arrière à toute vitesse, dans la voie de la voiture : le fuyard, puis deux voitures de
/// police sur ses talons. Au dernier moment la voiture s'écarte en tonneau et les laisse filer au ras de sa carrosserie.
#[derive(Debug, Clone, Copy)]
pub struct NearMiss;

impl StuntScript for NearMiss {
    fn kind(&self) -> StuntKind {
        StuntKind::NearMiss
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Common
    }
    fn weight(&self) -> f64 {
        2.4
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, _env: &StuntEnv<'_>, _world: &dyn StuntWorld) -> bool {
        true
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let period = env.period;
        // Variantes : fuyard plus ou moins tôt, 1 à 3 policiers, ampleur de l'écart.
        let pass = ctx.rng.range(7.0, 10.0).floor();
        let beats = pass + 6.0;
        let followers = 1 + pick_index(ctx.rng, 3);
        let spread = ctx.rng.range(3.6, 5.2);
        let a0 = env.altitude;
        let x0 = env.car_x.clamp(-2.5, 2.5);
        let aside = sign(ctx.rng);
        // Vitesse de rattrapage réglée pour que le fuyard parte à ~200 m derrière (la flotte oublie au-delà de 280 m).
        let rush = (200.0 / (pass * period)).clamp(45.0, 90.0);
        let flight_level = a0.max(1.6);
        let tint = ctx.rng.range(0.0, 8.0).floor() as i64;
        let quarry = spawn_at(
            ctx.world,
            SLEEK[2],
            (x0, flight_level, env.car_z + rush * pass * period),
            2.0,
            tint,
        );
        let mut police = Vec::with_capacity(followers);
        for (i, delay) in DELAYS.iter().take(followers).enumerate() {
            let id = spawn_at(
                ctx.world,
                POLICE,
                (x0, flight_level, env.car_z + rush * (pass + delay) * period),
                2.1,
                i64::from(i as u32),
            );
            if let Some(craft) = id.and_then(|id| ctx.world.craft_mut(id)) {
                craft.trail = 1.2;
            }
            police.push(id);
        }
        Box::new(Run {
            quarry,
            police,
            pass,
            rush,
            x0,
            a0,
            flight_level,
            aside,
            spread,
            weave: ctx.rng.range(0.0, 6.0),
            climb: Rate::default(),
            duration: beats * period,
        })
    }
}

struct Run {
    quarry: Option<CraftId>,
    police: Vec<Option<CraftId>>,
    pass: f64,
    rush: f64,
    x0: f64,
    a0: f64,
    flight_level: f64,
    aside: f64,
    spread: f64,
    weave: f64,
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
        let at = |pass: f64| e.car_z + self.rush * e.period * (pass - u);
        let lane = |pass: f64| self.x0 + ((u - pass) * 1.1 + self.weave).sin() * 1.1;
        move_craft(
            ctx.world,
            self.quarry,
            lane(self.pass),
            self.flight_level,
            at(self.pass),
            e.dt,
        );
        for (i, car) in self.police.iter().enumerate() {
            let pass = self.pass + DELAYS[i];
            move_craft(ctx.world, *car, lane(pass), self.flight_level, at(pass), e.dt);
        }
        focus_on(ctx.world, self.quarry, out);
        // Le dernier recul : la voiture saute d'un côté, tonneau dans le sens de l'esquive.
        let pass = self.pass;
        let dodge = bump(u, pass - 1.8, pass - 0.5, pass + 1.2, pass + 3.0);
        out.x = self.x0 + self.aside * self.spread * dodge;
        out.altitude = self.a0 + 1.6 * dodge;
        out.roll = self.aside * TAU * between(u, pass - 1.4, pass + 0.8);
        out.pitch = (self.climb.follow(out.altitude, e.dt) * 0.02).clamp(-0.25, 0.25);
        out.hover = true;
        true
    }

    fn end(&mut self, world: &mut dyn StuntWorld) {
        release(world, self.quarry.take());
        for car in &mut self.police {
            release(world, car.take());
        }
    }
}
