//! Méga-tour (`events/tower.ts`) : un immeuble démesuré dont la façade est une autoroute verticale. Cinq voies, les
//! véhicules y montent et y descendent, nez en l'air. Les dalles lumineuses de la façade filent au même sens que les voies.
//!
//! Le corps de la tour et ses rails sont des panneaux ([`PanelKind::Body`], [`PanelKind::Rails`]) reposés à chaque image.
//! Écart (voir la PR) : le corps n'a pas le matériau des façades de la ville (voir `crossroad`).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use crate::scenery::{BASE_DEPTH_M, Clearing};
use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::{index, pick_tone, range};
use crate::traffic::fleet::{CraftId, Fleet};
use crate::traffic::panels::PanelKind;
use crate::traffic::variant::HEAVY;

const LANES: usize = 5;
const LANE_SPACING_M: f32 = 13.0;
/// Épaisseur de la tour, le long de la route.
const DEPTH_M: f32 = LANES as f32 * LANE_SPACING_M + 18.0;
/// La façade côté route.
const FACE_X_M: f32 = 17.0;
const BOTTOM_M: f32 = -48.0;
const TOP_M: f32 = 330.0;
const WIDTH_M: f32 = 46.0;
/// La tour finit quand la voiture l'a dépassée de cette distance, m.
const PAST_M: f64 = 190.0;
const MAX_CRAFTS: usize = 96;

#[derive(Debug, Clone, Copy)]
struct Flow {
    z: f64,
    /// Vers le haut (1) ou vers le bas (−1).
    dir: f32,
    next: f32,
}

#[derive(Debug)]
pub(crate) struct TowerEvent {
    crafts: Vec<CraftId>,
    flows: [Flow; LANES],
    side: f32,
    z: f64,
    tint: LinearRgba,
}

impl TowerEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let z = ctx.frame.car.z - 400.0;
        let tint = pick_tone(ctx);
        let mut event = Self {
            crafts: Vec::with_capacity(MAX_CRAFTS),
            flows: std::array::from_fn(|i| Flow {
                z: z + f64::from((i as f32 + 0.5 - LANES as f32 / 2.0) * LANE_SPACING_M),
                dir: if i % 2 == 0 { 1.0 } else { -1.0 },
                next: 0.0,
            }),
            side,
            z,
            tint,
        };
        for flow in &mut event.flows {
            flow.next = range(ctx.rng, 0.2, 2.0);
        }
        // Les voies sont déjà pleines à l'apparition.
        for lane in 0..LANES {
            let flow = event.flows[lane];
            let mut y = BOTTOM_M + 10.0;
            while y < TOP_M - 10.0 {
                event.launch(ctx, flow, y);
                y += range(ctx.rng, 24.0, 60.0);
            }
        }
        event
    }

    fn launch(&mut self, ctx: &mut EventCtx<'_>, flow: Flow, y: f32) {
        if self.crafts.len() >= MAX_CRAFTS {
            return;
        }
        let heavy = ctx.rng.unit() < 0.1;
        let variant = if heavy {
            HEAVY[index(ctx.rng, HEAVY.len())]
        } else {
            Fleet::sleek(ctx.rng.unit())
        };
        let calm = if ctx.frame.calm { 0.6 } else { 1.0 };
        let speed = if heavy {
            range(ctx.rng, 14.0, 24.0)
        } else {
            range(ctx.rng, 20.0, 50.0)
        } * calm;
        let position = DVec3::new(
            f64::from(self.side * (FACE_X_M - 1.6)),
            f64::from(y),
            flow.z + f64::from(range(ctx.rng, -0.8, 0.8)),
        );
        let tint = pick_tone(ctx);
        let scale = if heavy { 2.4 } else { range(ctx.rng, 1.5, 1.9) };
        let Some(id) = ctx.fleet.spawn(
            variant,
            position,
            Vec3::new(0.0, flow.dir * speed, 0.0),
            tint,
            scale,
            ctx.rng,
        ) else {
            return;
        };
        if let Some(craft) = ctx.fleet.get_mut(id) {
            // Le haut du véhicule regarde la route : il roule sur la façade, nez en l'air ou en bas.
            craft.up = Vec3::new(-self.side, 0.0, 0.0);
            craft.trail = 0.9;
        }
        self.crafts.push(id);
    }

    /// Pose le corps de la tour et ses rails.
    fn draw(&self, ctx: &mut EventCtx<'_>) {
        let pulse = ctx.scene.fx.kick;
        let panels = &mut ctx.scene.panels;
        let base = -f64::from(BASE_DEPTH_M);
        panels.block(
            PanelKind::Body,
            PanelKind::Body,
            DVec3::new(f64::from(self.side * (FACE_X_M + WIDTH_M / 2.0)), base, self.z),
            Vec3::new(WIDTH_M, BASE_DEPTH_M + TOP_M + 40.0, DEPTH_M),
            false,
            self.tint,
            1.0,
            0.777,
        );
        // Les rails : un plan vertical collé à la façade, côté route.
        panels.quad(
            PanelKind::Rails,
            DVec3::new(
                f64::from(self.side * (FACE_X_M - 0.15)),
                f64::from((TOP_M + BOTTOM_M) * 0.5),
                self.z,
            ),
            Vec3::new(0.0, 0.0, self.side * DEPTH_M * 0.5),
            Vec3::new(0.0, (TOP_M - BOTTOM_M) * 0.5, 0.0),
            LinearRgba::rgb(pulse, 0.0, 0.0),
            1.0,
            0.0,
        );
    }
}

impl TrafficEvent for TowerEvent {
    fn structure(&self) -> bool {
        true
    }

    fn clearing(&self) -> Option<Clearing> {
        Some(Clearing {
            z0: self.z - f64::from(DEPTH_M) / 2.0 - 14.0,
            z1: self.z + f64::from(DEPTH_M) / 2.0 + 14.0,
            side: if self.side > 0.0 { 1 } else { -1 },
        })
    }

    /// Un point de la façade et la hauteur visible de la tour : pour les plans du mode cinéma.
    fn focus(&self) -> Option<Focus> {
        Some(Focus {
            position: DVec3::new(f64::from(self.side * FACE_X_M), 90.0, self.z),
            size: 180.0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        if self.z > ctx.frame.car.z + PAST_M {
            return false;
        }
        for lane in 0..LANES {
            self.flows[lane].next -= dt;
            if self.flows[lane].next > 0.0 {
                continue;
            }
            let flow = self.flows[lane];
            self.flows[lane].next = range(ctx.rng, 0.9, 2.4) / (0.7 + ctx.frame.beat.mid * 0.8);
            self.launch(ctx, flow, if flow.dir > 0.0 { BOTTOM_M } else { TOP_M - 20.0 });
        }
        self.crafts.retain(|&id| {
            let inside = ctx.fleet.get(id).is_some_and(|c| {
                let y = c.position.y as f32;
                (BOTTOM_M - 4.0..=TOP_M).contains(&y)
            });
            if !inside {
                ctx.fleet.release(id);
            }
            inside
        });
        self.draw(ctx);
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        for id in self.crafts.drain(..) {
            ctx.fleet.release(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn les_voies_sont_pleines_des_le_depart_et_les_vehicules_montent_nez_en_l_air() {
        let mut rig = Rig::new(2);
        let mut event = rig.start(TowerEvent::new);
        assert!(event.crafts.len() >= 15, "{} véhicules", event.crafts.len());
        for _ in 0..120 {
            assert!(rig.step(&mut event, 1.0 / 60.0, |_| {}));
        }
        let mut up = 0;
        let mut down = 0;
        for &id in &event.crafts {
            let craft = rig.fleet.get(id).expect("vivant");
            assert!((craft.up.x + event.side).abs() < 1e-6, "le haut regarde la route");
            if craft.velocity.y > 0.0 {
                up += 1;
            } else {
                down += 1;
            }
        }
        assert!(up > 0 && down > 0, "deux sens : {up} / {down}");
        // Corps (5 faces) ; les rails sont de la lumière.
        assert_eq!(rig.scene.panels.solid().len(), 5);
        assert_eq!(rig.scene.panels.light().len(), 1);
    }

    #[test]
    fn la_zone_dégagée_est_du_cote_de_la_tour() {
        let mut rig = Rig::new(2);
        let event = rig.start(TowerEvent::new);
        let clearing = event.clearing().expect("zone");
        assert_eq!(f32::from(clearing.side), event.side);
        assert!(clearing.z1 - clearing.z0 > f64::from(DEPTH_M));
    }
}
