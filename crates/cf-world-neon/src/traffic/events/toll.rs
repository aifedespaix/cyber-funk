//! Péage lumineux (`events/toll.ts`) : une file de portiques enjambe la route. Chacun tend un rideau d'hologramme entre ses
//! montants ; quand la voiture le franchit, il vire du cyan au vert. Seule la barre du haut est un obstacle (voir
//! `PortalView.obstruction`) : en vol, la voiture passe dessous, ou dessus, comme pour les portiques néon de la route.
//!
//! Écarts (voir la PR) : les rideaux et les portiques ne sont pas rendus (la couche de rendu des portiques n'existe pas
//! encore) ; seuls les montants lumineux, en lumière continue, marquent la position des portiques. L'éclat du passage est
//! supprimé : il passerait par le limiteur de flashs, accessible côté matière seulement.

use bevy::color::LinearRgba;
use bevy::math::DVec3;

use super::common::{lerp_color, tone};
use crate::pilot::flight::Obstruction;
use crate::traffic::event::{EventCtx, TrafficEvent};

const GATES: usize = 4;
const SPACING_M: f64 = 58.0;
const FIRST_M: f64 = 280.0;
/// Hauteur de la barre du haut, m (`BAR_BOTTOM` et `BAR_TOP` de `portals.ts`).
const BAR_BOTTOM_M: f64 = 11.2;
const BAR_TOP_M: f64 = 12.3;
/// Abscisse des montants, m.
const POST_X_M: f64 = 9.8;

#[derive(Debug, Clone, Copy)]
struct Gate {
    z: f64,
    /// Instant du passage de la voiture (horloge de la scène), s.
    passed_at: Option<f64>,
}

#[derive(Debug)]
pub(crate) struct TollEvent {
    gates: Vec<Gate>,
    cold: LinearRgba,
    warm: LinearRgba,
}

impl TollEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let first = ctx.frame.car.z - FIRST_M - f64::from(super::common::range(ctx.rng, 0.0, 40.0));
        let gates = (0..GATES)
            .map(|i| Gate {
                z: first - i as f64 * SPACING_M,
                passed_at: None,
            })
            .collect();
        Self {
            gates,
            cold: tone(ctx, 1),
            warm: tone(ctx, 4),
        }
    }
}

impl TrafficEvent for TollEvent {
    fn structure(&self) -> bool {
        true
    }

    fn obstructions(&self, out: &mut Vec<Obstruction>) {
        for gate in &self.gates {
            out.push(Obstruction {
                z_near: gate.z + 0.9,
                z_far: gate.z - 0.9,
                lo: BAR_BOTTOM_M,
                hi: BAR_TOP_M,
            });
        }
    }

    fn update(&mut self, _dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        let car = ctx.frame.car;
        let Some(last) = self.gates.last() else {
            return false;
        };
        if car.z < last.z - 70.0 {
            return false;
        }
        let time = ctx.frame.time;
        for gate in &mut self.gates {
            if gate.passed_at.is_none() && car.z < gate.z {
                gate.passed_at = Some(time);
            }
            let passed = gate.passed_at.is_some();
            let colour = lerp_color(self.cold, self.warm, if passed { 1.0 } else { 0.0 });
            for side in [-POST_X_M, POST_X_M] {
                ctx.glints.add(DVec3::new(side, BAR_TOP_M, gate.z), 1.6, colour, 1.2);
            }
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.gates.clear();
    }
}
