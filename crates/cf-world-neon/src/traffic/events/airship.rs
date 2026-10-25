//! Cargo géant (`events/airship.ts`) : une masse lente qui traverse le ciel au-dessus des tours, feux de position et bandes
//! néon sur toute la coque. Plus lent que la voiture : elle le rattrape et passe dessous.
//!
//! Écarts (voir la PR) : feux de position en lumière continue (le clignotement passerait par le limiteur de flashs,
//! accessible côté matière seulement). Les deux projecteurs qui balaient la route sont publiés dans la scène
//! ([`Scene::pins`](crate::traffic::scene::Scene::pins)) ; le système de faisceaux du monde (J5-07) les dessinera.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_render::SceneColor;

use super::common::{alive, index, pick_tone, pin, range, release, set_pose, spawn_scripted, tone};
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::CraftId;
use crate::traffic::variant::CARGO;

#[derive(Debug)]
pub(crate) struct AirshipEvent {
    craft: Option<CraftId>,
    base: DVec3,
    velocity: Vec3,
    bob: f32,
    scale: f32,
    lamp_left: LinearRgba,
    lamp_right: LinearRgba,
    belly: LinearRgba,
}

impl AirshipEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let scale = range(ctx.rng, 26.0, 36.0);
        let base = DVec3::new(
            f64::from(side * range(ctx.rng, 70.0, 140.0)),
            f64::from(range(ctx.rng, 88.0, 118.0)),
            ctx.frame.car.z - 430.0,
        );
        let calm = if ctx.frame.calm { 0.8 } else { 1.0 };
        let velocity = Vec3::new(-side * range(ctx.rng, 2.0, 6.0), 0.0, -range(ctx.rng, 9.0, 14.0) * calm);
        let variant = CARGO[index(ctx.rng, CARGO.len())];
        let tint = pick_tone(ctx);
        let craft = spawn_scripted(ctx, variant, base, velocity, tint, scale);
        if let Some(c) = craft.and_then(|id| ctx.fleet.get_mut(id)) {
            c.trail = 0.0;
        }
        Self {
            craft,
            base,
            velocity,
            bob: range(ctx.rng, 0.0, std::f32::consts::TAU),
            scale,
            lamp_left: tone(ctx, 0),
            lamp_right: tone(ctx, 2),
            belly: tone(ctx, 4),
        }
    }
}

impl TrafficEvent for AirshipEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        let Some(id) = self.craft else {
            return false;
        };
        if !alive(ctx, Some(id)) {
            return false;
        }
        let Some(craft) = ctx.fleet.get(id) else {
            return false;
        };
        let moved = craft.position + self.velocity.as_dvec3() * f64::from(dt);
        let age = craft.age;
        // Le cargo ondule en hauteur autour de son étage.
        let position = DVec3::new(
            moved.x,
            self.base.y + f64::from((age * 0.35 + self.bob).sin() * 2.2),
            moved.z,
        );
        set_pose(ctx, id, position, self.velocity);

        // Feux de position et phare de ventre, en lumière continue.
        let s = self.scale;
        let (sx, sy, sz) = (f64::from(s * 0.9), f64::from(s * 0.1), f64::from(s * 0.2));
        ctx.glints
            .add(position + DVec3::new(-sx, sy, sz), s * 0.22, self.lamp_left, 2.2);
        ctx.glints
            .add(position + DVec3::new(sx, sy, sz), s * 0.22, self.lamp_right, 2.2);
        ctx.glints.add(
            position + DVec3::new(0.0, f64::from(-s * 0.1), f64::from(-s * 0.9)),
            s * 0.28,
            self.belly,
            1.4,
        );

        // Projecteurs : actifs tant que le cargo est à portée de la route.
        let car_z = ctx.frame.car.z;
        let dz = position.z - car_z;
        if dz < 40.0 && dz > -240.0 {
            let time = ctx.frame.time as f32;
            for i in 0..2 {
                let t = time * 0.45 + self.bob + i as f32 * 2.3;
                let side = if i == 0 { -1.0 } else { 1.0 };
                let origin =
                    position + DVec3::new(f64::from(side * s * 0.3), f64::from(-s * 0.25), f64::from(-s * 0.4));
                let target_z = car_z - 20.0 - 45.0 * f64::from(0.5 + 0.5 * (t * 0.7 + i as f32).sin());
                let tint = ctx
                    .frame
                    .named(if i == 0 { SceneColor::Ice } else { SceneColor::Azure });
                pin(ctx, origin, t.sin() * 7.5, target_z, tint);
            }
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        release(ctx, self.craft.take());
    }
}
