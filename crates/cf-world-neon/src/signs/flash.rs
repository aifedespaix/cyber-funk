//! Sécurité photosensible des enseignes (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : tout ce qui brille d'un coup, clignote ou
//! pulse sur une enseigne passe par le [`FlashLimiter`] de `cf-show`, comme le post-process (`cf-render::postfx`) et la
//! voiture (`car::safety`).
//!
//! Sillage faisait claquer ses panneaux de bien des façons : néons qui grésillent, « ACHÈTE MOI » en stroboscope sur les
//! caisses claires, « GAME OVER » à 14 Hz sur les drops, flash blanc du radar, pulsation des lisérés sur chaque kick,
//! panneaux qui bugent à l'approche. Ici :
//!
//! - [`SignFlash`] : deux limiteurs partagés par toutes les enseignes. `pulse` suit les kicks (lisérés, halos, bandeau,
//!   « ACHÈTE MOI ») ; `burst` porte les événements rares (flash du radar, verdict des drops). Ce que les enseignes
//!   montrent est le niveau que le limiteur rend : au plus 3 départs par seconde, montée et descente d'au moins
//!   `MIN_FADE`, plafond de la voie. Flashs coupés ou respiration : tout retombe à 0 en `MIN_FADE` au plus.
//! - [`DisplayGate`] : le bug d'un panneau qu'on traverse. L'écran qui s'éteint et se rallume est une variation de
//!   luminance : on la borne de la même façon (changements espacés, fondus de `MIN_FADE`). Flashs coupés : seule
//!   l'extinction du passage reste, une fois, en fondu.

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MAX_FLASHES_PER_SECOND, MIN_FADE};

/// Le pouls doit passer de sous `EDGE_LOW` à au-dessus de `EDGE_HIGH` pour compter pour un kick (`ShowFx::trigger`).
const EDGE_HIGH: f32 = 0.8;
const EDGE_LOW: f32 = 0.5;
/// Montée du pouls des enseignes (s) : un peu plus que `MIN_FADE`.
const PULSE_ATTACK_S: f32 = 0.1;
/// Descente du pouls des enseignes (s).
const PULSE_DECAY_S: f32 = 0.25;
/// Montée d'un événement (s).
const BURST_ATTACK_S: f32 = 0.1;
/// Descente d'un événement (s) : assez longue pour que le verdict ou le radar se lise.
const BURST_DECAY_S: f32 = 0.6;

/// Les deux limiteurs des enseignes.
#[derive(Debug, Clone, Default)]
pub struct SignFlash {
    pulse: FlashLimiter,
    burst: FlashLimiter,
    last_pulse: f32,
}

impl SignFlash {
    /// Avance d'une image. `policy` : `(flashes, breathing)` du spectacle, `None` sans spectacle ; `now` : horloge (s) ;
    /// `kick_pulse` : pouls du kick (0 → 1) ; `event` : un événement demande un flash (radar, drop).
    pub fn update(&mut self, dt_s: f32, now: f64, policy: Option<(bool, bool)>, kick_pulse: f32, event: bool) {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        self.pulse.set_policy(flashes, breathing);
        self.burst.set_policy(flashes, breathing);
        if kick_pulse > EDGE_HIGH && self.last_pulse < EDGE_LOW {
            self.pulse.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: PULSE_ATTACK_S,
                    decay_s: PULSE_DECAY_S,
                },
                now,
            );
        }
        if event {
            self.burst.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: BURST_ATTACK_S,
                    decay_s: BURST_DECAY_S,
                },
                now,
            );
        }
        self.last_pulse = kick_pulse;
        self.pulse.step(dt_s);
        self.burst.step(dt_s);
    }

    /// Le pouls que montrent les enseignes, 0 → 1 : le niveau du limiteur ramené au plafond de sa voie.
    #[must_use]
    pub fn pulse(&self) -> f32 {
        self.pulse.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// Le flash d'événement, 0 → 1, même échelle.
    #[must_use]
    pub fn burst(&self) -> f32 {
        self.burst.value(Channel::Flash) / Channel::Flash.cap()
    }
}

/// Écart minimal entre deux changements d'état de l'écran d'un panneau qui bugue, en s : trois cycles éteint/allumé par
/// seconde au plus, comme `MAX_FLASHES_PER_SECOND`.
pub const MIN_TOGGLE_GAP_S: f64 = 1.0 / (2.0 * MAX_FLASHES_PER_SECOND as f64);

/// L'écran d'un panneau, vu à travers les bornes de sécurité.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayGate {
    target_on: bool,
    last_toggle_s: f64,
    shown: f32,
}

impl Default for DisplayGate {
    fn default() -> Self {
        Self {
            target_on: true,
            last_toggle_s: f64::NEG_INFINITY,
            shown: 1.0,
        }
    }
}

impl DisplayGate {
    /// Un panneau neuf : écran allumé, sans historique.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Avance d'une image. `wanted_on` : ce que le bug demande ; `passing` : l'extinction est celle du passage ;
    /// `permitted` : les flashs sont permis (`FlashLimiter::permitted`). Rend la luminosité de l'écran, 0 → 1.
    pub fn step(&mut self, dt_s: f32, now: f64, wanted_on: bool, passing: bool, permitted: bool) -> f32 {
        // Flashs coupés : un clignotement de bug est ignoré, l'extinction du passage reste (une fois, en fondu).
        let wanted = if permitted || passing { wanted_on } else { true };
        if wanted != self.target_on && now - self.last_toggle_s >= MIN_TOGGLE_GAP_S {
            self.target_on = wanted;
            self.last_toggle_s = now;
        }
        let goal = if self.target_on { 1.0 } else { 0.0 };
        let max_step = dt_s.max(0.0) / MIN_FADE;
        self.shown += (goal - self.shown).clamp(-max_step, max_step);
        self.shown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Une suite de demandes pseudo-aléatoire et reproductible.
    fn noise(seed: &mut u64) -> f32 {
        *seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        ((*seed >> 33) as f32) / (u32::MAX >> 1) as f32
    }

    #[test]
    fn pas_plus_de_trois_departs_par_seconde_ni_de_variation_plus_rapide_que_min_fade() {
        let mut seed = 7;
        let mut flash = SignFlash::default();
        let (mut last_p, mut last_b) = (0.0_f32, 0.0_f32);
        let mut starts = Vec::new();
        for i in 0..60 * 120 {
            let now = f64::from(i) / 60.0;
            // Un pouls qui frappe à 180 bpm et des événements demandés à toute vitesse.
            let pulse = if i % 20 < 3 { 1.0 } else { 0.0 };
            flash.update(DT, now, Some((true, false)), pulse, noise(&mut seed) > 0.5);
            let (p, b) = (flash.pulse(), flash.burst());
            let slew = 1.0 / MIN_FADE * DT + 1e-4;
            assert!((p - last_p).abs() <= slew && (b - last_b).abs() <= slew, "variation trop brusque à {now:.2} s");
            if p > 0.02 && last_p <= 0.02 {
                starts.push(now);
            }
            (last_p, last_b) = (p, b);
        }
        for window in starts.windows(MAX_FLASHES_PER_SECOND + 1) {
            assert!(window[MAX_FLASHES_PER_SECOND] - window[0] >= 1.0, "plus de 3 départs en 1 s : {window:?}");
        }
        assert!(starts.len() > 100, "le limiteur a laissé passer des flashs : {}", starts.len());
    }

    #[test]
    fn flashs_coupes_ou_respiration_tout_retombe_et_rien_ne_repart() {
        for policy in [Some((false, false)), Some((true, true)), Some((false, true)), None] {
            let mut flash = SignFlash::default();
            for i in 0..60 * 20 {
                let now = f64::from(i) / 60.0;
                flash.update(DT, now, policy, if i % 20 < 3 { 1.0 } else { 0.0 }, i % 7 == 0);
                if i > 10 {
                    assert!(flash.pulse() == 0.0 && flash.burst() == 0.0, "{policy:?} à {now:.2} s");
                }
            }
        }
    }

    #[test]
    fn un_ecran_qui_bugue_change_au_plus_six_fois_par_seconde_en_fondu() {
        let mut seed = 99;
        let mut gate = DisplayGate::default();
        let mut last = 1.0_f32;
        let mut flips = 0;
        let mut state_on = true;
        for i in 0..60 * 60 {
            let now = f64::from(i) / 60.0;
            // Le bug demande un nouvel état à presque chaque image.
            let wanted = noise(&mut seed) > 0.3;
            let shown = gate.step(DT, now, wanted, false, true);
            assert!((shown - last).abs() <= DT / MIN_FADE + 1e-4, "fondu trop rapide");
            let on = shown > 0.5;
            if on != state_on {
                flips += 1;
                state_on = on;
            }
            last = shown;
        }
        // 60 s × 6 changements par seconde, au plus.
        assert!(flips <= 60 * 6 + 2, "{flips} changements en 60 s");
        assert!(flips > 20, "le bug doit rester visible quand les flashs sont permis ({flips})");
    }

    #[test]
    fn flashs_coupes_seule_l_extinction_du_passage_reste() {
        let mut gate = DisplayGate::default();
        let mut seed = 5;
        for i in 0..600 {
            let shown = gate.step(DT, f64::from(i) / 60.0, noise(&mut seed) > 0.5, false, false);
            assert!((shown - 1.0).abs() < 1e-6, "un clignotement de bug a survécu à la coupure des flashs");
        }
        let mut last = 1.0;
        for i in 600..660 {
            last = gate.step(DT, f64::from(i) / 60.0, false, true, false);
        }
        assert!(last < 0.01, "le passage éteint l'écran ({last})");
        for i in 660..720 {
            last = gate.step(DT, f64::from(i) / 60.0, true, false, false);
        }
        assert!((last - 1.0).abs() < 1e-6, "puis il se rallume ({last})");
    }
}
