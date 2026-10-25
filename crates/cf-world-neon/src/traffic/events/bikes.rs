//! Bande de motos lumineuses (`events/bikes.ts`) : une à cinq arrivent par l'arrière dans une voie latérale, se glissent
//! devant la voiture, enchaînent deux ou trois figures (slalom, tresse, manège, V, huit) puis filent devant, certaines en
//! wheeling. Chaque moto laisse un mur de lumière (`LightWalls`).
//!
//! Tout est calculé dans le repère de la voiture (latéral, distance devant) : les figures restent à plus de 14 m devant,
//! et pendant l'arrivée chaque moto reste dans sa voie latérale jusqu'à ce qu'elle ait dépassé la voiture. Un dernier
//! garde-fou repousse toute moto qui serait trop près de la voiture.

use bevy::math::{DVec3, Vec3};

use super::common::{alive, index, range, release, smooth, spawn_scripted, tone};
use crate::traffic::bike_patterns::{PATTERNS, Pattern, Slot};
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::fleet::CraftId;
use crate::traffic::model::bike::WHEEL;
use crate::traffic::variant::BIKE;
use crate::traffic::walls::WallId;

const SCALE: f32 = 1.3;
const ARRIVE_S: f32 = 4.0;
/// Les motos arrivent par une voie latérale : à 7,6 m de l'axe, la plus proche est à 3 m de la voiture même au bord (4,6 m).
const LANE_M: f32 = 7.6;
/// Marge minimale entre l'axe d'une moto et celui de la voiture quand elles sont à hauteur l'une de l'autre.
const GAP_M: f32 = 2.6;
const BLEND_S: f32 = 1.2;
const LEAVE_ACCEL: f32 = 34.0;
const LEAVE_DISTANCE_M: f32 = 340.0;
const WHEELIE: f32 = 0.95;

#[derive(Debug, Clone, Copy)]
struct Bike {
    craft: CraftId,
    /// Voie d'arrivée, -1 ou 1 : elle ne change jamais, donc la moto ne passe jamais de l'autre côté de la voiture.
    side: f32,
    start_z: f32,
    leave_at: f32,
    wheelie: bool,
    placed: bool,
    wall: Option<WallId>,
}

#[derive(Debug)]
pub(crate) struct BikesEvent {
    bikes: Vec<Bike>,
    patterns: Vec<Pattern>,
    durations: Vec<f32>,
    time: f32,
}

impl BikesEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        const COUNTS: [usize; 8] = [1, 2, 3, 3, 4, 4, 5, 5];
        let count = COUNTS[index(ctx.rng, COUNTS.len())];
        let figure_count = 2 + index(ctx.rng, 2);
        let mut patterns = Vec::with_capacity(figure_count);
        let mut durations = Vec::with_capacity(figure_count);
        let mut previous = usize::MAX;
        let mut figures = 0.0;
        for _ in 0..figure_count {
            let mut k = index(ctx.rng, PATTERNS.len());
            if k == previous {
                k = (k + 1) % PATTERNS.len();
            }
            previous = k;
            patterns.push(PATTERNS[k]);
            let duration = range(ctx.rng, 5.0, 7.0);
            durations.push(duration);
            figures += duration;
        }
        let pursuit = figures + ARRIVE_S;

        let lead = if ctx.frame.car.x > 0.0 { -1.0 } else { 1.0 };
        let warm = if ctx.rng.coin() { tone(ctx, 1) } else { tone(ctx, 4) };
        let amber = tone(ctx, 0);
        let car = ctx.frame.car;
        let mut bikes = Vec::with_capacity(count);
        for i in 0..count {
            let start_z = -(70.0 + i as f32 * 14.0);
            let side = if count > 1 && i % 2 == 1 { -lead } else { lead };
            let tint = if i % 2 == 0 { warm } else { amber };
            let position = DVec3::new(
                f64::from(side * LANE_M),
                f64::from(WHEEL * SCALE),
                car.z - f64::from(start_z),
            );
            let velocity = Vec3::new(0.0, 0.0, -ctx.frame.speed);
            let Some(craft) = spawn_scripted(ctx, BIKE, position, velocity, tint, SCALE) else {
                continue;
            };
            let wheelie = ctx.rng.unit() < 0.55;
            let wall = ctx.walls.claim(tint);
            if let Some(c) = ctx.fleet.get_mut(craft) {
                c.lean = 12.0;
                // Le mur de lumière remplace la traînée en croix : il reste dans le monde derrière la moto.
                c.trail = 0.0;
            }
            bikes.push(Bike {
                craft,
                side,
                start_z,
                leave_at: pursuit + i as f32 * 0.35,
                wheelie,
                placed: false,
                wall,
            });
        }
        Self {
            bikes,
            patterns,
            durations,
            time: 0.0,
        }
    }

    /// Position de la moto de rang `rank` sur `total` dans le repère de la voiture, selon la phase du spectacle.
    fn place(&self, bike: &Bike, rank: usize, total: usize) -> Slot {
        let now = self.time;
        if now < ARRIVE_S {
            // Arrivée : la distance se rattrape vite, la position latérale ne quitte la voie qu'une fois la voiture dépassée.
            let progress = now / ARRIVE_S;
            let mut out = (self.patterns[0])(now, rank, total);
            out.z = mix(bike.start_z, out.z, 1.0 - (1.0 - progress).powi(3));
            out.x = mix(bike.side * LANE_M, out.x, smooth(0.6, 1.0, progress));
            return out;
        }
        let local = now - ARRIVE_S;
        let mut figure = 0;
        let mut start = 0.0;
        while figure < self.durations.len() - 1 && local >= start + self.durations[figure] {
            start += self.durations[figure];
            figure += 1;
        }
        let mut out = (self.patterns[figure])(now, rank, total);
        if figure > 0 && local - start < BLEND_S {
            let previous = (self.patterns[figure - 1])(now, rank, total);
            let s = smooth(0.0, BLEND_S, local - start);
            out.x = mix(previous.x, out.x, s);
            out.z = mix(previous.z, out.z, s);
        }
        let away = now - bike.leave_at;
        if away > 0.0 {
            out.z += 0.5 * LEAVE_ACCEL * away * away;
        }
        out
    }
}

fn mix(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

impl TrafficEvent for BikesEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        if !self.bikes.iter().any(|b| alive(ctx, Some(b.craft))) {
            return false;
        }
        self.time += dt;
        let n = self.bikes.len();
        let time = self.time;
        for i in 0..n {
            let bike = self.bikes[i];
            if !alive(ctx, Some(bike.craft)) {
                continue;
            }
            let mut slot = self.place(&bike, i, n);

            // Garde-fou : à hauteur de la voiture (fenêtre de -8 à +4 m, arrondie sur 4 m), jamais plus près que GAP.
            let near = smooth(-12.0, -8.0, slot.z) * (1.0 - smooth(4.0, 8.0, slot.z));
            if near > 0.0 {
                let wanted = ctx.frame.car.x as f32 + bike.side * GAP_M * near;
                if bike.side * slot.x < bike.side * wanted {
                    slot.x = wanted;
                }
            }

            let wave = if time > ARRIVE_S {
                ctx.frame.beat.pulse * 0.22
            } else {
                0.0
            };
            let car = ctx.frame.car;
            let position = DVec3::new(
                f64::from(slot.x),
                f64::from(WHEEL * SCALE + wave),
                car.z - f64::from(slot.z),
            );
            let dt_safe = dt > 1e-3;
            let Some(craft) = ctx.fleet.get_mut(bike.craft) else {
                continue;
            };
            let previous = craft.position;
            craft.position = position;
            craft.velocity = if bike.placed && dt_safe {
                ((position - previous) / f64::from(dt)).as_vec3()
            } else {
                Vec3::new(0.0, 0.0, -ctx.frame.speed)
            };
            let tint = craft.tint;
            // Petits wheelings en pleine figure, un grand en partant.
            let leaving = time > bike.leave_at;
            let pop = if bike.wheelie
                && time > ARRIVE_S
                && !leaving
                && ((time * 0.5 + i as f32).floor() as i64).rem_euclid(4) == 0
            {
                0.55
            } else {
                0.0
            };
            let target = if leaving && bike.wheelie { WHEELIE } else { pop };
            craft.pitch += (target - craft.pitch) * (1.0 - (-dt * 5.0).exp());
            self.bikes[i].placed = true;

            if let Some(wall) = bike.wall {
                ctx.walls.push(wall, position.x, position.z, ctx.frame.time);
            }
            ctx.glints.add(position + DVec3::new(0.0, 0.4, -1.2), 2.8, tint, 1.1);
            if slot.z > LEAVE_DISTANCE_M {
                release(ctx, Some(bike.craft));
                if let Some(wall) = bike.wall {
                    ctx.walls.finish(wall);
                }
            }
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        for bike in self.bikes.drain(..) {
            release(ctx, Some(bike.craft));
            if let Some(wall) = bike.wall {
                ctx.walls.finish(wall);
            }
        }
    }
}
