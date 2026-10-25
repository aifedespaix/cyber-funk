//! Danseur géant (`events/dancer.ts`) : une silhouette dansante, haute comme un immeuble, se dresse au milieu de la route,
//! loin devant. Elle suit le tempo (bras, jambes, buste). Ce n'est que de la lumière : la voiture lui rentre dedans sans
//! rien heurter, et la projection bugue autour d'elle (bandes décalées, couleurs écartées, morceaux éteints) puis se
//! reforme derrière. Rien n'est déclaré comme obstacle, même pour la voiture volante.
//!
//! Deux plans croisés ([`PanelKind::Dancer`]) : de face on le voit plein, de côté sa moitié ; la voiture roule dedans. Le
//! gain suit le pouls que le limiteur laisse passer (`SceneFx::kick`), jamais le kick brut.

use bevy::math::{DVec3, Vec3};

use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::range;
use crate::traffic::panels::PanelKind;

const WIDTH_M: f32 = 34.0;
const HEIGHT_M: f32 = 54.0;
const AHEAD_M: f64 = 340.0;
/// On le garde jusqu'à ce que la voiture l'ait dépassé de cette distance, m.
const PAST_M: f64 = 150.0;
const FADE_IN_S: f32 = 2.0;

#[derive(Debug)]
pub(crate) struct DancerEvent {
    x: f32,
    z: f64,
    age: f32,
}

impl DancerEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        Self {
            x: range(ctx.rng, -8.0, 8.0),
            z: ctx.frame.car.z - AHEAD_M,
            age: 0.0,
        }
    }

    /// Pose les deux plans croisés avec le gain `gain` et le pouls accordé `pulse` (qui anime la silhouette).
    fn place(&self, ctx: &mut EventCtx<'_>, gain: f32, pulse: f32) {
        let centre = DVec3::new(f64::from(self.x), f64::from(HEIGHT_M * 0.5), self.z);
        let up = Vec3::new(0.0, HEIGHT_M * 0.5, 0.0);
        // Un plan dans la largeur de la route, l'autre tourné d'un quart de tour.
        for across in [Vec3::new(WIDTH_M * 0.5, 0.0, 0.0), Vec3::new(0.0, 0.0, -WIDTH_M * 0.5)] {
            ctx.scene
                .panels
                .quad_raw(PanelKind::Dancer, centre, across, up, [gain, pulse, 0.0], 0.0);
        }
    }
}

impl TrafficEvent for DancerEvent {
    fn structure(&self) -> bool {
        true
    }

    fn focus(&self) -> Option<Focus> {
        Some(Focus {
            position: DVec3::new(f64::from(self.x), f64::from(HEIGHT_M * 0.5), self.z),
            size: HEIGHT_M,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if ctx.frame.car.z < self.z - PAST_M {
            return false;
        }
        let fade_in = (self.age / FADE_IN_S).min(1.0);
        let pulse = ctx.scene.fx.kick;
        self.place(ctx, fade_in * (0.9 + pulse * 0.25), pulse);
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn le_danseur_se_leve_en_deux_secondes_n_est_pas_un_obstacle_et_part_derriere_la_voiture() {
        let mut rig = Rig::new(3);
        let mut event = rig.start(DancerEvent::new);
        let mut obstacles = Vec::new();
        event.obstructions(&mut obstacles);
        assert!(obstacles.is_empty() && event.obstruction().is_none(), "pas un obstacle");
        assert!(rig.step(&mut event, 1.0 / 60.0, |_| {}));
        let first = rig.scene.panels.light()[0].color[0];
        assert!(first < 0.05, "il se lève : {first}");
        let mut frames = 1;
        while rig.step(&mut event, 1.0 / 60.0, |_| {}) {
            frames += 1;
            assert!(frames < 60 * 60, "il finit quand la voiture l'a dépassé");
            assert_eq!(rig.scene.panels.light().len(), 2, "deux plans croisés");
        }
        // La voiture roule à 30 m/s : 340 + 150 m en environ 16 s.
        assert!((13 * 60..20 * 60).contains(&frames), "{frames} images");
    }
}
