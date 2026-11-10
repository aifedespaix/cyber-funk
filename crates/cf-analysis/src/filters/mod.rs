//! Bibliothèque de filtres de signaux, sans Bevy ni allocation (P2 §2.2, chantier C1).
//!
//! Tous les coefficients sont de la forme `1 − exp(−Δt/τ)` ou une intégration exacte : sur un signal constant par
//! morceaux, le résultat aux instants d'image ne dépend pas de la cadence (30, 60 ou 144 i/s). Chaque filtre est
//! un petit état `Copy`, créé à l'ouverture et avancé par `step(x, dt)`.
//!
//! - [`smooth`] : attaque/relâchement, crête avec maintien, One-Euro, borne de pente ;
//! - [`spring`] : ressort à amortissement critique et sous-amorti ;
//! - [`trigger`] : bascule de Schmitt, zone morte, z-score adaptatif ;
//! - [`level`] : niveau en dB, AGC et courbe (étapes 1 à 3 de la chaîne).

pub mod level;
pub mod smooth;
pub mod spring;
pub mod trigger;

pub use level::{LevelAgc, curve_pow};
pub use smooth::{AttackRelease, OneEuro, PeakHold, SlewLimiter, alpha};
pub use spring::Spring;
pub use trigger::{AdaptiveZ, Schmitt, dead_zone};

/// Borne `x` dans 0..1 ; un nombre non fini vaut 0.
#[must_use]
pub fn sat(x: f32) -> f32 {
    if x.is_finite() { x.clamp(0.0, 1.0) } else { 0.0 }
}

/// Pas de temps utilisable : fini et positif, plafonné à une seconde (un trou d'image n'emballe aucun filtre).
#[must_use]
pub(crate) fn safe_dt(dt: f32) -> f32 {
    if dt.is_finite() { dt.clamp(0.0, 1.0) } else { 0.0 }
}

#[cfg(test)]
mod tests;
