//! Route croisée surélevée (`events/crossroad.ts`) : une autre route, perpendiculaire, passe au-dessus ou au-dessous de la
//! nôtre et la ville s'écarte le temps de la traverser. Quatre files, deux sens, des véhicules de toutes sortes ; la route
//! est déjà pleine quand elle apparaît.
//!
//! La dalle et ses six piliers sont des panneaux ([`PanelKind::SlabTop`], [`PanelKind::SlabSide`], [`PanelKind::Body`])
//! reposés à chaque image : pas d'entité, pas de maillage propre à l'événement. Écart (voir la PR) : les piliers ne
//! prennent pas le matériau des façades (fenêtres et brouillard de la ville) mais celui des panneaux (encre, fenêtres
//! hachées, arêtes néon à leur teinte).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};
use cf_render::SceneColor;

use crate::pilot::flight::Obstruction;
use crate::scenery::{BASE_DEPTH_M, Clearing};
use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::{index, pick_tone, range};
use crate::traffic::fleet::{CraftId, Fleet};
use crate::traffic::panels::PanelKind;
use crate::traffic::variant::HEAVY;

/// La route croisée s'étend sur ±380 m.
const HALF_LENGTH_M: f32 = 380.0;
const WIDTH_M: f32 = 19.0;
const SLAB_M: f32 = 1.6;
/// Décalages en z des quatre files : les deux premières vont vers +x.
const LANE_Z_M: [f32; 4] = [-6.0, -2.2, 2.2, 6.0];
const OVER_Y_M: f32 = 19.0;
const UNDER_Y_M: f32 = -22.0;
const PILLARS_X_M: [f32; 6] = [-250.0, -140.0, -60.0, 60.0, 140.0, 250.0];
/// Une route croisée finit quand la voiture l'a dépassée de cette distance, m.
const PAST_M: f64 = 190.0;
/// Les lampadaires ne sont posés que dans le champ proche, m.
const LAMP_RANGE_M: f64 = 330.0;
const MAX_CRAFTS: usize = 128;

#[derive(Debug, Clone, Copy)]
struct Flow {
    /// Dans le sens de +x (1) ou de −x (−1).
    dir: f32,
    lane: usize,
    next: f32,
}

#[derive(Debug)]
pub(crate) struct CrossRoadEvent {
    crafts: Vec<CraftId>,
    flows: [Flow; 4],
    z: f64,
    y: f32,
    tint: LinearRgba,
    ice: LinearRgba,
}

impl CrossRoadEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let y = if ctx.rng.unit() < 0.55 { OVER_Y_M } else { UNDER_Y_M };
        let mut event = Self {
            crafts: Vec::with_capacity(MAX_CRAFTS),
            flows: std::array::from_fn(|lane| Flow {
                dir: if lane < 2 { 1.0 } else { -1.0 },
                lane,
                next: 0.0,
            }),
            z: ctx.frame.car.z - 390.0,
            y,
            tint: pick_tone(ctx),
            ice: ctx.frame.named(SceneColor::Ice),
        };
        for flow in &mut event.flows {
            flow.next = range(ctx.rng, 0.2, 1.6);
        }
        // Route déjà vivante : on la peuple sur toute sa longueur.
        for lane in 0..event.flows.len() {
            let flow = event.flows[lane];
            let mut x = -HALF_LENGTH_M + range(ctx.rng, 0.0, 40.0);
            while x < HALF_LENGTH_M {
                event.launch(ctx, flow, x);
                x += range(ctx.rng, 32.0, 95.0);
            }
        }
        event
    }

    fn launch(&mut self, ctx: &mut EventCtx<'_>, flow: Flow, x: f32) {
        if self.crafts.len() >= MAX_CRAFTS {
            return;
        }
        let heavy = ctx.rng.unit() < 0.12;
        let variant = if heavy {
            HEAVY[index(ctx.rng, HEAVY.len())]
        } else {
            Fleet::sleek(ctx.rng.unit())
        };
        let calm = if ctx.frame.calm { 0.6 } else { 1.0 };
        let speed = if heavy {
            range(ctx.rng, 16.0, 28.0)
        } else {
            range(ctx.rng, 26.0, 64.0)
        } * calm;
        let position = DVec3::new(
            f64::from(x),
            f64::from(self.y + 0.7 + range(ctx.rng, 0.0, 0.25)),
            self.z + f64::from(LANE_Z_M[flow.lane] + range(ctx.rng, -0.4, 0.4)),
        );
        let tint = pick_tone(ctx);
        let scale = if heavy { 2.4 } else { range(ctx.rng, 1.6, 2.0) };
        let Some(id) = ctx.fleet.spawn(
            variant,
            position,
            Vec3::new(flow.dir * speed, 0.0, 0.0),
            tint,
            scale,
            ctx.rng,
        ) else {
            return;
        };
        if let Some(craft) = ctx.fleet.get_mut(id) {
            craft.trail = 0.7;
        }
        self.crafts.push(id);
    }

    /// Pose la dalle et les piliers de l'image.
    fn draw(&self, ctx: &mut EventCtx<'_>) {
        let y = f64::from(self.y);
        let panels = &mut ctx.scene.panels;
        let pulse = ctx.scene.fx.kick;
        // Dalle : la couleur ne sert que de réservoir à `pulse` (éclat des bords).
        panels.block(
            PanelKind::SlabSide,
            PanelKind::SlabTop,
            DVec3::new(0.0, y - f64::from(SLAB_M), self.z),
            Vec3::new(HALF_LENGTH_M * 2.0, SLAB_M, WIDTH_M),
            true,
            LinearRgba::rgb(pulse, 0.0, 0.0),
            1.0,
            0.0,
        );
        let base = -f64::from(BASE_DEPTH_M);
        let height = BASE_DEPTH_M + self.y - SLAB_M;
        for (i, x) in PILLARS_X_M.into_iter().enumerate() {
            let seed = ((40.0 + i as f32 * 13.0) * 0.0137).fract();
            panels.block(
                PanelKind::Body,
                PanelKind::Body,
                DVec3::new(f64::from(x), base, self.z),
                Vec3::new(7.0, height, 12.0),
                false,
                self.tint,
                1.0,
                seed,
            );
        }
    }
}

impl TrafficEvent for CrossRoadEvent {
    fn structure(&self) -> bool {
        true
    }

    fn clearing(&self) -> Option<Clearing> {
        Some(Clearing {
            z0: self.z - f64::from(WIDTH_M) / 2.0 - 12.0,
            z1: self.z + f64::from(WIDTH_M) / 2.0 + 12.0,
            side: 0,
        })
    }

    /// La dalle surélevée (et les véhicules qui y roulent) coupe la route en altitude ; sous le sol, elle ne gêne personne.
    fn obstruction(&self) -> Option<Obstruction> {
        (self.y >= 0.0).then(|| Obstruction {
            z_near: self.z + f64::from(WIDTH_M) / 2.0,
            z_far: self.z - f64::from(WIDTH_M) / 2.0,
            lo: f64::from(self.y - SLAB_M),
            hi: f64::from(self.y + 2.6),
        })
    }

    /// Le croisement et sa largeur : pour les plans du mode cinéma.
    fn focus(&self) -> Option<Focus> {
        Some(Focus {
            position: DVec3::new(0.0, f64::from(self.y), self.z),
            size: 60.0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        // Terminée quand la voiture l'a dépassée de loin.
        if self.z > ctx.frame.car.z + PAST_M {
            return false;
        }
        for lane in 0..self.flows.len() {
            self.flows[lane].next -= dt;
            if self.flows[lane].next > 0.0 {
                continue;
            }
            let flow = self.flows[lane];
            self.flows[lane].next = range(ctx.rng, 0.45, 1.5) / (0.7 + ctx.frame.beat.bass * 0.8);
            self.launch(ctx, flow, -flow.dir * (HALF_LENGTH_M - 6.0));
        }
        // Un véhicule sorti de la dalle (ou recyclé par la flotte) est rendu.
        self.crafts.retain(|&id| {
            let on_slab = ctx
                .fleet
                .get(id)
                .is_some_and(|c| c.position.x.abs() <= f64::from(HALF_LENGTH_M) + 4.0);
            if !on_slab {
                ctx.fleet.release(id);
            }
            on_slab
        });

        // Lampadaires : une haleine de lumière tous les 22 m de chaque bord, uniquement dans le champ proche.
        if (self.z - ctx.frame.car.z).abs() < LAMP_RANGE_M {
            for k in -8..=8 {
                let x = f64::from(k as f32 * 22.0);
                for edge in [-1.0_f32, 1.0] {
                    ctx.glints.add(
                        DVec3::new(
                            x,
                            f64::from(self.y + 3.2),
                            self.z + f64::from(edge * (WIDTH_M / 2.0 - 0.4)),
                        ),
                        2.4,
                        self.ice,
                        0.7,
                    );
                }
            }
        }
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
    fn la_route_est_peuplee_des_l_apparition_et_chaque_vehicule_reste_sur_la_dalle() {
        let mut rig = Rig::new(10);
        let mut event = rig.start(CrossRoadEvent::new);
        assert!(event.crafts.len() >= 20, "{} véhicules", event.crafts.len());
        assert!(event.structure());
        let clearing = event.clearing().expect("une zone sans immeuble");
        assert!(clearing.z0 < event.z && clearing.z1 > event.z);
        // Les véhicules suivent une file : leur z est celui d'une des quatre files.
        for &id in &event.crafts {
            let craft = rig.fleet.get(id).expect("vivant");
            let dz = (craft.position.z - event.z) as f32;
            assert!(LANE_Z_M.iter().any(|l| (dz - l).abs() <= 0.45), "{dz}");
        }
        for _ in 0..600 {
            assert!(rig.step(&mut event, 1.0 / 60.0, |_| {}));
        }
        // Dalle (4 côtés, dessus, dessous) et six piliers de 5 faces.
        assert_eq!(rig.scene.panels.solid().len(), 6 + 6 * 5);
    }

    #[test]
    fn la_dalle_n_est_un_obstacle_que_si_elle_est_au_dessus_du_sol() {
        let mut rig = Rig::new(10);
        let mut over = rig.start(CrossRoadEvent::new);
        over.y = OVER_Y_M;
        let o = over.obstruction().expect("dalle surélevée");
        assert!(o.lo > 15.0 && o.hi > o.lo && o.z_near > o.z_far);
        over.y = UNDER_Y_M;
        assert!(over.obstruction().is_none());
    }
}
