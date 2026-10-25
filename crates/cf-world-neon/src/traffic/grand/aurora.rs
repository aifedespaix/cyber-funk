//! Rideau d'aurore (`events/aurora.ts`) : un grand plan vertical additif posé dans le ciel, devant la voiture. Son motif est
//! du bruit en rayons verticaux qui ondulent (lime, menthe, violet vers le haut). Elle se lève sur 5 s, brille un bon moment
//! (plus fort quand le morceau est calme) puis s'éteint.
//!
//! Le plan est un panneau de lumière ([`PanelKind::Aurora`]) reposé à chaque image ; sa quantité et son pouls sont ses
//! paramètres. Le pouls est celui que le limiteur laisse passer (`SceneFx::kick`), jamais celui du kick brut : un grand
//! plan qui battrait en tout ou rien serait un flash.

use bevy::math::{DVec3, Vec3};

use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::panels::PanelKind;

const WIDTH_M: f32 = 1100.0;
const HEIGHT_M: f32 = 260.0;
const LIFE_S: f32 = 36.0;
/// Le voile se tient loin devant et haut, au-dessus des plus hautes tours (280 m) : on le voit surgir derrière la ville.
const AHEAD_M: f64 = 560.0;
const BOTTOM_M: f64 = 190.0;
const RAMP_S: f32 = 5.0;

#[derive(Debug, Default)]
pub(crate) struct AuroraEvent {
    age: f32,
}

impl AuroraEvent {
    pub(crate) fn new(_ctx: &mut EventCtx<'_>) -> Self {
        Self::default()
    }
}

impl TrafficEvent for AuroraEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > LIFE_S {
            return false;
        }
        let envelope = (self.age / RAMP_S).min(1.0) * ((LIFE_S - self.age) / RAMP_S).min(1.0);
        let calm = 1.0 - ctx.frame.drive.intensity * 0.35;
        let pulse = ctx.scene.fx.kick;
        ctx.scene.panels.quad_raw(
            PanelKind::Aurora,
            DVec3::new(0.0, BOTTOM_M + f64::from(HEIGHT_M) * 0.5, ctx.frame.car.z - AHEAD_M),
            Vec3::new(WIDTH_M * 0.5, 0.0, 0.0),
            Vec3::new(0.0, HEIGHT_M * 0.5, 0.0),
            [envelope * calm, pulse, 0.0],
            0.0,
        );
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn l_aurore_se_leve_brille_puis_s_eteint_sans_pulser_hors_limiteur() {
        let mut rig = Rig::new(1);
        let mut event = rig.start(AuroraEvent::new);
        let mut amounts = Vec::new();
        for _ in 0..(40 * 60) {
            // Un pouls brut de kick est présent dans l'image ; le limiteur n'en laisse rien passer (`kick` = 0).
            let alive = rig.step(&mut event, 1.0 / 60.0, |frame| frame.beat.pulse = 1.0);
            amounts.push(rig.scene.panels.light().first().map(|p| p.color));
            if !alive {
                break;
            }
        }
        let first = amounts
            .first()
            .copied()
            .flatten()
            .expect("un panneau dès la première image");
        assert!(first[0] < 0.05, "elle se lève : {first:?}");
        let peak = amounts.iter().flatten().map(|c| c[0]).fold(0.0, f32::max);
        assert!(peak > 0.5, "elle brille : {peak}");
        assert!(amounts.iter().flatten().all(|c| c[1] == 0.0), "pouls brut interdit");
        assert!(amounts.len() < 40 * 60, "elle finit d'elle-même");
    }
}
