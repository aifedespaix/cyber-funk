//! Escorte (`events/escort.ts`) : un véhicule plonge du ciel, se cale à côté de la voiture au ras de la chaussée, roule un
//! moment à sa hauteur (il corrige sans cesse, comme un pilote), puis remet les gaz et file dans la nuit. Mouvement par
//! ressort : rien n'est tracé d'avance, tout dépend de la voiture.

use bevy::math::{DVec3, Vec3};

use super::common::{alive, pick_tone, range, release, set_pose, spawn_scripted};
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::{CraftId, Fleet};

/// Secondes passées à hauteur de la voiture avant de s'éclipser.
const STAY_S: f32 = 8.0;

#[derive(Debug)]
pub(crate) struct EscortEvent {
    craft: Option<CraftId>,
    side: f64,
    offset: DVec3,
    departed: bool,
    age: f32,
}

impl EscortEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let car = ctx.frame.car;
        let side = if car.x.abs() > 2.0 {
            -car.x.signum()
        } else if ctx.rng.coin() {
            -1.0
        } else {
            1.0
        };
        let origin = DVec3::new(car.x + side * 24.0, 46.0, car.z + 90.0);
        let velocity = Vec3::new(0.0, -6.0, -ctx.frame.speed - 28.0);
        let variant = Fleet::sleek(ctx.rng.unit());
        let tint = pick_tone(ctx);
        let craft = spawn_scripted(ctx, variant, origin, velocity, tint, 2.1);
        let offset = DVec3::new(
            f64::from(range(ctx.rng, -1.2, 1.2)),
            0.0,
            f64::from(range(ctx.rng, -3.5, -1.0)),
        );
        Self {
            craft,
            side,
            offset,
            departed: false,
            age: 0.0,
        }
    }
}

impl TrafficEvent for EscortEvent {
    fn proximity(&self, ctx: &EventCtx<'_>) -> bool {
        let Some(craft) = self.craft.and_then(|id| ctx.fleet.get(id)) else {
            return false;
        };
        let car = ctx.frame.car;
        craft.position.z < car.z + 70.0 && craft.position.z > car.z - 230.0 && craft.position.y < 30.0
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        let Some(id) = self.craft else {
            return false;
        };
        self.age += dt;
        if !alive(ctx, Some(id)) {
            return false;
        }
        let car = ctx.frame.car;
        let climbing = self.age > STAY_S || ctx.frame.altitude > 1.5;
        let Some(craft) = ctx.fleet.get_mut(id) else {
            return false;
        };
        if climbing && !self.departed {
            self.departed = true;
            craft.trail = 1.6;
        }
        // Cible : à côté de la voiture ; une fois en hauteur, haut et loin devant.
        let slot = if climbing {
            DVec3::new(car.x + self.side * 30.0, 70.0, car.z - 260.0)
        } else {
            let x = (car.x + self.side * 6.2).clamp(-7.4, 7.4) + self.offset.x;
            DVec3::new(x, 1.7, car.z + self.offset.z)
        };
        let (stiffness, damping) = if climbing { (1.1, 0.9) } else { (3.4, 2.8) };
        let car_velocity = -ctx.frame.speed;
        let mut v = craft.velocity;
        let p = craft.position;
        let dx = (slot.x - p.x) as f32;
        let dy = (slot.y - p.y) as f32;
        let dz = (slot.z - p.z) as f32;
        v.x += (dx * stiffness - v.x * damping) * dt;
        v.y += (dy * stiffness - v.y * damping) * dt;
        v.z += (dz * stiffness - (v.z - car_velocity) * damping) * dt;
        if climbing {
            v.z -= 40.0 * dt;
        }
        let position = p + v.as_dvec3() * f64::from(dt);
        set_pose(ctx, id, position, v);

        // Lueur sous la coque : sert d'éclairage à la scène, la route lui répond.
        if position.y < 6.0 {
            let tint = ctx.fleet.get(id).map_or(bevy::color::LinearRgba::WHITE, |c| c.tint);
            ctx.glints
                .add(DVec3::new(position.x, position.y - 0.3, position.z), 5.5, tint, 0.5);
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        release(ctx, self.craft.take());
    }
}
