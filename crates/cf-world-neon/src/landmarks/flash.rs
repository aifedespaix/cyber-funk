//! Sécurité photosensible des immeubles-vitrines (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : tout ce qui brille d'un coup,
//! clignote, éclate ou « glitche » passe par le [`FlashLimiter`] de `cf-show`, comme les enseignes (`signs::flash`), le
//! trafic (`traffic::flash`) et la voiture (`car::safety`).
//!
//! Sillage faisait claquer ses immeubles de bien des façons : pouls des fenêtres et des lisérés sur chaque kick, éruption
//! de la lave, panneau ERROR en stroboscope à 17 Hz, hologramme qui se déchire, arche qui éclate en voxels rouges clignotants,
//! visages des robots qui se déchirent. Ici :
//!
//! - [`PulseGate`] : le pouls des kicks, **partagé** par tous les immeubles (`beat.pulse` que lit la logique est le niveau
//!   que rend le limiteur) ;
//! - [`Burst`] : un limiteur propre à un immeuble pour ses événements rares (drop, éruption, déchirure). Le niveau rendu va
//!   de 0 à 1 ; au plus 3 départs par seconde, montée et descente d'au moins `MIN_FADE` ;
//! - [`slew`] : borne la pente d'une grandeur lumineuse continue (lumières dynamiques) à `1 / MIN_FADE` par seconde.
//!
//! **Au repos** (flashs coupés par le look, mode respiration, pas de spectacle) : les deux limiteurs rendent 0, ce qui est
//! l'état « calme » de chaque immeuble : pas de pouls, pas d'éruption, pas de déchirure, mais la scène continue de
//! s'animer lentement (pluie de code, balancier, robots).

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MIN_FADE};

/// Le pouls doit passer de sous `EDGE_LOW` à au-dessus de `EDGE_HIGH` pour compter pour un kick.
const EDGE_HIGH: f32 = 0.8;
const EDGE_LOW: f32 = 0.5;
/// Montée du pouls (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const PULSE_ATTACK_S: f32 = 0.1;
/// Descente du pouls (s).
pub const PULSE_DECAY_S: f32 = 0.25;
/// Montée d'un événement (s).
pub const BURST_ATTACK_S: f32 = 0.1;

fn request(attack_s: f32, decay_s: f32) -> FlashRequest {
    FlashRequest {
        channel: Channel::Flash,
        peak: Channel::Flash.cap(),
        attack_s,
        decay_s,
    }
}

/// Le pouls des kicks rendu par le limiteur, pour tous les immeubles.
#[derive(Debug, Clone, Default)]
pub struct PulseGate {
    limiter: FlashLimiter,
    last_pulse: f32,
}

impl PulseGate {
    /// Avance d'une image. `permitted` : les flashs sont permis ; `now` : horloge (s) ; `raw_pulse` : pouls brut de
    /// l'analyse (0 → 1). Rend le pouls borné, 0 → 1.
    pub fn update(&mut self, dt_s: f32, now: f64, permitted: bool, raw_pulse: f32) -> f32 {
        self.limiter.set_policy(permitted, false);
        if raw_pulse > EDGE_HIGH && self.last_pulse < EDGE_LOW {
            self.limiter.trigger(request(PULSE_ATTACK_S, PULSE_DECAY_S), now);
        }
        self.last_pulse = raw_pulse;
        self.limiter.step(dt_s);
        self.level()
    }

    /// Le pouls borné, 0 → 1 : le niveau du limiteur ramené au plafond de sa voie.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.limiter.value(Channel::Flash) / Channel::Flash.cap()
    }
}

/// Un événement rare d'un immeuble, vu à travers les bornes de sécurité.
#[derive(Debug, Clone, Default)]
pub struct Burst {
    limiter: FlashLimiter,
}

impl Burst {
    /// Demande un départ qui retombe en `decay_s` secondes. Refusé (faux) s'il est interdit ou trop rapproché du
    /// précédent : l'appelant n'a alors rien à montrer.
    pub fn trigger(&mut self, decay_s: f32, now: f64) -> bool {
        self.trigger_scaled(1.0, decay_s, now)
    }

    /// Comme [`Self::trigger`], avec une force de 0 à 1 (la part du plafond de la voie que l'on demande).
    pub fn trigger_scaled(&mut self, strength: f32, decay_s: f32, now: f64) -> bool {
        let mut request = request(BURST_ATTACK_S, decay_s);
        request.peak *= strength.clamp(0.0, 1.0);
        self.limiter.trigger(request, now)
    }

    /// Avance d'une image avec la politique du spectacle.
    pub fn step(&mut self, dt_s: f32, permitted: bool) {
        self.limiter.set_policy(permitted, false);
        self.limiter.step(dt_s);
    }

    /// Le niveau borné, 0 → 1.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.limiter.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// Les flashs sont-ils permis ?
    #[must_use]
    pub fn permitted(&self) -> bool {
        self.limiter.permitted()
    }

    /// Remet à zéro (l'immeuble est rangé).
    pub fn reset(&mut self) {
        self.limiter = FlashLimiter::new();
    }
}

/// Rapproche `current` de `target` sans jamais varier plus vite que `1 / MIN_FADE` par seconde, en montée comme en
/// descente : une lumière dynamique qui suit une énergie en dents de scie ne devient jamais un flash.
#[must_use]
pub fn slew(current: f32, target: f32, dt_s: f32) -> f32 {
    let max_step = dt_s.max(0.0) / MIN_FADE;
    current + (target - current).clamp(-max_step, max_step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_show::limiter::MAX_FLASHES_PER_SECOND;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn le_pouls_de_flashs_coupes_reste_a_zero() {
        let mut gate = PulseGate::default();
        for i in 0..600 {
            let pulse = if i % 15 == 0 { 1.0 } else { 0.0 };
            assert!(gate.update(DT, f64::from(DT) * f64::from(i), false, pulse).abs() < f32::EPSILON);
        }
    }

    #[test]
    fn le_pouls_borne_ne_saute_jamais_plus_vite_que_min_fade() {
        let mut gate = PulseGate::default();
        let mut last = 0.0_f32;
        let mut peak = 0.0_f32;
        for i in 0..1200 {
            // Pire cas : un kick toutes les 0,25 s.
            let raw = if i % 15 < 3 { 1.0 } else { 0.0 };
            let level = gate.update(DT, f64::from(DT) * f64::from(i), true, raw);
            assert!(
                (level - last).abs() <= DT / MIN_FADE + 1e-3,
                "saut de {}",
                (level - last).abs()
            );
            last = level;
            peak = peak.max(level);
        }
        assert!(peak > 0.9, "le pouls part quand même ({peak})");
    }

    #[test]
    fn jamais_plus_de_trois_departs_par_seconde() {
        let mut burst = Burst::default();
        let mut starts = 0usize;
        let mut times = Vec::new();
        for i in 0..1200 {
            let now = f64::from(DT) * f64::from(i);
            burst.step(DT, true);
            // On redemande à chaque image : le limiteur décide.
            if burst.trigger(0.3, now) {
                starts += 1;
                times.push(now);
            }
        }
        assert!(starts > 0);
        for &t in &times {
            let in_second = times.iter().filter(|&&u| u >= t && u < t + 1.0).count();
            assert!(in_second <= MAX_FLASHES_PER_SECOND, "{in_second} départs en 1 s");
        }
    }

    #[test]
    fn un_depart_interdit_est_refuse_et_rien_ne_brille() {
        let mut burst = Burst::default();
        burst.step(DT, false);
        assert!(!burst.trigger(1.0, 1.0));
        assert!(burst.level().abs() < f32::EPSILON);
    }

    #[test]
    fn couper_les_flashs_en_plein_depart_redescend_en_min_fade() {
        let mut burst = Burst::default();
        burst.step(DT, true);
        assert!(burst.trigger(1.0, 0.0));
        let mut now = 0.0;
        while burst.level() < 0.9 {
            now += f64::from(DT);
            burst.step(DT, true);
        }
        burst.step(DT, false);
        let mut frames = 0;
        while burst.level() > 0.0 {
            burst.step(DT, false);
            frames += 1;
            assert!(frames < 20, "toujours allumé après {frames} images");
        }
        assert!(now > 0.0);
    }

    #[test]
    fn la_pente_d_une_lumiere_est_bornee() {
        let mut v = 0.0;
        for _ in 0..30 {
            let next = slew(v, 1.0, DT);
            assert!((next - v).abs() <= DT / MIN_FADE + 1e-6);
            v = next;
        }
        assert!(v > 0.99);
    }
}
