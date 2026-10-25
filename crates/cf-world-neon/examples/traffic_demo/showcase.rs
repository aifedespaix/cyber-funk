//! La vitrine : un événement de test qui pose devant la voiture de quoi juger le rendu du trafic.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_world_neon::traffic::bike_patterns::PATTERNS;
use cf_world_neon::traffic::motion::Motion;
use cf_world_neon::traffic::variant::{BIKE, HEAVY, POLICE, SLEEK};
use cf_world_neon::traffic::walls::WallId;
use cf_world_neon::traffic::{CraftId, EventCtx, TrafficEvent};

/// Hauteur du centre de la moto : l'origine du modèle est l'axe de la roue arrière, le sol est à `−WHEEL × échelle`.
const BIKE_SCALE: f32 = 1.4;
const BIKE_Y: f64 = 0.36 * BIKE_SCALE as f64;
/// Motos de la figure du manège.
const BIKES: usize = 3;

/// Un véhicule scénarisé : décalage par rapport à la voiture (x, y, z devant : z < 0).
#[derive(Debug)]
struct Placed {
    id: CraftId,
    offset: DVec3,
}

/// La vitrine.
#[derive(Debug)]
pub struct Showcase {
    placed: Vec<Placed>,
    bikes: Vec<(CraftId, WallId)>,
    age: f32,
}

impl Showcase {
    /// Pose les véhicules : la police, deux profilés, un mineur et un cargo en l'air, trois motos et leurs murs.
    pub fn new(ctx: &mut EventCtx<'_>) -> Self {
        let tint = |i: usize| ctx.frame.cycle[i % ctx.frame.cycle.len()];
        let mut placed = Vec::new();
        let mut spawn = |ctx: &mut EventCtx<'_>, variant: u8, scale: f32, offset: DVec3, tint: LinearRgba| {
            if let Some(id) = ctx
                .fleet
                .spawn(variant, ctx.frame.car + offset, Vec3::ZERO, tint, scale, ctx.rng)
            {
                if let Some(craft) = ctx.fleet.get_mut(id) {
                    craft.motion = Motion::Scripted;
                }
                placed.push(Placed { id, offset });
            }
        };
        let tints: [LinearRgba; 4] = [tint(0), tint(1), tint(2), tint(4)];
        spawn(ctx, POLICE, 1.9, DVec3::new(3.2, 0.4, -26.0), tints[0]);
        spawn(ctx, SLEEK[0], 1.9, DVec3::new(-3.4, 0.4, -20.0), tints[1]);
        spawn(ctx, SLEEK[2], 1.9, DVec3::new(0.2, 0.4, -34.0), tints[3]);
        spawn(ctx, HEAVY[0], 2.8, DVec3::new(-7.0, 13.0, -52.0), tints[2]);
        spawn(ctx, HEAVY[1], 2.8, DVec3::new(8.0, 10.0, -78.0), tints[1]);
        let mut bikes = Vec::new();
        for i in 0..BIKES {
            let color = tints[i + 1];
            let Some(wall) = ctx.walls.claim(color) else { continue };
            let Some(id) = ctx
                .fleet
                .spawn(BIKE, ctx.frame.car, Vec3::ZERO, color, BIKE_SCALE, ctx.rng)
            else {
                continue;
            };
            if let Some(craft) = ctx.fleet.get_mut(id) {
                craft.motion = Motion::Scripted;
                craft.lean = 2.5;
            }
            bikes.push((id, wall));
        }
        Self {
            placed,
            bikes,
            age: 0.0,
        }
    }
}

impl TrafficEvent for Showcase {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        let car = ctx.frame.car;
        let velocity = Vec3::new(0.0, 0.0, -ctx.frame.speed.max(30.0));
        for p in &self.placed {
            if let Some(craft) = ctx.fleet.get_mut(p.id) {
                craft.position = car + p.offset;
                craft.velocity = velocity;
            }
        }
        for (i, (id, wall)) in self.bikes.iter().enumerate() {
            let slot = PATTERNS[2](self.age, i, BIKES);
            let at = car + DVec3::new(f64::from(slot.x), BIKE_Y, -f64::from(slot.z) - 10.0);
            if let Some(craft) = ctx.fleet.get_mut(*id) {
                // Le cap suit la tangente de l'ellipse : la vitesse relative à la voiture, ajoutée à la sienne.
                let next = PATTERNS[2](self.age + 0.05, i, BIKES);
                let relative = Vec3::new((next.x - slot.x) / 0.05, 0.0, -(next.z - slot.z) / 0.05);
                craft.position = at;
                craft.velocity = velocity + relative;
            }
            ctx.walls.push(*wall, at.x, at.z, ctx.frame.time);
        }
        // Les gyrophares de la police et une balise au bout du mineur : des points lumineux, à intensité constante (le
        // battement des gyrophares eux-mêmes passe par le limiteur, dans le matériau des véhicules).
        if let Some(police) = self.placed.first() {
            let at = car + police.offset;
            let (red, blue) = (LinearRgba::rgb(1.0, 0.1, 0.1), LinearRgba::rgb(0.1, 0.3, 1.0));
            ctx.glints.add(at + DVec3::new(-0.4, 1.9, 0.0), 0.9, red, 3.0);
            ctx.glints.add(at + DVec3::new(0.4, 1.9, 0.0), 0.9, blue, 3.0);
        }
        if let Some(heavy) = self.placed.get(3) {
            let at = car + heavy.offset;
            ctx.glints
                .add(at + DVec3::new(0.0, 3.4, 0.0), 1.4, LinearRgba::rgb(1.0, 0.2, 0.1), 2.5);
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        for p in self.placed.drain(..) {
            ctx.fleet.release(p.id);
        }
        for (id, wall) in self.bikes.drain(..) {
            ctx.fleet.release(id);
            ctx.walls.finish(wall);
        }
    }
}
