//! Étapes 1 à 3 de la chaîne : niveau en dB, AGC par suivi de crête, courbe.

use super::{safe_dt, sat};

/// Niveau `L = 10·log10(e + 1e−10)` puis AGC : `L_pic = max(L, L_pic − Δt·R/τ_agc)` et
/// `x = sat((L − L_pic + R)/R)`, `R` étant la plage en dB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelAgc {
    /// Plage `R` (dB).
    pub range_db: f32,
    /// Durée de retombée de la crête sur toute la plage (s).
    pub tau_s: f32,
    peak_db: f32,
}

impl LevelAgc {
    /// AGC de P2 §2.2 : 36 dB sur 10 s.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            range_db: 36.0,
            tau_s: 10.0,
            peak_db: -100.0,
        }
    }

    /// Normalise l'énergie `e` (puissance) dans 0..1.
    pub fn step(&mut self, e: f32, dt: f32) -> f32 {
        let level = 10.0 * (e.max(0.0) + 1e-10).log10();
        let fall = safe_dt(dt) * self.range_db / self.tau_s.max(1e-3);
        self.peak_db = level.max(self.peak_db - fall);
        sat((level - self.peak_db + self.range_db) / self.range_db)
    }
}

impl Default for LevelAgc {
    fn default() -> Self {
        Self::new()
    }
}

/// Étape 3 : `x ← x^γ`.
#[must_use]
pub fn curve_pow(x: f32, gamma: f32) -> f32 {
    sat(x).powf(gamma.max(0.01))
}
