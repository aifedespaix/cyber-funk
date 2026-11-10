//! Ressort à amortissement critique et sous-amorti, en intégration exacte (stable pour tout `Δt`).

use super::safe_dt;

/// Un ressort suivant une cible `x` supposée constante pendant l'image.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spring {
    y: f32,
    v: f32,
}

impl Spring {
    /// Ressort au repos en 0.
    #[must_use]
    pub const fn new() -> Self {
        Self { y: 0.0, v: 0.0 }
    }

    /// Position courante.
    #[must_use]
    pub const fn value(&self) -> f32 {
        self.y
    }

    /// Vitesse courante.
    #[must_use]
    pub const fn velocity(&self) -> f32 {
        self.v
    }

    /// Place le ressort en `y`, vitesse nulle.
    pub const fn set(&mut self, y: f32) {
        self.y = y;
        self.v = 0.0;
    }

    #[allow(
        clippy::many_single_char_names,
        reason = "noms de P2 §2.2 (j, e, u, w), gardés pour la lecture croisée"
    )]
    /// Amortissement critique, pulsation `ω = 2π·f_n`.
    pub fn step_critical(&mut self, x: f32, f_n: f32, dt: f32) -> f32 {
        let dt = safe_dt(dt);
        let w = std::f32::consts::TAU * f_n.max(0.0);
        let j = self.y - x;
        let e = (-w * dt).exp();
        let u = (self.v + w * j) * dt;
        self.y = x + (j + u) * e;
        self.v = (self.v - w * u) * e;
        self.y
    }

    #[allow(
        clippy::many_single_char_names,
        reason = "noms de P2 §2.2 (E, C, S, b, j, w), gardés pour la lecture croisée"
    )]
    /// Ressort sous-amorti `ressort(x; f_n, ζ)`. `ζ ≥ 1` retombe sur l'amortissement critique.
    pub fn step(&mut self, x: f32, f_n: f32, zeta: f32, dt: f32) -> f32 {
        if zeta >= 0.999 || f_n <= 0.0 {
            return self.step_critical(x, f_n, dt);
        }
        let zeta = zeta.max(0.0);
        let dt = safe_dt(dt);
        let w = std::f32::consts::TAU * f_n;
        let wd = w * (1.0 - zeta * zeta).sqrt();
        let j = self.y - x;
        let e = (-zeta * w * dt).exp();
        let (s, c) = (wd * dt).sin_cos();
        let b = (self.v + zeta * w * j) / wd;
        self.y = x + e * (j * c + b * s);
        self.v = e * ((b * wd - zeta * w * j) * c - (j * wd + zeta * w * b) * s);
        self.y
    }
}
