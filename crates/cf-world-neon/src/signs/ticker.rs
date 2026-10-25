//! Le bandeau défilant (`signs/ticker.ts`) : la logique de son défilement. Le dessin du texte est cuit
//! (`paint::Painter::strip`) ; ici ne reste que la position.
//!
//! Le défilement n'est pas régulier : il avance par saccades. Chaque kick pousse le texte d'un cran proportionnel aux
//! basses, chaque coup de charleston lui donne un petit recul, et les aigus ajoutent un tremblement. Sans musique, il
//! ne reste que la dérive lente.

use super::audio::SignAudio;
use crate::rng::hash;

/// Vitesse de défilement de base, en pixels par seconde.
const BASE_SPEED: f32 = 70.0;

/// L'état d'un bandeau.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ticker {
    offset: f32,
    period: f32,
}

impl Ticker {
    /// Remet le bandeau au début d'une boucle de `period` pixels (`set`).
    pub fn set(&mut self, period: f32) {
        self.period = period.max(1.0);
        self.offset = 0.0;
    }

    /// Avance d'une image (`step`).
    pub fn step(&mut self, audio: &SignAudio) {
        self.offset += BASE_SPEED * audio.dt * (0.4 + audio.mid);
        if audio.calm {
            return;
        }
        if audio.kick {
            self.offset += 10.0 + audio.bass * 34.0;
        }
        if audio.snare {
            self.offset -= 6.0;
        }
        // Évite que la position grossisse sans fin (un set dure des heures) : on garde un nombre de boucles borné.
        if self.offset > self.period * 4.0 {
            self.offset -= self.period * 4.0;
        }
        if self.offset < 0.0 {
            self.offset += self.period;
        }
    }

    /// Position de lecture dans la boucle, en pixels, `0..period`, avec le tremblement des aigus (`shake`).
    #[must_use]
    pub fn position(&self, audio: &SignAudio, salt: f64) -> f32 {
        let shake = if audio.calm {
            0.0
        } else {
            (hash(audio.time * 60.0, salt) as f32 - 0.5) * audio.high * 3.0
        };
        (self.offset + shake).rem_euclid(self.period.max(1.0))
    }

    /// Longueur de la boucle, en pixels.
    #[must_use]
    pub const fn period(&self) -> f32 {
        self.period
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio() -> SignAudio {
        SignAudio {
            dt: 1.0 / 60.0,
            ..SignAudio::default()
        }
    }

    #[test]
    fn la_derive_lente_suffit_sans_musique() {
        let mut t = Ticker::default();
        t.set(1_000.0);
        let a = audio();
        for _ in 0..60 {
            t.step(&a);
        }
        // 70 px/s × 0,4 pendant une seconde.
        assert!((t.position(&a, 1.0) - 28.0).abs() < 0.5, "{}", t.position(&a, 1.0));
    }

    #[test]
    fn un_kick_pousse_le_texte_et_un_charleston_le_recule() {
        let mut t = Ticker::default();
        t.set(1_000.0);
        let mut a = audio();
        a.kick = true;
        a.bass = 1.0;
        t.step(&a);
        assert!(t.position(&audio(), 0.0) > 40.0);
        let before = t.position(&audio(), 0.0);
        a.kick = false;
        a.snare = true;
        t.step(&a);
        assert!(t.position(&audio(), 0.0) < before + 1.0);
    }

    #[test]
    fn la_position_reste_dans_la_boucle_et_le_calme_ne_tremble_pas() {
        let mut t = Ticker::default();
        t.set(500.0);
        let mut a = audio();
        a.high = 1.0;
        a.mid = 1.0;
        for _ in 0..100_000 {
            t.step(&a);
            let p = t.position(&a, 2.0);
            assert!((0.0..500.0).contains(&p), "{p}");
        }
        a.calm = true;
        a.time = 3.0;
        let p1 = t.position(&a, 1.0);
        a.time = 3.1;
        assert!((t.position(&a, 1.0) - p1).abs() < 1e-6);
    }
}
