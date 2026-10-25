//! Sécurité photosensible des impacts (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : tout éclair d'impact passe par le
//! [`FlashLimiter`] de `cf-show`, comme la voiture (`car::safety`), le trafic et les enseignes.
//!
//! Sont des éclairs : le flash d'une explosion contre un immeuble et l'éclat de sol d'un astre qui s'éteint dans la brume.
//! Ils partagent **une seule voie** : un seul éclair brille à la fois, au plus 3 départs par seconde glissante (même
//! pendant une pluie de douze astres), 0,25 s au moins entre deux départs, montée et descente d'au moins `MIN_FADE`,
//! plafond `MAX_FLASH_PEAK`. Flashs coupés (`FlashKill`), mode respiration ou absence de spectacle : aucun éclair.
//!
//! Un impact refusé n'est pas muet pour autant : l'onde de choc, la gerbe et l'anneau de brume restent, plus sombres
//! (voir [`UNGRANTED_GAIN`]) ; seul l'éclat est retenu. Ce que montre le shader est le niveau que rend le limiteur
//! ([`ImpactFlash::level`]), jamais la demande de l'impact.

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MAX_FLASH_PEAK};

/// Montée d'un éclair d'impact (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const ATTACK_S: f32 = 0.1;
/// Descente d'un éclair d'impact (s).
pub const DECAY_S: f32 = 0.3;
/// Éclat de l'onde de choc et de l'anneau de brume quand le limiteur n'a pas accordé d'éclair : un anneau sombre qui
/// s'étale, sans à-coup de lumière.
pub const UNGRANTED_GAIN: f32 = 0.15;
/// Éclat de l'onde de choc et de l'anneau quand l'éclair est accordé : le plafond de la voie.
pub const GRANTED_GAIN: f32 = MAX_FLASH_PEAK;

/// La voie des éclairs d'impact.
#[derive(Debug, Clone, Default)]
pub struct ImpactFlash {
    limiter: FlashLimiter,
}

impl ImpactFlash {
    /// Règle la politique du spectacle (`(flashes, breathing)`, `None` sans spectacle : rien ne flashe), puis avance les
    /// fondus de `dt_s`. À appeler **après** les demandes de l'image : un éclair demandé commence à monter dans l'image.
    pub fn step(&mut self, dt_s: f32, policy: Option<(bool, bool)>) {
        self.set_policy(policy);
        self.limiter.step(dt_s);
    }

    /// Règle la politique avant de demander.
    pub fn set_policy(&mut self, policy: Option<(bool, bool)>) {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        self.limiter.set_policy(flashes, breathing);
    }

    /// Demande un éclair à l'instant `now` (s) : vrai s'il est accordé.
    pub fn request(&mut self, now: f64) -> bool {
        self.limiter.trigger(
            FlashRequest {
                channel: Channel::Flash,
                peak: Channel::Flash.cap(),
                attack_s: ATTACK_S,
                decay_s: DECAY_S,
            },
            now,
        )
    }

    /// Le niveau accordé, en lumière ajoutée : 0 → `MAX_FLASH_PEAK`.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.limiter.value(Channel::Flash)
    }

    /// Les éclairs sont-ils permis en ce moment ?
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

    /// Rejoue `seconds` secondes avec une demande à chaque image (le pire cas) ; rend le niveau et les départs.
    fn hammer(policy: Option<(bool, bool)>, seconds: f32) -> (Vec<f32>, Vec<usize>) {
        let mut flash = ImpactFlash::default();
        let (mut levels, mut starts) = (Vec::new(), Vec::new());
        for i in 0..(seconds / DT) as usize {
            let now = f64::from(DT) * i as f64;
            flash.set_policy(policy);
            if flash.request(now) {
                starts.push(i);
            }
            flash.step(DT, policy);
            levels.push(flash.level());
        }
        (levels, starts)
    }

    #[test]
    fn sans_spectacle_flashs_coupes_ou_respiration_rien_ne_flashe() {
        for policy in [None, Some((false, false)), Some((true, true))] {
            let (levels, starts) = hammer(policy, 5.0);
            assert!(starts.is_empty(), "{policy:?}");
            assert!(levels.iter().all(|&l| l == 0.0), "{policy:?}");
        }
    }

    #[test]
    fn jamais_plus_de_trois_departs_par_seconde_meme_a_chaque_image() {
        let (_, starts) = hammer(Some((true, false)), 10.0);
        assert!(starts.len() >= 8, "des éclairs partent : {}", starts.len());
        let window = (1.0 / DT) as usize;
        for &s in &starts {
            let in_second = starts.iter().filter(|&&t| t >= s && t < s + window).count();
            assert!(in_second <= MAX_FLASHES_PER_SECOND, "{in_second} départs en 1 s");
        }
        for pair in starts.windows(2) {
            assert!(pair[1] - pair[0] >= (0.25 / DT) as usize - 1, "écart de 0,25 s");
        }
    }

    #[test]
    fn le_niveau_ne_depasse_pas_le_plafond_ni_ne_varie_plus_vite_que_min_fade() {
        let (levels, _) = hammer(Some((true, false)), 10.0);
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
        let mut flash = ImpactFlash::default();
        flash.set_policy(Some((true, false)));
        assert!(flash.request(0.0));
        for _ in 0..8 {
            flash.step(DT, Some((true, false)));
        }
        assert!(flash.level() > 0.1);
        let mut frames = 0;
        while flash.level() > 0.0 {
            flash.step(DT, Some((false, false)));
            frames += 1;
            assert!(frames < 20, "le niveau doit retomber");
        }
        assert!(frames as f32 * DT <= MIN_FADE + 2.0 * DT);
    }
}
