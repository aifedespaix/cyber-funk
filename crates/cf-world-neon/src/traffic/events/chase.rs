//! Poursuite (`events/chase.ts`) : un véhicule file au-dessus de la route en slalomant, des voitures de police sur ses
//! talons, au ras de sa trajectoire. Une, deux ou trois voitures de police, tirées au hasard (45 / 35 / 20 %) ; parfois ils
//! viennent de face.
//!
//! Le fuyard et la police suivent le même chemin (`Motion::Path`), la police avec un retard : le slalom se répète derrière.
//!
//! # Ce qui fait « police »
//!
//! - **le gyrophare** : une rampe sur le toit, un halo rouge à gauche et un bleu à droite, dont l'éclat alterne en
//!   opposition de phase (la lumière « passe » d'un côté à l'autre) à 1,6 Hz, sans jamais s'éteindre ;
//! - **deux projecteurs sous la voiture**, un rouge et un bleu, qui balaient la chaussée en sens inverse et se croisent,
//!   chacun avec son rythme : on lit tout de suite une sirène.
//!
//! # Règle 7 (WCAG 2.3.1)
//!
//! Chaque couleur est une lumière continue modulée en sinus entre 45 % et 100 % de son éclat, 1,6 fois par seconde au plus
//! (sous les 3 Hz) : ce n'est pas une alternance noir/blanc. Les projecteurs ont une teinte constante ; c'est leur
//! position qui bouge. L'alternance à 2,6 Hz de Sillage, elle, n'est pas reprise.

use std::f32::consts::TAU;

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_render::SceneColor;

use super::common::{alive, index, pick_tone, pin, range, release};
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
    /// Le rouge franc des gyrophares et le bleu.
    alarm: LinearRgba,
    azure: LinearRgba,
}

/// Fréquence de rotation du gyrophare, Hz : sous les 3 flashs par seconde de la règle 7.
const BEACON_HZ: f32 = 1.6;
/// Fréquence du balayage des projecteurs, Hz.
const SWEEP_HZ: f32 = 0.55;

/// Éclat d'une couleur du gyrophare à l'instant `t` (s), de `0,45` à `1` : `phase` 0 pour le rouge, `0,5` pour le bleu.
/// Les deux sont en opposition de phase : jamais éteints ensemble, jamais au maximum ensemble.
fn beacon_level(t: f32, phase: f32) -> f32 {
    let wave = 0.5 + 0.5 * ((t * BEACON_HZ + phase) * TAU).sin();
    0.45 + 0.55 * wave * wave
}

impl ChaseEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let mut event = Self {
            crafts: Vec::with_capacity(4),
            police: Vec::with_capacity(3),
            quarry: None,
            alarm: ctx.frame.named(SceneColor::Alarm),
            azure: ctx.frame.named(SceneColor::Azure),
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

        // La police suit le même chemin avec un retard d'une seconde environ : une, deux ou trois voitures.
        let roll = ctx.rng.unit();
        let followers = if roll < 0.45 {
            1
        } else if roll < 0.8 {
            2
        } else {
            3
        };
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
        let time = ctx.frame.time as f32;
        for (i, &car_id) in self.police.iter().enumerate() {
            let Some(car) = ctx.fleet.get(car_id) else {
                continue;
            };
            let side = f64::from(car.roll.cos() * 0.3 * car.scale);
            let p = car.position;
            // Chaque voiture a son propre déphasage : les gyrophares ne battent pas tous ensemble.
            let own = i as f32 * 0.37;
            let red = beacon_level(time + own, 0.0);
            let blue = beacon_level(time + own, 0.5);
            // La rampe sur le toit : deux halos de chaque côté, un cœur plus petit et plus vif au-dessus.
            for (dx, tint, level) in [(-side, self.alarm, red), (side, self.azure, blue)] {
                ctx.glints
                    .add(DVec3::new(p.x + dx, p.y + 0.95, p.z), 4.6, tint, 1.5 * level);
                ctx.glints
                    .add(DVec3::new(p.x + dx * 0.55, p.y + 1.05, p.z), 1.6, tint, 2.6 * level);
            }

            // Deux projecteurs sous la voiture, tant qu'elle est près de la voiture : l'un rouge, l'autre bleu, qui balaient
            // la chaussée en sens inverse (ils se croisent au milieu) avec un léger décalage en profondeur.
            let car_z = ctx.frame.car.z;
            if p.z <= car_z + 60.0 && p.z >= car_z - 260.0 && p.y.abs() <= 80.0 {
                let sweep = ((time + own) * SWEEP_HZ * TAU).sin();
                let centre = p.x as f32 * 0.5;
                let reach = 6.5;
                let red_x = (centre + sweep * reach).clamp(-8.0, 8.0);
                let blue_x = (centre - sweep * reach).clamp(-8.0, 8.0);
                let sway = ((time + own) * SWEEP_HZ * TAU * 2.0).cos() * 4.0;
                pin(ctx, p, red_x, p.z - 10.0 - f64::from(sway), self.alarm);
                pin(ctx, p, blue_x, p.z - 10.0 + f64::from(sway), self.azure);
            }
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
