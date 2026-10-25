//! Ravitaillement (`events/refuel.ts`) : un pétrolier géant rattrape la voiture bien au-dessus d'elle (60 m ou plus : au-dessus
//! des plus hauts étages), deux petits engins accrochés à sa coque par des flexibles de lumière dont le carburant circule
//! en impulsions au tempo. Les engins collés au pétrolier ne descendent jamais jusqu'à la route.
//!
//! Écarts (voir la PR) : les deux projecteurs qui balaient la chaussée sont publiés dans la scène (voir `airship`) ; les
//! feux de position ne clignotent pas (le clignotement passerait par le limiteur de flashs, accessible côté matière
//! seulement) ; l'amplitude de l'impulsion des flexibles est réduite (0,9 + 0,6 × impulsion, au lieu de 0,9 + 2,4 ×).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_render::SceneColor;

use super::common::{alive, index, pick_tone, pin, range, release, set_pose, spawn_scripted, tone};
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::{CraftId, Fleet};
use crate::traffic::variant::CARGO;

const TANKER_SCALE: f32 = 30.0;
const HOSE_POINTS: usize = 12;
const LIFE_S: f32 = 30.0;

#[derive(Debug, Clone, Copy)]
struct Client {
    id: CraftId,
    offset: Vec3,
    index: f32,
}

#[derive(Debug)]
pub(crate) struct RefuelEvent {
    age: f32,
    tanker: Option<CraftId>,
    altitude: f32,
    phase: f32,
    clients: Vec<Client>,
    hose: LinearRgba,
    lamp_left: LinearRgba,
    lamp_right: LinearRgba,
}

impl RefuelEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let altitude = range(ctx.rng, 66.0, 82.0);
        let phase = range(ctx.rng, 0.0, std::f32::consts::TAU);
        let speed = ctx.frame.speed;
        let extra = if ctx.frame.calm { 4.0 } else { 9.0 };
        let velocity = Vec3::new(0.0, 0.0, -(speed + extra));
        let origin = DVec3::new(
            f64::from(side * range(ctx.rng, 18.0, 46.0)),
            f64::from(altitude),
            ctx.frame.car.z + 200.0,
        );
        let tint = pick_tone(ctx);
        let variant = CARGO[index(ctx.rng, CARGO.len())];
        let tanker = spawn_scripted(ctx, variant, origin, velocity, tint, TANKER_SCALE);
        let mut event = Self {
            age: 0.0,
            tanker,
            altitude,
            phase,
            clients: Vec::with_capacity(2),
            hose: tone(ctx, 0),
            lamp_left: tone(ctx, 1),
            lamp_right: tone(ctx, 3),
        };
        if let Some(tanker_id) = tanker {
            if let Some(craft) = ctx.fleet.get_mut(tanker_id) {
                craft.trail = 0.0;
            }
            for i in 0..2 {
                let offset = Vec3::new(
                    (if i == 0 { -1.0 } else { 1.0 }) * TANKER_SCALE * 0.62,
                    -TANKER_SCALE * 0.62,
                    TANKER_SCALE * if i == 0 { 0.1 } else { -0.25 },
                );
                let variant = Fleet::sleek(ctx.rng.unit());
                if let Some(id) = spawn_scripted(ctx, variant, origin, velocity, tint, 2.4) {
                    if let Some(craft) = ctx.fleet.get_mut(id) {
                        craft.trail = 0.4;
                    }
                    event.clients.push(Client {
                        id,
                        offset,
                        index: i as f32,
                    });
                }
            }
        }
        event
    }
}

impl TrafficEvent for RefuelEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        let Some(tanker) = self.tanker else {
            return false;
        };
        if self.age > LIFE_S || !alive(ctx, Some(tanker)) {
            return false;
        }
        // Le pétrolier avance à sa vitesse et ondule en hauteur.
        let Some(craft) = ctx.fleet.get(tanker) else {
            return false;
        };
        let velocity = craft.velocity;
        let scale = craft.scale;
        let moved = craft.position + velocity.as_dvec3() * f64::from(dt);
        let position = DVec3::new(
            moved.x,
            f64::from(self.altitude + (self.age * 0.3 + self.phase).sin() * 1.4),
            moved.z,
        );
        set_pose(ctx, tanker, position, velocity);

        // Feux de position, en lumière continue (voir l'écart en tête de module).
        let side = f64::from(scale * 0.9);
        let lamp_y = f64::from(scale * 0.1);
        let lamp_z = f64::from(scale * 0.2);
        ctx.glints.add(
            position + DVec3::new(-side, lamp_y, lamp_z),
            scale * 0.2,
            self.lamp_left,
            2.2,
        );
        ctx.glints.add(
            position + DVec3::new(side, lamp_y, lamp_z),
            scale * 0.2,
            self.lamp_right,
            2.2,
        );

        // Projecteurs sur la route tant que le pétrolier est à portée.
        let car_z = ctx.frame.car.z;
        let dz = position.z - car_z;
        if dz < 60.0 && dz > -260.0 {
            let time = ctx.frame.time as f32;
            for i in 0..2 {
                let t = time * 0.4 + self.phase + i as f32 * 2.1;
                let side = if i == 0 { -1.0 } else { 1.0 };
                let origin = position + DVec3::new(f64::from(side * scale * 0.3), f64::from(-scale * 0.2), 0.0);
                let target_z = car_z - 25.0 - 40.0 * f64::from(0.5 + 0.5 * (t * 0.8 + i as f32).sin());
                let tint = ctx
                    .frame
                    .named(if i == 0 { SceneColor::Amber } else { SceneColor::Ice });
                pin(ctx, origin, t.sin() * 7.0, target_z, tint);
            }
        }

        // Les engins se tiennent sous le flexible et tanguent doucement.
        let t = self.age;
        let pulse = ctx.frame.dance_beat.fract() as f32;
        for ci in 0..self.clients.len() {
            let client = self.clients[ci];
            let i = client.index;
            let anchor = position
                + DVec3::new(
                    f64::from(client.offset.x) * 0.9,
                    f64::from(-scale * 0.12),
                    f64::from(client.offset.z),
                );
            let pose = position
                + DVec3::new(
                    f64::from(client.offset.x + (t * 0.9 + i).sin() * 0.9),
                    f64::from(client.offset.y + (t * 1.4 + i * 2.0).sin() * 0.7),
                    f64::from(client.offset.z + (t * 0.7).sin() * 1.2),
                );
            if !set_pose(ctx, client.id, pose, velocity) {
                continue;
            }
            let body = pose + DVec3::new(0.0, 0.8, 0.0);
            for k in 0..HOSE_POINTS {
                let u = k as f32 / (HOSE_POINTS - 1) as f32;
                let sag = (u * std::f32::consts::PI).sin() * -1.4;
                // Impulsion qui court le long du flexible, au tempo.
                let wave = (1.0 - (((u - pulse * 1.3) % 1.0 + 1.0) % 1.0 - 0.5).abs() * 4.0).max(0.0);
                let p = anchor.lerp(body, f64::from(u)) + DVec3::new(0.0, f64::from(sag), 0.0);
                ctx.glints.add(p, 0.7 + wave * 1.4, self.hose, 0.9 + wave * 0.6);
            }
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        release(ctx, self.tanker.take());
        for client in self.clients.drain(..) {
            release(ctx, Some(client.id));
        }
    }
}
