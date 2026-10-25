//! Saut d'enseigne (`scripts/sign-hop.ts`).

use std::cell::RefCell;

use bevy::math::DVec3;

use super::IdleRun;
use crate::pilot::flight::Obstruction;
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, sign, turn};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

/// Les structures plus hautes (arches-immeubles) sont trop grosses pour un saut.
const MAX_TOP: f64 = 30.0;
/// Distance (m) à laquelle la structure doit se trouver devant la voiture : ni trop près, ni trop loin.
const NEAR: f64 = 75.0;
const FAR: f64 = 320.0;

thread_local! {
    /// Tampon des structures au-dessus de la route : alloué une fois (à la première demande), réutilisé ensuite.
    static OVERHEAD: RefCell<Vec<Obstruction>> = const { RefCell::new(Vec::new()) };
}

/// La structure en travers de la route la plus proche à portée de saut : panneau sur portique, bras, portique néon.
fn target(car_z: f64, world: &dyn StuntWorld) -> Option<Obstruction> {
    OVERHEAD.with(|cell| {
        let mut overhead = cell.borrow_mut();
        overhead.clear();
        world.overhead(&mut overhead, car_z);
        let mut best: Option<Obstruction> = None;
        let mut best_ahead = f64::INFINITY;
        for o in overhead.iter() {
            let ahead = car_z - f64::midpoint(o.z_near, o.z_far);
            if !(NEAR..=FAR).contains(&ahead) || o.hi > MAX_TOP || ahead >= best_ahead {
                continue;
            }
            best = Some(*o);
            best_ahead = ahead;
        }
        best
    })
}

/// Salto arrière par dessus un panneau publicitaire sur son portique (ou un portique néon) : la voiture monte en arc,
/// boucle au-dessus de la structure et se repose derrière. Calée sur la structure, pas sur la musique.
#[derive(Debug, Clone, Copy)]
pub struct SignHop;

impl StuntScript for SignHop {
    fn kind(&self) -> StuntKind {
        StuntKind::SignHop
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
        target(env.car_z, world).is_some()
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        // La structure a pu sortir de la fenêtre entre `available` et `begin` : on ne joue rien plutôt que de viser dans le vide.
        let Some(o) = target(env.car_z, ctx.world) else {
            return Box::new(IdleRun);
        };
        let z = f64::midpoint(o.z_near, o.z_far);
        let a0 = env.altitude;
        let apex = a0.max(o.hi + ctx.rng.range(3.0, 6.0));
        // Salto arrière ou avant, voiture un peu décalée de l'axe.
        let salto = sign(ctx.rng);
        let shift = ctx.rng.range(-1.0, 1.0);
        let reach = ctx.rng.range(30.0, 42.0);
        let start = env.car_z - z;
        Box::new(Run {
            z,
            hi: o.hi,
            a0,
            apex,
            salto,
            shift,
            reach,
            climb: Rate::default(),
            duration: (start + 140.0) / env.speed.max(12.0) + 0.6,
        })
    }
}

struct Run {
    z: f64,
    hi: f64,
    a0: f64,
    apex: f64,
    salto: f64,
    shift: f64,
    reach: f64,
    climb: Rate,
    duration: f64,
}

impl StuntRun for Run {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn step(&mut self, _t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let e = ctx.env;
        // Tout se joue à la distance de la structure (m, positive devant la voiture), pas au temps.
        let d = e.car_z - self.z;
        if d < -130.0 {
            return false;
        }
        out.focus = DVec3::new(0.0, self.hi, self.z);
        out.x = (e.car_x + self.shift).clamp(-3.5, 3.5);
        out.altitude = self.a0 + (self.apex - self.a0) * between(d, 110.0, 12.0) * (1.0 - between(d, -10.0, -120.0));
        out.pitch = self.salto * TAU * turn(d, self.reach, -self.reach)
            + (self.climb.follow(out.altitude, e.dt) * 0.02).clamp(-0.3, 0.3);
        out.hover = out.altitude > 0.8;
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
