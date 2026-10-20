//! Fondus de sécurité : noir (`Blackout`), intensité globale (`Master`) et opacité des couches (`LayerFade`).
//!
//! Le limiteur de flashs de Sillage ne couvre que les flashs ; le noir s'y coupait et s'y rallumait sec. Ici toute
//! grandeur que l'opérateur pousse d'un coup (un pad à maintien, une touche) et qui change la luminance de l'écran
//! glisse à vitesse bornée : une course complète ne prend jamais moins de `MIN_FADE` (WCAG 2.3.1, skill
//! `live-safety`), en montée comme en descente.

use crate::limiter::{MAX_STEP, MIN_FADE};

/// Durée (s) d'un passage complet au noir, ou du noir à l'image.
pub const BLACKOUT_FADE: f32 = 0.25;
/// Durée (s) d'une course complète de l'intensité globale ou de l'opacité d'une couche, poussée par l'opérateur.
pub const OPERATOR_FADE: f32 = 0.25;

const _: () = assert!(BLACKOUT_FADE >= MIN_FADE && OPERATOR_FADE >= MIN_FADE);

/// Une grandeur 0..1 qui suit sa consigne à vitesse bornée : au plus `dt / full_s` par image, avec `dt` borné à
/// `MAX_STEP` (un trou d'image ne fait pas sauter la valeur, comme dans le limiteur).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fade {
    value: f32,
    target: f32,
    full_s: f32,
}

impl Fade {
    /// Une grandeur au repos à `value`, qui fait une course complète en `full_s` secondes (au moins `MIN_FADE`).
    #[must_use]
    pub fn new(value: f32, full_s: f32) -> Self {
        let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 };
        Self {
            value,
            target: value,
            full_s: if full_s.is_finite() { full_s.max(MIN_FADE) } else { MIN_FADE },
        }
    }

    /// Nouvelle consigne, bornée à 0..1 ; une consigne non finie est ignorée.
    pub fn set_target(&mut self, target: f32) {
        if target.is_finite() {
            self.target = target.clamp(0.0, 1.0);
        }
    }

    /// Consigne courante.
    #[must_use]
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Valeur courante.
    #[must_use]
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Avance de `dt` secondes vers la consigne et rend la valeur. Un `dt` négatif ou non fini ne change rien.
    pub fn step(&mut self, dt: f32) -> f32 {
        if dt.is_finite() && dt > 0.0 {
            let max = dt.min(MAX_STEP) / self.full_s;
            self.value += (self.target - self.value).clamp(-max, max);
        }
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blackout_fades_in_and_out_never_faster_than_min_fade() {
        let dt = 1.0 / 60.0;
        let mut fade = Fade::new(0.0, BLACKOUT_FADE);
        fade.set_target(1.0);
        let mut frames = 0;
        while fade.step(dt) < 1.0 {
            frames += 1;
            assert!(frames < 1000);
        }
        // 0,25 s à 60 i/s : 15 images, jamais moins de MIN_FADE.
        assert!(frames as f32 * dt >= MIN_FADE);
        assert!((frames as f32 + 1.0) * dt >= BLACKOUT_FADE - 1e-4);
        fade.set_target(0.0);
        let before = fade.value();
        let after = fade.step(dt);
        assert!(before - after <= dt / MIN_FADE + 1e-6, "la sortie du noir glisse aussi");
    }

    #[test]
    fn a_frame_hitch_does_not_jump() {
        let mut fade = Fade::new(0.0, OPERATOR_FADE);
        fade.set_target(1.0);
        assert!(fade.step(2.0) <= MAX_STEP / OPERATOR_FADE + 1e-6);
        assert!((fade.step(f32::NAN) - fade.value()).abs() < f32::EPSILON);
        fade.set_target(f32::NAN);
        assert!((fade.target() - 1.0).abs() < f32::EPSILON);
    }
}
