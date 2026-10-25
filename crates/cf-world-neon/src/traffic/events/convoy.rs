//! Convoi à contresens (`events/convoy.ts`) : trois à cinq véhicules arrivent de face au-dessus de la route, étagés dans
//! les couloirs, à quelques instants d'écart (jamais au pas), puis disparaissent derrière la caméra.

use bevy::math::{DVec3, Vec3};

use super::common::{alive, index, pick_tone, range, release};
use crate::traffic::air_lanes::LANES;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::{CraftId, Fleet};
use crate::traffic::frame::FLOOR_MARGIN_M;
use crate::traffic::motion::{Motion, Weave};

/// Un véhicule à lancer : son instant (s depuis le début) et son couloir (indice dans `LANES`).
#[derive(Debug, Clone, Copy)]
struct Launch {
    at_s: f32,
    lane: usize,
}

#[derive(Debug)]
pub(crate) struct ConvoyEvent {
    crafts: Vec<CraftId>,
    queue: Vec<Launch>,
    next: usize,
    elapsed: f32,
}

impl ConvoyEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        // Les couloirs de l'étage de la voiture sont exclus ; s'il n'en reste aucun, le convoi n'a pas lieu.
        let lanes: Vec<usize> = LANES
            .iter()
            .enumerate()
            .filter(|(_, l)| l.dir > 0 && ctx.frame.is_free(l.y, FLOOR_MARGIN_M))
            .map(|(i, _)| i)
            .collect();
        let mut queue = Vec::new();
        if !lanes.is_empty() {
            let count = 3 + index(ctx.rng, 3);
            let mut at_s = 0.0;
            for _ in 0..count {
                at_s += range(ctx.rng, 0.25, 1.1);
                queue.push(Launch {
                    at_s,
                    lane: lanes[index(ctx.rng, lanes.len())],
                });
            }
        }
        Self {
            crafts: Vec::with_capacity(queue.len()),
            queue,
            next: 0,
            elapsed: 0.0,
        }
    }
}

impl TrafficEvent for ConvoyEvent {
    fn proximity(&self, ctx: &EventCtx<'_>) -> bool {
        let car = ctx.frame.car;
        self.crafts.iter().any(|&id| {
            ctx.fleet
                .get(id)
                .is_some_and(|c| c.position.z < car.z + 70.0 && c.position.z > car.z - 230.0)
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.elapsed += dt;
        while self.next < self.queue.len() && self.queue[self.next].at_s <= self.elapsed {
            let launch = self.queue[self.next];
            self.next += 1;
            let lane = LANES[launch.lane];
            let car_z = ctx.frame.car.z;
            let origin = DVec3::new(
                f64::from(lane.x + range(ctx.rng, -1.6, 1.6)),
                f64::from(lane.y + range(ctx.rng, -1.0, 1.0)),
                car_z - 520.0,
            );
            let calm = if ctx.frame.calm { 0.5 } else { 1.0 };
            let speed = range(ctx.rng, 70.0, 105.0) * calm;
            let velocity = Vec3::new(0.0, 0.0, speed);
            let variant = Fleet::sleek(ctx.rng.unit());
            let tint = pick_tone(ctx);
            let scale = range(ctx.rng, 1.8, 2.2);
            let Some(id) = ctx.fleet.spawn(variant, origin, velocity, tint, scale, ctx.rng) else {
                continue;
            };
            let (ax, ay) = (range(ctx.rng, 0.3, 1.8), range(ctx.rng, 0.2, 0.9));
            let weave = Weave::random(ctx.rng, ax, ay);
            if let Some(craft) = ctx.fleet.get_mut(id) {
                craft.motion = Motion::path(origin, velocity, Some(weave), 0.0);
            }
            self.crafts.push(id);
        }
        let launching = self.next < self.queue.len();
        launching || self.crafts.iter().any(|&id| alive(ctx, Some(id)))
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        for id in self.crafts.drain(..) {
            release(ctx, Some(id));
        }
    }
}
