//! Pouls de la ville (`events/city-pulse.ts`) : pendant quelques phrases, les fenêtres des immeubles clignotent sur le kick.
//! À chaque temps un autre lot de fenêtres éteintes s'allume (voir `uWinPulse` dans la façade), puis retombe avec le pouls
//! du kick. Ce n'est pas un flash plein écran : seules les fenêtres bougent, au rythme des kicks. Si les flashs sont coupés,
//! le pouls devient une respiration lente, sans à-coup.
//!
//! Règle 7 : le pouls est celui que le limiteur laisse passer (`SceneFx::kick`), jamais le kick brut. Quand les flashs ne
//! sont pas permis (`SceneFx::flashes` faux : pas de spectacle, flashs coupés, respiration), la respiration prend le relais.
//!
//! Écart (voir la PR) : Sillage fixait « adouci » à la création, d'après le look du moment ; ici l'état est relu à chaque
//! image, de sorte qu'un look qui coupe les flashs en cours de route adoucit aussitôt l'événement.

use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::events::common::range;

/// Un temps musical = un kick ; une phrase = 16 temps.
const PHRASE: u32 = 16;
const FADE_IN_S: f32 = 0.8;
const FADE_OUT_S: f32 = 1.6;
/// Force du pouls quand le look n'en donne pas (`?? 0.6`).
const DEFAULT_AMOUNT: f32 = 0.6;
/// Au plus 48 s : sans kicks, l'événement ne dure pas indéfiniment.
const MAX_S: f32 = 48.0;
/// Période de la respiration lente, rad/s (environ 4 s).
const BREATH_RATE: f64 = 1.6;

#[derive(Debug)]
pub(crate) struct CityPulseEvent {
    phrases: u32,
    start_count: u32,
    amount: f32,
    age: f32,
    smooth: f32,
}

impl CityPulseEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        Self {
            phrases: range(ctx.rng, 2.0, 4.0).round() as u32,
            start_count: ctx.frame.beat.count,
            amount: ctx.frame.show.map_or(DEFAULT_AMOUNT, |s| s.city_pulse),
            age: 0.0,
            smooth: 0.0,
        }
    }
}

impl TrafficEvent for CityPulseEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        let length = self.phrases * PHRASE;
        let elapsed = ctx.frame.beat.count.saturating_sub(self.start_count);
        let done = elapsed >= length || self.age > MAX_S;
        let target = if done { 0.0 } else { 1.0 };
        let fade = if target > self.smooth { FADE_IN_S } else { FADE_OUT_S };
        self.smooth += (target - self.smooth) * (dt / fade * 2.0).min(1.0);
        let fx = &mut ctx.scene.fx;
        let (pulse, soft) = if fx.flashes {
            (fx.kick, 1.0)
        } else {
            // Respiration lente : 0,35 à 0,55, sans à-coup, à moitié de force.
            (
                0.35 + 0.2 * (0.5 + 0.5 * (ctx.frame.time * BREATH_RATE).sin() as f32),
                0.5,
            )
        };
        fx.win_pulse = self.amount.min(1.0) * pulse * self.smooth * soft;
        !(done && self.smooth < 0.02)
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        ctx.scene.fx.win_pulse = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    /// Rejoue l'événement avec un kick à 2 Hz ; `flashes` dit si le limiteur laisse passer le pouls (ici : 1 sur le kick).
    fn run(flashes: bool) -> (Vec<f32>, usize) {
        let mut rig = Rig::new(6);
        let mut event = rig.start(CityPulseEvent::new);
        let mut values = Vec::new();
        for i in 0..(60 * 60) {
            rig.scene.fx.flashes = flashes;
            rig.scene.fx.kick = if flashes && i % 30 < 6 { 1.0 } else { 0.0 };
            let alive = rig.step(&mut event, 1.0 / 60.0, |frame| {
                frame.beat.count = i / 30;
                frame.beat.kick = i % 30 == 0;
            });
            values.push(rig.scene.fx.win_pulse);
            if !alive {
                break;
            }
        }
        (values, event.phrases as usize)
    }

    #[test]
    fn le_pouls_suit_le_kick_accorde_puis_retombe_apres_les_phrases() {
        let (values, phrases) = run(true);
        let peak = values.iter().copied().fold(0.0, f32::max);
        assert!(peak > 0.5, "les fenêtres pulsent : {peak}");
        assert!((2..=4).contains(&phrases));
        // 2 à 4 phrases de 16 kicks à 2 Hz : de 16 à 32 s, puis la fin.
        assert!(values.len() < 40 * 60, "{} images", values.len());
        assert!(values.len() > 12 * 60);
    }

    #[test]
    fn sans_l_accord_du_limiteur_le_pouls_est_une_respiration_lente() {
        let (values, _) = run(false);
        let peak = values.iter().copied().fold(0.0, f32::max);
        assert!(peak < 0.35, "adouci : {peak}");
        // Aucun à-coup : d'une image à l'autre, la variation reste minuscule.
        let max_step = values.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(max_step < 0.01, "pas de saut : {max_step}");
    }
}
