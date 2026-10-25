//! Sécurité photosensible du paquet du drop (règle 7 de `CLAUDE.md`, WCAG 2.3.1) : le départ de chaque onde de choc et celui
//! de la gerbe passent par un [`FlashLimiter`] de `cf-show`, comme les impacts (`meteors::flash`), la voiture et le trafic.
//!
//! Un départ **accordé** brille au plafond de la voie ([`GRANTED_GAIN`]) ; un départ **refusé** (voie occupée, 0,25 s pas
//! écoulées, trois départs déjà dans la seconde) n'est pas muet : l'anneau et la gerbe restent, plus sombres
//! ([`UNGRANTED_GAIN`]). Quand les flashs ne sont pas permis (pas de spectacle, `FlashKill`, respiration), rien ne
//! s'allume du tout : le paquet est nul (`flash` du paquet est déjà 0 alors, voir `cf-show::drop`), et ce qui brillait
//! encore redescend en `MIN_FADE` au plus (voir `DropFx::master`).

use cf_show::limiter::{Channel, FlashLimiter, FlashRequest, MAX_FLASH_PEAK};

/// Montée d'un départ (s) : un peu plus que `MIN_FADE`, soit 6 images à 60 i/s.
pub const ATTACK_S: f32 = 0.1;
/// Descente demandée au limiteur (s) : la voie est libre 0,3 s après le départ, avant le suivant de la cascade.
pub const DECAY_S: f32 = 0.2;
/// Éclat d'un anneau ou d'une gerbe accordés : le plafond de la voie.
pub const GRANTED_GAIN: f32 = MAX_FLASH_PEAK;
/// Éclat d'un anneau ou d'une gerbe refusés : un anneau sombre qui s'étale, sans à-coup de lumière.
pub const UNGRANTED_GAIN: f32 = 0.15;

/// La voie des départs du paquet du drop.
#[derive(Debug, Clone, Default)]
pub struct DropFlash {
    limiter: FlashLimiter,
}

impl DropFlash {
    /// Règle la politique du spectacle (`(flashes, breathing)`, `None` sans spectacle : rien ne flashe). À appeler avant de
    /// demander.
    pub fn set_policy(&mut self, policy: Option<(bool, bool)>) {
        let (flashes, breathing) = policy.unwrap_or((false, true));
        self.limiter.set_policy(flashes, breathing);
    }

    /// Demande un départ à l'instant `now` (s) : vrai s'il est accordé.
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

    /// Fait avancer les fondus de `dt_s`.
    pub fn step(&mut self, dt_s: f32) {
        self.limiter.step(dt_s);
    }

    /// Les flashs sont-ils permis en ce moment ?
    #[must_use]
    pub fn permitted(&self) -> bool {
        self.limiter.permitted()
    }

    /// Niveau du limiteur, en lumière ajoutée : 0 → `MAX_FLASH_PEAK`.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.limiter.value(Channel::Flash)
    }
}

#[cfg(test)]
mod tests {
    use cf_show::limiter::MAX_FLASHES_PER_SECOND;

    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn rien_n_est_accorde_sans_spectacle_flashs_coupes_ou_respiration() {
        for policy in [None, Some((false, false)), Some((true, true))] {
            let mut flash = DropFlash::default();
            flash.set_policy(policy);
            assert!(!flash.request(0.0), "{policy:?}");
            assert!(!flash.permitted());
        }
    }

    #[test]
    fn jamais_plus_de_trois_departs_par_seconde_meme_demandes_a_chaque_image() {
        let mut flash = DropFlash::default();
        flash.set_policy(Some((true, false)));
        let mut starts = Vec::new();
        for i in 0..(10.0 / DT) as usize {
            let now = f64::from(DT) * i as f64;
            if flash.request(now) {
                starts.push(now);
            }
            flash.step(DT);
        }
        assert!(starts.len() >= 8);
        for &s in &starts {
            let in_second = starts.iter().filter(|&&t| t >= s && t < s + 1.0).count();
            assert!(in_second <= MAX_FLASHES_PER_SECOND, "{in_second} départs en 1 s");
        }
    }

    #[test]
    fn la_cascade_de_trois_ondes_espacee_de_034_s_est_entierement_accordee() {
        let mut flash = DropFlash::default();
        flash.set_policy(Some((true, false)));
        let mut granted = Vec::new();
        let mut now = 0.0_f64;
        for i in 0..40 {
            if i % 20 == 0 {
                granted.push(flash.request(now));
            }
            flash.step(DT);
            now += f64::from(DT);
        }
        // Le pas ci-dessus est de 20 images (0,33 s) : la cascade de `DropFx` (0,34 s) l'est aussi.
        assert_eq!(granted, [true, true]);
    }
}
