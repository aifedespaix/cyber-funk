//! Le bloom : réglages de l'`UnrealBloomPass` de Sillage traduits vers le `Bloom` de Bevy.
//!
//! Les deux algorithmes diffèrent : Sillage filtre l'image au-dessus d'un seuil de luminance, la floute sur 5 niveaux
//! (de la demi-résolution au 1/32) et rajoute la somme pondérée (`strength` × facteurs mélangés par `radius`) ; Bevy
//! fait une pyramide de sous-échantillonnage / sur-échantillonnage (filtre de Jimenez), avec une pondération par
//! fréquence (`low_frequency_boost`). On garde de Sillage ce qui se voit : le **seuil haut** (seules les sources
//! vraiment intenses rayonnent), le mode **additif**, la **demi-résolution** de départ et la **pulsation** sur le kick.
//!
//! L'intensité de Bevy ne répond pas linéairement à celle de Sillage (elle s'applique à chaque étage de la pyramide) :
//! elle est calée sur le rendu de référence de Sillage (`tools/sillage-golden/postfx/`, même scène, même shader
//! d'étalonnage), au repos (force 0,3) et au sommet d'un kick (force 0,5). Écart moyen résiduel sur la scène de
//! référence : 6 à 9 niveaux sur 255 (ciel, charte, halo d'un néon, reflet de la route), voir la PR de J4-02.

use bevy::post_process::bloom::{Bloom, BloomCompositeMode, BloomPrefilter};

/// Force du bloom au repos (`strength` de l'`UnrealBloomPass`, mode spectacle).
pub const BLOOM_STRENGTH: f32 = 0.3;
/// Surcroît de force au sommet d'un kick (`0.3 + pulse × 0.2` de `PostFX.setPulse`).
pub const BLOOM_PULSE: f32 = 0.2;
/// Seuil de luminance du bloom (`threshold` de l'`UnrealBloomPass`) : seules les arêtes et portiques rayonnent.
pub const BLOOM_THRESHOLD: f32 = 0.85;
/// Diviseur de résolution du bloom par défaut (`bloomDivisor` de Sillage) : 2 (net) ou plus (économe).
pub const DEFAULT_BLOOM_DIVISOR: u32 = 2;
/// Intensité de Bevy = `INTENSITY_AT_ZERO + INTENSITY_PER_STRENGTH × force de Sillage`, calée sur la référence :
/// force 0,3 → 0,45, force 0,5 → 0,55. Valable sur la plage de `bloom_strength` (0,3 → 0,5).
const INTENSITY_AT_ZERO: f32 = 0.3;
const INTENSITY_PER_STRENGTH: f32 = 0.5;
/// Douceur du genou du seuil. Sillage a un `smoothWidth` de 0,01 (un seuil presque franc) ; un genou trop dur
/// scintille sur les arêtes fines en mouvement avec la pyramide de Bevy.
const THRESHOLD_SOFTNESS: f32 = 0.2;
/// Bornes de la taille du premier niveau du bloom (px).
const MIN_MIP: u32 = 64;
const MAX_MIP: u32 = 2048;

/// Force du bloom pour une impulsion de kick `pulse` (1 → 0).
#[must_use]
pub fn bloom_strength(pulse: f32) -> f32 {
    let pulse = if pulse.is_finite() { pulse.clamp(0.0, 1.0) } else { 0.0 };
    BLOOM_STRENGTH + pulse * BLOOM_PULSE
}

/// Taille (px) du premier niveau du bloom : la hauteur de l'image divisée par `divisor`.
#[must_use]
pub fn bloom_mip(height_px: u32, divisor: u32) -> u32 {
    (height_px / divisor.max(1)).clamp(MIN_MIP, MAX_MIP)
}

/// Intensité du `Bloom` de Bevy pour une force de Sillage.
#[must_use]
pub fn bloom_intensity(strength: f32) -> f32 {
    INTENSITY_AT_ZERO + INTENSITY_PER_STRENGTH * strength
}

/// Le `Bloom` de Bevy pour une force de Sillage et une taille de premier niveau. Pondération des fréquences : celle
/// de `Bloom::NATURAL`, qui donne l'étalement de l'`UnrealBloomPass` sur la référence.
#[must_use]
pub fn bloom_settings(strength: f32, max_mip_dimension: u32) -> Bloom {
    Bloom {
        intensity: bloom_intensity(strength),
        prefilter: BloomPrefilter {
            threshold: BLOOM_THRESHOLD,
            threshold_softness: THRESHOLD_SOFTNESS,
        },
        composite_mode: BloomCompositeMode::Additive,
        max_mip_dimension,
        ..Bloom::NATURAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_kick_fait_respirer_le_bloom_comme_dans_sillage() {
        assert!((bloom_strength(0.0) - 0.3).abs() < 1e-6);
        assert!((bloom_strength(1.0) - 0.5).abs() < 1e-6);
        assert!((bloom_strength(f32::NAN) - 0.3).abs() < 1e-6);
        assert!((bloom_strength(4.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn le_premier_niveau_est_a_la_demi_resolution_par_defaut() {
        assert_eq!(bloom_mip(1080, DEFAULT_BLOOM_DIVISOR), 540);
        assert_eq!(bloom_mip(1080, 4), 270);
        assert_eq!(bloom_mip(1080, 0), 1080);
        assert_eq!(bloom_mip(10, 2), MIN_MIP);
    }

    #[test]
    fn le_bloom_est_additif_et_a_seuil_haut() {
        let bloom = bloom_settings(0.3, 540);
        assert_eq!(bloom.composite_mode, BloomCompositeMode::Additive);
        assert!((bloom.prefilter.threshold - 0.85).abs() < 1e-6);
        assert_eq!(bloom.max_mip_dimension, 540);
    }

    #[test]
    fn l_intensite_de_bevy_suit_le_calage_sur_sillage() {
        assert!((bloom_intensity(BLOOM_STRENGTH) - 0.45).abs() < 1e-6);
        assert!((bloom_intensity(BLOOM_STRENGTH + BLOOM_PULSE) - 0.55).abs() < 1e-6);
    }
}
