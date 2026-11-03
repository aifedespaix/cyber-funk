//! Lumière de l'habitacle : une lueur douce au-dessus du passager qui respire, vire de teinte et réagit un peu à la
//! route et à la musique, pour que le cockpit ne soit jamais une boîte noire figée.
//!
//! Règle 7 : le kick qu'elle reçoit est déjà le niveau **borné par le limiteur** (`CarFlash::kick`). La respiration est
//! lente (≈ 0,1 Hz, très en dessous de 3 Hz) et de faible amplitude ; quand les flashs sont coupés, tout reste fixe.

use bevy::color::{LinearRgba, Mix};

/// Intensité de repos, en candelas (l'ancienne lumière fixe valait 0,12).
const BASE_CD: f32 = 0.30;
/// Respiration : profondeur (±) autour de 1 et fréquence, Hz.
const BREATH_DEPTH: f32 = 0.2;
const BREATH_HZ: f32 = 0.1;
/// Dérive de teinte entre le néon du moment et le second néon, Hz.
const DRIFT_HZ: f32 = 0.06;
/// Gain maximal d'un kick (multiplicateur : 1 + gain × niveau).
const KICK_GAIN: f32 = 1.1;
/// Gain maximal d'un virage serré.
const CORNER_GAIN: f32 = 0.35;
/// Vitesse latérale (m/s) qui donne le gain de virage maximal.
const CORNER_FULL_MPS: f32 = 4.0;
/// Relâchement du kick, par seconde (≈ 0,3 s).
const KICK_DECAY_PER_S: f32 = 3.5;
/// Part d'alarme (rouge) ajoutée à la teinte quand on freine à fond.
const BRAKE_TINT: f32 = 0.45;

/// État animé de la lumière de l'habitacle.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CabinLight {
    clock: f32,
    kick: f32,
    corner: f32,
    brake: f32,
    animated: bool,
}

impl CabinLight {
    /// Avance d'une image. `kick` : 0 à 1, déjà borné par le limiteur ; `brake` : 0 à 1 ; `lateral_mps` : vitesse
    /// latérale mesurée ; `animated` : les flashs sont permis (sinon la lumière reste fixe).
    pub fn update(&mut self, dt_s: f32, kick: f32, brake: f32, lateral_mps: f32, animated: bool) {
        self.clock += dt_s;
        self.animated = animated;
        let kick = kick.clamp(0.0, 1.0);
        self.kick = if kick > self.kick {
            kick
        } else {
            kick.max(self.kick - dt_s * KICK_DECAY_PER_S)
        };
        // Le virage et le freinage arrivent lissés : jamais de saut de lumière.
        let corner = (lateral_mps.abs() / CORNER_FULL_MPS).clamp(0.0, 1.0);
        let follow = (dt_s * 2.0).min(1.0);
        self.corner += (corner - self.corner) * follow;
        self.brake += (brake.clamp(0.0, 1.0) - self.brake) * follow;
    }

    /// Intensité voulue, en candelas.
    #[must_use]
    pub fn intensity_cd(&self) -> f32 {
        if !self.animated {
            return BASE_CD;
        }
        let breath = 1.0 + BREATH_DEPTH * (self.clock * std::f32::consts::TAU * BREATH_HZ).sin();
        BASE_CD * breath * (1.0 + KICK_GAIN * self.kick + CORNER_GAIN * self.corner)
    }

    /// Teinte : dérive lente entre `glow` et `secondary`, qui rougit au freinage.
    #[must_use]
    pub fn color(&self, glow: LinearRgba, secondary: LinearRgba, alarm: LinearRgba) -> LinearRgba {
        let drift = if self.animated {
            0.5 + 0.5 * (self.clock * std::f32::consts::TAU * DRIFT_HZ).sin()
        } else {
            1.0
        };
        glow.mix(&secondary, drift).mix(&alarm, BRAKE_TINT * self.brake)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(seconds: f32, kick: f32, brake: f32, lateral: f32, animated: bool) -> CabinLight {
        let mut light = CabinLight::default();
        for _ in 0..(seconds * 60.0) as usize {
            light.update(1.0 / 60.0, kick, brake, lateral, animated);
        }
        light
    }

    #[test]
    fn fixe_quand_les_flashs_sont_coupes() {
        let light = run(5.0, 1.0, 1.0, 9.0, false);
        assert!((light.intensity_cd() - BASE_CD).abs() < 1e-6);
    }

    #[test]
    fn reste_douce_meme_au_maximum() {
        let light = run(1.0, 1.0, 1.0, 9.0, true);
        let max = BASE_CD * (1.0 + BREATH_DEPTH) * (1.0 + KICK_GAIN + CORNER_GAIN);
        assert!(light.intensity_cd() <= max + 1e-4);
        assert!(max < 1.2, "l'habitacle ne doit jamais éblouir : {max}");
    }

    #[test]
    fn la_respiration_est_lente_et_bornee() {
        let mut light = CabinLight::default();
        let mut previous: Option<f32> = None;
        for _ in 0..600 {
            light.update(1.0 / 60.0, 0.0, 0.0, 0.0, true);
            let now = light.intensity_cd();
            assert!((BASE_CD * (1.0 - BREATH_DEPTH)..=BASE_CD * (1.0 + BREATH_DEPTH) + 1e-6).contains(&now));
            if let Some(before) = previous {
                // Pas de saut d'une image à l'autre : moins de 1 % de la base.
                assert!((now - before).abs() < BASE_CD * 0.01);
            }
            previous = Some(now);
        }
    }

    #[test]
    fn le_kick_se_relache() {
        let mut light = run(0.5, 1.0, 0.0, 0.0, true);
        let peak = light.intensity_cd();
        for _ in 0..90 {
            light.update(1.0 / 60.0, 0.0, 0.0, 0.0, true);
        }
        assert!(light.intensity_cd() < peak * 0.7);
    }

    #[test]
    fn le_freinage_rougit_la_teinte() {
        let glow = LinearRgba::rgb(0.0, 0.5, 1.0);
        let secondary = LinearRgba::rgb(0.0, 0.5, 1.0);
        let alarm = LinearRgba::rgb(1.0, 0.0, 0.0);
        let calm = run(2.0, 0.0, 0.0, 0.0, true).color(glow, secondary, alarm);
        let braking = run(2.0, 0.0, 1.0, 0.0, true).color(glow, secondary, alarm);
        assert!(braking.red > calm.red + 0.2);
    }
}
