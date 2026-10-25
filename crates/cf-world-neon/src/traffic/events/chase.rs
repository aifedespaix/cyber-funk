//! Poursuite (`events/chase.ts`) : un véhicule file au-dessus de la route en slalomant, une voiture de police sur ses
//! talons, au ras de sa trajectoire. Parfois deux voitures de police ; parfois ils viennent de face.
//!
//! Le fuyard et la police suivent le même chemin (`Motion::Path`), la police avec un retard : le slalom se répète derrière.
//! Les gyrophares alternent rouge et bleu côté matière, derrière le limiteur de flashs (`TrafficFlash`, règle 7) ; ici les
//! halos des gyrophares restent à intensité constante.
//!
//! Écart : pas de faisceau de sirène qui balaie la chaussée (pas de faisceaux dans [`EventCtx`]).

use bevy::math::{DVec3, Vec3};

use super::common::{alive, index, pick_tone, range, release};
use crate::traffic::air_lanes::LANES;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::{CraftId, Fleet};
use crate::traffic::frame::FLOOR_MARGIN_M;
use crate::traffic::motion::{Motion, Weave};
use crate::traffic::variant::POLICE;

#[derive(Debug)]
pub(crate) struct ChaseEvent {
    crafts: Vec<CraftId>,
    police: Vec<CraftId>,
    /// Le fuyard, tant qu'il vit : les plans « poursuite » le filment avec la police.
    quarry: Option<CraftId>,
    alarm: bevy::color::LinearRgba,
    azure: bevy::color::LinearRgba,
}

impl ChaseEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let mut event = Self {
            crafts: Vec::with_capacity(3),
            police: Vec::with_capacity(2),
            quarry: None,
            alarm: super::common::tone(ctx, 0),
            azure: super::common::tone(ctx, 3),
        };
        let oncoming = ctx.rng.unit() < 0.4;
        let lanes: Vec<usize> = LANES
            .iter()
            .enumerate()
            .filter(|(_, l)| if oncoming { l.dir > 0 } else { l.dir < 0 })
            .filter(|(_, l)| ctx.frame.is_free(l.y, FLOOR_MARGIN_M))
            .map(|(i, _)| i)
            .collect();
        // Aucun couloir libre dans ce sens (la voiture occupe l'étage) : la poursuite n'a pas lieu.
        if lanes.is_empty() {
            return event;
        }
        let lane = LANES[lanes[index(ctx.rng, lanes.len())]];
        let speed = range(ctx.rng, 92.0, 135.0);
        let (ax, ay) = (range(ctx.rng, 2.2, 4.2), range(ctx.rng, 1.0, 2.4));
        let weave = Weave::random(ctx.rng, ax, ay);
        let dir = f32::from(lane.dir);
        let z = if dir < 0.0 {
            ctx.frame.car.z + 200.0
        } else {
            ctx.frame.car.z - 540.0
        };
        let origin = DVec3::new(f64::from(lane.x), f64::from(lane.y), z);
        let velocity = Vec3::new(0.0, 0.0, dir * speed);

        let variant = Fleet::sleek(ctx.rng.unit());
        let tint = pick_tone(ctx);
        if let Some(quarry) = ctx.fleet.spawn(variant, origin, velocity, tint, 2.0, ctx.rng) {
            if let Some(craft) = ctx.fleet.get_mut(quarry) {
                craft.motion = Motion::path(origin, velocity, Some(weave), 0.0);
            }
            event.crafts.push(quarry);
            event.quarry = Some(quarry);
        }

        // La police suit le même chemin avec un retard d'une seconde environ.
        let followers = if ctx.rng.unit() < 0.3 { 2 } else { 1 };
        for i in 0..followers {
            let delay = range(ctx.rng, 0.8, 1.2) + i as f32 * range(ctx.rng, 0.45, 0.7);
            let Some(car) = ctx.fleet.spawn(POLICE, origin, velocity, event.azure, 2.1, ctx.rng) else {
                continue;
            };
            if let Some(craft) = ctx.fleet.get_mut(car) {
                craft.motion = Motion::path(origin, velocity, Some(weave), -delay);
                craft.trail = 1.2;
            }
            event.crafts.push(car);
            event.police.push(car);
        }
        event
    }
}

impl TrafficEvent for ChaseEvent {
    fn proximity(&self, ctx: &EventCtx<'_>) -> bool {
        let car = ctx.frame.car;
        self.crafts.iter().any(|&id| {
            ctx.fleet
                .get(id)
                .is_some_and(|c| c.position.z < car.z + 70.0 && c.position.z > car.z - 230.0)
        })
    }

    fn quarry(&self) -> Option<CraftId> {
        self.quarry
    }

    fn update(&mut self, _dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        if !self.crafts.iter().any(|&id| alive(ctx, Some(id))) {
            return false;
        }
        if self.quarry.is_some_and(|id| !alive(ctx, Some(id))) {
            self.quarry = None;
        }
        // Halos des deux gyrophares, de part et d'autre du toit, en lumière continue.
        for &car_id in &self.police {
            let Some(car) = ctx.fleet.get(car_id) else {
                continue;
            };
            let side = f64::from(car.roll.cos() * 0.3 * car.scale);
            let p = car.position;
            ctx.glints
                .add(DVec3::new(p.x - side, p.y + 0.9, p.z), 3.2, self.alarm, 1.7);
            ctx.glints
                .add(DVec3::new(p.x + side, p.y + 0.9, p.z), 3.2, self.azure, 1.7);
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        for id in self.crafts.drain(..) {
            release(ctx, Some(id));
        }
        self.police.clear();
        self.quarry = None;
    }
}
