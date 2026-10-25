//! Course de drones (`events/race.ts`) : une meute de cinq engins file sous une série de portiques qui s'allument à leur
//! passage, en serpentant d'un bord à l'autre de la chaussée. Ce sont de vrais véhicules de la flotte : leurs tranches
//! d'altitude sont réservées à la voiture volante (elle n'y monte pas), et l'événement ne démarre que si elle est au sol.
//! Les barres des portiques sont des obstacles pour elle (elle passe dessous ou dessus), les rideaux d'hologramme ne le
//! sont pas.
//!
//! Écart (voir la PR) : l'allumage d'un portique (le rideau passe de 0 à 1,7) monte en 0,12 s au lieu de claquer, et son
//! éclat de passage se règle sur le pouls que le limiteur laisse passer (`SceneFx::kick`).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_render::SceneColor;

use super::portal;
use crate::pilot::flight::Obstruction;
use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::{alive, pick_tone, range, release, set_pose, spawn_scripted};
use crate::traffic::fleet::{CraftId, Fleet};

const RACERS: usize = 5;
const GATES: usize = 5;
const SPACING_M: f64 = 82.0;
const FIRST_M: f64 = 230.0;
const LIFE_S: f32 = 18.0;
/// Les coureurs filent entre 4,5 et 9 m : sous la barre des portiques (11,2 m) et au-dessus d'une voiture au sol.
const LOW_M: f32 = 5.2;
const SWING_M: f32 = 2.1;
/// Le rideau d'un portique qui vient de s'allumer monte en lumière en 0,12 s.
const LIGHT_UP_S: f64 = 0.12;

#[derive(Debug, Clone, Copy)]
struct Gate {
    z: f64,
    /// Instant du premier passage d'un coureur : le portique s'allume pour la meute.
    lit_at: Option<f64>,
}

#[derive(Debug, Clone, Copy)]
struct Racer {
    id: CraftId,
    lane: f32,
    lead: f32,
    phase: f32,
}

#[derive(Debug)]
pub(crate) struct RaceEvent {
    gates: Vec<Gate>,
    racers: Vec<Racer>,
    speed: f32,
    start: f64,
    age: f32,
    time: f32,
    lead: DVec3,
    cold: LinearRgba,
    hot: LinearRgba,
}

impl RaceEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let speed = ctx.frame.speed * 1.08 + 4.0;
        let first = ctx.frame.car.z - FIRST_M - f64::from(range(ctx.rng, 0.0, 30.0));
        let gates = (0..GATES)
            .map(|i| Gate {
                z: first - i as f64 * SPACING_M,
                lit_at: None,
            })
            .collect();
        let start = ctx.frame.car.z - 90.0;
        let mut racers = Vec::with_capacity(RACERS);
        for i in 0..RACERS {
            let origin = DVec3::new(
                f64::from((i as f32 - 2.0) * 2.4),
                f64::from(LOW_M + 1.0),
                start + f64::from(range(ctx.rng, -20.0, 20.0)),
            );
            let tint = pick_tone(ctx);
            let variant = Fleet::sleek(ctx.rng.unit());
            let Some(id) = spawn_scripted(ctx, variant, origin, Vec3::new(0.0, 0.0, -speed), tint, 1.9) else {
                continue;
            };
            if let Some(craft) = ctx.fleet.get_mut(id) {
                craft.trail = 1.5;
            }
            racers.push(Racer {
                id,
                lane: i as f32,
                lead: range(ctx.rng, -14.0, 14.0) + (RACERS - i) as f32 * 3.0,
                phase: range(ctx.rng, 0.0, std::f32::consts::TAU),
            });
        }
        Self {
            gates,
            racers,
            speed,
            start,
            age: 0.0,
            time: 0.0,
            lead: DVec3::new(0.0, f64::from(LOW_M), start),
            cold: ctx.frame.named(SceneColor::Azure),
            hot: ctx.frame.named(SceneColor::Rose),
        }
    }

    /// La pose d'un coureur au temps `t` de la course : un slalom plein de chaussée, déphasé d'un coureur à l'autre, sans
    /// jamais dépasser les montants (±9,8).
    fn pose(&self, racer: &Racer, t: f32) -> (DVec3, Vec3) {
        let swerve = t * 0.85 + racer.phase + racer.lane * 0.9;
        let x = swerve.sin() * 5.6;
        let y = LOW_M + 1.6 + (t * 1.3 + racer.phase).sin() * SWING_M;
        let z = self.start + f64::from(racer.lead) - f64::from(self.speed * t);
        let velocity = Vec3::new(
            swerve.cos() * 5.6 * 0.85,
            (t * 1.3 + racer.phase).cos() * SWING_M * 1.3,
            -self.speed,
        );
        (DVec3::new(f64::from(x), f64::from(y), z), velocity)
    }
}

impl TrafficEvent for RaceEvent {
    fn structure(&self) -> bool {
        true
    }

    fn proximity(&self, ctx: &EventCtx<'_>) -> bool {
        let car_z = ctx.frame.car.z;
        self.racers.iter().any(|r| {
            ctx.fleet
                .get(r.id)
                .is_some_and(|c| c.position.z < car_z + 70.0 && c.position.z > car_z - 230.0)
        })
    }

    fn obstructions(&self, out: &mut Vec<Obstruction>) {
        out.extend(self.gates.iter().map(|g| portal::obstruction(g.z)));
    }

    /// Centre de la meute et son emprise : pour les plans du mode cinéma.
    fn focus(&self) -> Option<Focus> {
        Some(Focus {
            position: self.lead,
            size: 22.0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        self.time += dt;
        let Some(last) = self.gates.last().map(|g| g.z) else {
            return false;
        };
        // Les coureurs sont scénarisés : la pose est écrite avant la mise à jour de la flotte.
        let mut front = f64::INFINITY;
        let mut living = 0;
        for i in 0..self.racers.len() {
            let racer = self.racers[i];
            if !alive(ctx, Some(racer.id)) {
                continue;
            }
            let (position, velocity) = self.pose(&racer, self.time);
            set_pose(ctx, racer.id, position, velocity);
            if living == 0 {
                self.lead = position;
            }
            living += 1;
            front = front.min(position.z);
        }
        if living == 0 || self.age > LIFE_S || ctx.frame.car.z < last - 90.0 {
            return false;
        }
        let pulse = ctx.scene.fx.kick;
        let time = ctx.frame.time;
        for gate in &mut self.gates {
            if gate.lit_at.is_none() && front < gate.z {
                gate.lit_at = Some(time);
            }
            let (colour, curtain) = match gate.lit_at {
                Some(at) => {
                    let since = time - at;
                    let ramp = (since / LIGHT_UP_S).min(1.0) as f32;
                    (self.hot, (0.5 + (1.2 - since as f32).max(0.0) + pulse * 0.5) * ramp)
                }
                None => (self.cold, 0.0),
            };
            portal::draw(ctx.scene, gate.z, colour, curtain);
        }
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        for racer in self.racers.drain(..) {
            release(ctx, Some(racer.id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn la_meute_serpente_sous_les_montants_et_allume_les_portiques_un_a_un() {
        let mut rig = Rig::new(12);
        let mut event = rig.start(RaceEvent::new);
        assert_eq!(event.racers.len(), RACERS);
        let mut lit = 0;
        for _ in 0..(LIFE_S as usize * 60) {
            if !rig.step(&mut event, 1.0 / 60.0, |_| {}) {
                break;
            }
            for r in &event.racers {
                let c = rig.fleet.get(r.id).expect("vivant");
                assert!(c.position.x.abs() <= 5.6 + 1e-6 && c.position.x.abs() < f64::from(portal::POST_X_M));
                // Entre 4,5 et 9 m : sous la barre des portiques, au-dessus d'une voiture au sol.
                assert!((4.5..9.0).contains(&c.position.y) && c.position.y < f64::from(portal::BAR_BOTTOM_M));
            }
            lit = lit.max(event.gates.iter().filter(|g| g.lit_at.is_some()).count());
        }
        assert!(lit >= 2, "les portiques s'allument au passage : {lit}");
        let mut obstacles = Vec::new();
        event.obstructions(&mut obstacles);
        assert_eq!(obstacles.len(), GATES);
        rig.end(&mut event);
        assert_eq!(rig.active_crafts(), 0);
    }

    #[test]
    fn un_portique_ne_s_allume_jamais_d_un_coup() {
        let mut rig = Rig::new(12);
        let mut event = rig.start(RaceEvent::new);
        let mut previous = 0.0_f32;
        let mut max_jump = 0.0_f32;
        for _ in 0..(LIFE_S as usize * 60) {
            if !rig.step(&mut event, 1.0 / 60.0, |_| {}) {
                break;
            }
            // L'éclat du premier rideau tracé (le portique 0) d'une image à l'autre.
            let curtain = rig.scene.panels.light().first().map_or(0.0, |p| p.color[0]);
            max_jump = max_jump.max((curtain - previous).abs());
            previous = curtain;
        }
        // 1,7 au plus : à 60 i/s, en 0,12 s, soit 7 images : jamais plus de 0,3 par image.
        assert!(max_jump < 0.35, "saut de {max_jump}");
    }
}
