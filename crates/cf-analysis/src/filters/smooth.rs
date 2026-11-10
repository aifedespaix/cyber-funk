//! Lissages : attaque/relâchement, crête avec maintien, One-Euro, borne de pente.

use super::safe_dt;

/// Coefficient `1 − exp(−Δt/τ)`. Une constante de temps nulle ou négative suit l'entrée sans retard.
#[must_use]
pub fn alpha(dt: f32, tau: f32) -> f32 {
    if tau > 0.0 {
        1.0 - (-safe_dt(dt) / tau).exp()
    } else {
        1.0
    }
}

/// Étape 5 : `y += (x − y)·(1 − exp(−Δt/τ))`, avec `τ_a` si `x > y`, sinon `τ_r`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttackRelease {
    /// Constante de temps de montée (s).
    pub attack_s: f32,
    /// Constante de temps de descente (s).
    pub release_s: f32,
    y: f32,
}

impl AttackRelease {
    /// Filtre au repos (sortie 0).
    #[must_use]
    pub const fn new(attack_s: f32, release_s: f32) -> Self {
        Self {
            attack_s,
            release_s,
            y: 0.0,
        }
    }

    /// Sortie courante.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.y
    }

    /// Remet la sortie à `y`.
    pub const fn set(&mut self, y: f32) {
        self.y = y;
    }

    /// Avance de `dt` secondes vers `x`.
    pub fn step(&mut self, x: f32, dt: f32) -> f32 {
        let tau = if x > self.y { self.attack_s } else { self.release_s };
        self.y += (x - self.y) * alpha(dt, tau);
        self.y
    }
}

/// Étape 6 : `p = max(x, p·exp(−Δt/τ_d))`, sans décroissance pendant le maintien `t_h` qui suit chaque nouvelle crête.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakHold {
    /// Constante de temps de décroissance (s).
    pub decay_s: f32,
    /// Durée du maintien (s).
    pub hold_s: f32,
    p: f32,
    hold_left: f32,
}

impl PeakHold {
    /// Crête au repos.
    #[must_use]
    pub const fn new(decay_s: f32, hold_s: f32) -> Self {
        Self {
            decay_s,
            hold_s,
            p: 0.0,
            hold_left: 0.0,
        }
    }

    /// Valeur courante.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.p
    }

    /// Avance de `dt` secondes avec l'entrée `x`.
    pub fn step(&mut self, x: f32, dt: f32) -> f32 {
        let mut dt = safe_dt(dt);
        if x >= self.p {
            self.p = x;
            self.hold_left = self.hold_s.max(0.0);
        } else {
            let held = dt.min(self.hold_left);
            self.hold_left -= held;
            dt -= held;
            if dt > 0.0 && self.decay_s > 0.0 {
                self.p *= (-dt / self.decay_s).exp();
            }
            self.p = self.p.max(x);
        }
        self.p
    }
}

/// Filtre One-Euro (Casiez et al.) pour les signaux continus : lent au repos, rapide sur les fronts.
///
/// Écart à P2 §2.2 : les deux lissages (dérivée à 1 Hz, signal à `f_c`) utilisent `1 − exp(−2π·f·Δt)` plutôt que
/// `1/(1 + 1/(2π·f·Δt))`, qui dépend de la cadence (Euler implicite) ; le reste est inchangé.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OneEuro {
    /// Fréquence de coupure minimale (Hz).
    pub f_min: f32,
    /// Coefficient de vitesse.
    pub beta: f32,
    y: f32,
    dx: f32,
    primed: bool,
}

/// Coupure (Hz) du lissage de la dérivée.
const D_CUTOFF_HZ: f32 = 1.0;

impl OneEuro {
    /// Filtre neuf ; il prend la première valeur reçue.
    #[must_use]
    pub const fn new(f_min: f32, beta: f32) -> Self {
        Self {
            f_min,
            beta,
            y: 0.0,
            dx: 0.0,
            primed: false,
        }
    }

    /// Sortie courante.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.y
    }

    /// Avance de `dt` secondes vers `x`.
    pub fn step(&mut self, x: f32, dt: f32) -> f32 {
        let dt = safe_dt(dt);
        if !self.primed {
            self.primed = true;
            self.y = x;
            return x;
        }
        if dt <= 0.0 {
            return self.y;
        }
        let raw = (x - self.y) / dt;
        self.dx += (raw - self.dx) * (1.0 - (-std::f32::consts::TAU * D_CUTOFF_HZ * dt).exp());
        let cutoff = self.f_min + self.beta * self.dx.abs();
        self.y += (x - self.y) * (1.0 - (-std::f32::consts::TAU * cutoff * dt).exp());
        self.y
    }
}

/// Étape 7 : borne de pente, `y ← y_préc + clamp(y − y_préc, −r↓·Δt, r↑·Δt)` (en unités par seconde).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlewLimiter {
    /// Montée maximale (par seconde).
    pub up: f32,
    /// Descente maximale (par seconde).
    pub down: f32,
    y: f32,
}

impl SlewLimiter {
    /// Limiteur au repos.
    #[must_use]
    pub const fn new(up: f32, down: f32) -> Self {
        Self { up, down, y: 0.0 }
    }

    /// Sortie courante.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.y
    }

    /// Avance de `dt` secondes vers `x`.
    pub fn step(&mut self, x: f32, dt: f32) -> f32 {
        let dt = safe_dt(dt);
        self.y += (x - self.y).clamp(-self.down.max(0.0) * dt, self.up.max(0.0) * dt);
        self.y
    }
}
