//! Péage lumineux (`events/toll.ts`) : une file de portiques enjambe la route. Chacun tend un rideau d'hologramme entre ses
//! montants ; quand la voiture le franchit, il bugue autour d'elle (elle le traverse : c'est de la lumière) et vire du cyan
//! au vert avec un éclat. Seule la barre du haut est un obstacle (voir [`portal`]) : en vol, la voiture passe dessous, ou
//! dessus, comme pour les portiques néon de la route.
//!
//! Les portiques et leurs rideaux sont dessinés par les panneaux de la scène (J7-05, voir [`portal`]). Écart (voir la PR) :
//! règle 7, l'éclat du passage n'est plus un saut du rideau à 2,6 fois son éclat, c'est l'éclat que le limiteur de flashs
//! accorde (`SceneFx::burst`) à la demande du passage ; le pouls du rideau en attente est le pouls accordé.

use bevy::color::LinearRgba;
use cf_render::SceneColor;

use super::common::{lerp_color, range};
use crate::pilot::flight::Obstruction;
use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::grand::portal;

const GATES: usize = 4;
const SPACING_M: f64 = 58.0;
const FIRST_M: f64 = 280.0;
/// L'éclat de passage retombe en une seconde (`FLASH`), et pèse 1,5 fois l'éclat accordé.
const FLASH_S: f64 = 1.0;
const FLASH_GAIN: f32 = 1.5;
/// Le rideau passe du cyan au vert (et de l'attente à l'éclat de passage) en 0,12 s.
const SWITCH_S: f64 = 0.12;
/// Un portique franchi s'éteint peu à peu derrière la voiture, sans jamais descendre sous ce plancher.
const FLOOR: f32 = 0.15;

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
        let first = ctx.frame.car.z - FIRST_M - f64::from(range(ctx.rng, 0.0, 40.0));
        let gates = (0..GATES)
            .map(|i| Gate {
                z: first - i as f64 * SPACING_M,
                passed_at: None,
            })
            .collect();
        Self {
            gates,
            cold: ctx.frame.named(SceneColor::Azure),
            warm: ctx.frame.named(SceneColor::Lime),
        }
    }
}

impl TrafficEvent for TollEvent {
    fn structure(&self) -> bool {
        true
    }

    fn obstructions(&self, out: &mut Vec<Obstruction>) {
        out.extend(self.gates.iter().map(|g| portal::obstruction(g.z)));
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
        let pulse = ctx.scene.fx.kick;
        // L'éclat accordé par le limiteur à l'image précédente, en 0..1.
        let granted = ctx.scene.fx.burst;
        for gate in &mut self.gates {
            if gate.passed_at.is_none() && car.z < gate.z {
                gate.passed_at = Some(time);
                // Le passage demande un éclat au limiteur : il n'éclate que s'il l'accorde.
                ctx.scene.fx.burst_request = true;
            }
            // Cyan en attente, vert une fois franchi ; le rideau s'éteint peu à peu derrière la voiture. Le passage fond en
            // 0,12 s (le rideau ne saute pas d'un éclat à l'autre).
            let waiting = 0.8 + pulse * 0.4;
            let (colour, curtain) = match gate.passed_at {
                Some(at) => {
                    let since = time - at;
                    let blend = (since / SWITCH_S).min(1.0) as f32;
                    let flash = (1.0 - since / FLASH_S).max(0.0) as f32 * granted * FLASH_GAIN;
                    let passed = (1.1 - since as f32 * 0.5).max(FLOOR) + flash;
                    (
                        lerp_color(self.cold, self.warm, blend),
                        waiting + (passed - waiting) * blend,
                    )
                }
                None => (self.cold, waiting),
            };
            portal::draw(ctx.scene, gate.z, colour, curtain);
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.gates.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn les_portiques_sont_des_obstacles_et_virent_au_vert_quand_la_voiture_passe() {
        let mut rig = Rig::new(14);
        let mut event = rig.start(TollEvent::new);
        let mut obstacles = Vec::new();
        event.obstructions(&mut obstacles);
        assert_eq!(obstacles.len(), GATES);
        let mut requests = 0;
        let mut passed = 0;
        while rig.step(&mut event, 1.0 / 60.0, |_| {}) {
            requests += usize::from(rig.scene.fx.burst_request);
            passed = event.gates.iter().filter(|g| g.passed_at.is_some()).count();
            // Chaque portique : deux montants et la barre (15 faces), plus son rideau (lumière).
            assert_eq!(rig.scene.panels.solid().len(), GATES * 15);
            assert_eq!(rig.scene.panels.light().len(), GATES);
        }
        assert_eq!(passed, GATES, "la voiture a franchi les quatre portiques");
        assert_eq!(requests, GATES, "chaque passage demande un éclat, une seule fois");
    }

    #[test]
    fn sans_eclat_accorde_le_rideau_ne_saute_pas_au_passage() {
        let mut rig = Rig::new(14);
        let mut event = rig.start(TollEvent::new);
        let mut previous: Option<f32> = None;
        let mut worst = 0.0_f32;
        while rig.step(&mut event, 1.0 / 60.0, |_| {}) {
            // Le dernier rideau tracé : celui du portique le plus lointain (jamais encore franchi pendant longtemps).
            let first = rig.scene.panels.light().first().map(|p| p.color[0]);
            if let (Some(a), Some(b)) = (previous, first) {
                worst = worst.max((b - a).abs());
            }
            previous = first;
        }
        // Pas de limiteur dans le banc : le rideau fond en 0,12 s au passage (au plus 0,3 d'éclat en 7 images).
        assert!(worst < 0.08, "saut de {worst}");
    }
}
