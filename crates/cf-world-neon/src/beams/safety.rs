//! Sécurité photosensible des faisceaux (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : le pouls que les kicks donnent aux
//! projecteurs (cône plus vif, tache plus large) passe par un [`FlashLimiter`] de `cf-show`, comme le font les bandes
//! néon du trafic (`traffic::flash`) et la voiture (`car::safety`).
//!
//! Ce que montrent les faisceaux est le niveau que le limiteur rend : au plus 3 départs par seconde glissante, 0,25 s
//! au moins entre deux, montée et descente d'au moins `MIN_FADE`, plafond de la voie. Sans spectacle, flashs coupés
//! (`FlashKill`) ou en mode respiration, le pouls est nul : il ne reste que la lueur de fond, qui suit les médiums
//! lissés (voir `field`).

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest};

/// Montée du pouls (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const ATTACK_S: f32 = 0.1;
/// Descente du pouls (s).
pub const DECAY_S: f32 = 0.25;
/// Le pouls doit passer de sous `EDGE_LOW` à au-dessus de `EDGE_HIGH` pour compter pour un kick.
const EDGE_HIGH: f32 = 0.8;
const EDGE_LOW: f32 = 0.5;

/// La voie du pouls des faisceaux.
#[derive(Debug, Clone, Default)]
pub struct BeamPulse {
    limiter: FlashLimiter,
    last_pulse: f32,
}

impl BeamPulse {
    /// Avance d'une image. `policy` : `(flashes, breathing)` du spectacle, `None` sans spectacle ; `now` : horloge de la
    /// scène (s) ; `pulse` : pouls du kick de l'analyse, 0 → 1.
    pub fn update(&mut self, dt_s: f32, now: f64, policy: Option<(bool, bool)>, pulse: f32) {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        self.limiter.set_policy(flashes, breathing);
        if pulse > EDGE_HIGH && self.last_pulse < EDGE_LOW {
            self.limiter.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: ATTACK_S,
                    decay_s: DECAY_S,
                },
                now,
            );
        }
        self.last_pulse = pulse;
        self.limiter.step(dt_s);
    }

    /// Le pouls que montrent les faisceaux, 0 → 1 : le niveau du limiteur ramené au plafond de sa voie.
    #[must_use]
    pub fn pulse(&self) -> f32 {
        self.limiter.value(Channel::Flash) / Channel::Flash.cap()
    }

    /// Les flashs sont-ils permis en ce moment ?
    #[must_use]
    pub fn permitted(&self) -> bool {
        self.limiter.permitted()
    }
}

#[cfg(test)]
mod tests {
    use cf_show::limiter::{MAX_FLASHES_PER_SECOND, MIN_FADE};

    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Rejoue `seconds` secondes avec un kick toutes les 0,25 s (le pire cas) ; rend le pouls de chaque image.
    fn run(policy: Option<(bool, bool)>, seconds: f32) -> Vec<f32> {
        let mut pulse = BeamPulse::default();
        (0..(seconds / DT) as usize)
            .map(|i| {
                let raw = match i % 15 {
                    0 => 1.0,
                    1..3 => 0.9,
                    _ => 0.0,
                };
                pulse.update(DT, f64::from(DT) * i as f64, policy, raw);
                pulse.pulse()
            })
            .collect()
    }

    #[test]
    fn sans_spectacle_flashs_coupes_ou_respiration_le_pouls_est_nul() {
        for policy in [None, Some((false, false)), Some((true, true))] {
            assert!(run(policy, 5.0).iter().all(|&p| p == 0.0), "{policy:?}");
        }
    }

    #[test]
    fn jamais_plus_de_trois_departs_par_seconde_ni_de_pente_sous_min_fade() {
        let levels = run(Some((true, false)), 10.0);
        // Un départ = le niveau repart de zéro vers le haut.
        let starts: Vec<usize> = (1..levels.len())
            .filter(|&i| levels[i] > 0.0 && levels[i - 1] == 0.0)
            .collect();
        assert!(starts.len() >= 8, "des pouls partent : {}", starts.len());
        let window = (1.0 / DT) as usize;
        for &s in &starts {
            let in_second = starts.iter().filter(|&&t| t >= s && t < s + window).count();
            assert!(in_second <= MAX_FLASHES_PER_SECOND, "{in_second} départs en 1 s");
        }
        let max_step = DT / MIN_FADE + 1e-3;
        for pair in levels.windows(2) {
            assert!(
                (pair[0] - pair[1]).abs() <= max_step,
                "saut de {}",
                (pair[0] - pair[1]).abs()
            );
        }
        assert!(levels.iter().all(|&p| p <= 1.0 + 1e-6));
    }

    #[test]
    fn couper_les_flashs_en_plein_pouls_redescend_en_min_fade() {
        let mut pulse = BeamPulse::default();
        pulse.update(DT, 0.0, Some((true, false)), 0.0);
        pulse.update(DT, 0.1, Some((true, false)), 1.0);
        for i in 0..8 {
            pulse.update(DT, 0.1 + f64::from(DT) * f64::from(i), Some((true, false)), 0.0);
        }
        assert!(pulse.pulse() > 0.2);
        let mut frames = 0;
        while pulse.pulse() > 0.0 {
            pulse.update(DT, 1.0, Some((false, false)), 0.0);
            frames += 1;
            assert!(frames < 20, "le pouls doit retomber");
        }
        assert!(frames as f32 * DT <= MIN_FADE + 2.0 * DT);
        assert!(!pulse.permitted());
    }
}
