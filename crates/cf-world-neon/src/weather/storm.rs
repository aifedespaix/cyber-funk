//! Les éclairs d'orage (règle 7 de `CLAUDE.md`, WCAG 2.3.1). Sillage ne faisait gronder que le tonnerre (`sfx.thunder`) ;
//! ici le tonnerre allume aussi, un instant, la brume : un éclair dans les nuages. Il passe par le [`FlashLimiter`] de
//! `cf-show`, comme les impacts de météores (`meteors::flash`) et la voiture.
//!
//! Garanties du limiteur : au plus 3 départs par seconde glissante, 0,25 s au moins entre deux, montée et descente d'au
//! moins `MIN_FADE`, plafond `MAX_FLASH_PEAK`. Flashs coupés (`FlashKill`), mode respiration ou absence de spectacle :
//! aucun éclair, la pluie et le tonnerre sonore restent. Ce que montre le shader est le niveau que rend le limiteur
//! ([`Lightning::level`]), jamais la demande.

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest};

/// Montée d'un éclair (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const ATTACK_S: f32 = 0.1;
/// Descente d'un éclair (s) : un éclair d'orage s'éteint plus lentement qu'il ne monte.
pub const DECAY_S: f32 = 0.45;

/// La voie des éclairs d'orage.
#[derive(Debug, Clone, Default)]
pub struct Lightning {
    limiter: FlashLimiter,
}

impl Lightning {
    /// Avance d'une image. `thunder` : le tonnerre gronde cette image ; `now_s` : horloge (s) ; `policy` :
    /// `(flashes, breathing)` du spectacle, `None` sans spectacle (rien ne flashe). Vrai si un éclair est accordé.
    pub fn update(&mut self, dt_s: f32, now_s: f64, thunder: bool, policy: Option<(bool, bool)>) -> bool {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        self.limiter.set_policy(flashes, breathing);
        let granted = thunder
            && self.limiter.trigger(
                FlashRequest {
                    channel: Channel::Flash,
                    peak: Channel::Flash.cap(),
                    attack_s: ATTACK_S,
                    decay_s: DECAY_S,
                },
                now_s,
            );
        self.limiter.step(dt_s);
        granted
    }

    /// Le niveau accordé, en lumière ajoutée : 0 → `MAX_FLASH_PEAK`.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.limiter.value(Channel::Flash)
    }
}

#[cfg(test)]
mod tests {
    use cf_show::limiter::{MAX_FLASH_PEAK, MAX_FLASHES_PER_SECOND, MIN_FADE};

    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Un tonnerre à chaque image (le pire cas) pendant `seconds`.
    fn hammer(policy: Option<(bool, bool)>, seconds: f32) -> (Vec<f32>, Vec<usize>) {
        let mut lightning = Lightning::default();
        let (mut levels, mut starts) = (Vec::new(), Vec::new());
        for i in 0..(seconds / DT) as usize {
            if lightning.update(DT, f64::from(DT) * i as f64, true, policy) {
                starts.push(i);
            }
            levels.push(lightning.level());
        }
        (levels, starts)
    }

    #[test]
    fn sans_spectacle_flashs_coupes_ou_respiration_pas_d_eclair() {
        for policy in [None, Some((false, false)), Some((true, true))] {
            let (levels, starts) = hammer(policy, 5.0);
            assert!(starts.is_empty() && levels.iter().all(|&l| l == 0.0), "{policy:?}");
        }
    }

    #[test]
    fn jamais_plus_de_trois_departs_par_seconde_ni_de_saut_plus_rapide_que_min_fade() {
        let (levels, starts) = hammer(Some((true, false)), 10.0);
        assert!(starts.len() >= 8, "des éclairs partent : {}", starts.len());
        let window = (1.0 / DT) as usize;
        for &s in &starts {
            let n = starts.iter().filter(|&&t| t >= s && t < s + window).count();
            assert!(n <= MAX_FLASHES_PER_SECOND, "{n} départs en 1 s");
        }
        assert!(levels.iter().all(|&l| l <= MAX_FLASH_PEAK + 1e-6));
        let max_step = MAX_FLASH_PEAK * DT / MIN_FADE + 1e-3;
        for pair in levels.windows(2) {
            assert!(
                (pair[0] - pair[1]).abs() <= max_step,
                "saut de {}",
                (pair[0] - pair[1]).abs()
            );
        }
    }

    #[test]
    fn couper_les_flashs_en_plein_eclair_redescend_en_min_fade() {
        let mut lightning = Lightning::default();
        let on = Some((true, false));
        assert!(lightning.update(DT, 0.0, true, on));
        for i in 1..10 {
            lightning.update(DT, f64::from(DT) * f64::from(i), false, on);
        }
        assert!(lightning.level() > 0.1);
        let mut frames = 0;
        while lightning.level() > 0.0 {
            lightning.update(DT, 1.0, false, Some((false, false)));
            frames += 1;
            assert!(frames < 20, "le niveau doit retomber");
        }
        assert!(frames as f32 * DT <= MIN_FADE + 2.0 * DT);
    }
}
