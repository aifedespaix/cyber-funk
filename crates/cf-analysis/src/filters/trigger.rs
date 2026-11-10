//! Déclencheurs : bascule de Schmitt, zone morte, z-score adaptatif.

use super::{safe_dt, smooth::alpha};

/// Étape 4 : bascule de Schmitt `θ_on = 0.12`, `θ_off = 0.08` par défaut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Schmitt {
    /// Seuil de passage à l'état haut.
    pub on: f32,
    /// Seuil de retour à l'état bas (≤ `on`).
    pub off: f32,
    high: bool,
}

impl Schmitt {
    /// Bascule à l'état bas.
    #[must_use]
    pub const fn new(on: f32, off: f32) -> Self {
        Self { on, off, high: false }
    }

    /// État courant.
    #[must_use]
    pub const fn is_high(&self) -> bool {
        self.high
    }

    /// Met à jour avec `x` ; renvoie l'état.
    pub fn update(&mut self, x: f32) -> bool {
        if self.high {
            if x < self.off {
                self.high = false;
            }
        } else if x > self.on {
            self.high = true;
        }
        self.high
    }
}

/// Zone morte : `sat((x − θ)/(1 − θ))`.
#[must_use]
pub fn dead_zone(x: f32, theta: f32) -> f32 {
    let theta = theta.clamp(0.0, 0.999);
    super::sat((x - theta) / (1.0 - theta))
}

/// Z-score adaptatif : moyenne et variance glissantes à constante de temps `τ`. Le score d'une valeur se calcule
/// contre les statistiques d'*avant* cette valeur, pour qu'un pic ne s'absorbe pas lui-même.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveZ {
    /// Constante de temps des statistiques (s).
    pub tau_s: f32,
    /// Écart-type plancher : évite qu'un silence rende le moindre souffle « exceptionnel ».
    pub min_std: f32,
    mean: f32,
    var: f32,
}

impl AdaptiveZ {
    /// Statistiques neuves.
    #[must_use]
    pub const fn new(tau_s: f32, min_std: f32) -> Self {
        Self {
            tau_s,
            min_std,
            mean: 0.0,
            var: 0.0,
        }
    }

    /// Moyenne courante.
    #[must_use]
    pub const fn mean(&self) -> f32 {
        self.mean
    }

    /// Score de `x`, puis intégration de `x` aux statistiques.
    pub fn update(&mut self, x: f32, dt: f32) -> f32 {
        let std = self.var.sqrt().max(self.min_std);
        let z = (x - self.mean) / std;
        let a = alpha(safe_dt(dt), self.tau_s);
        let d = x - self.mean;
        self.mean += d * a;
        self.var += (d * d - self.var) * a;
        z
    }
}
