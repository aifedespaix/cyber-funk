//! Hasard du pilote : un générateur à graine injectée à la place du `Math.random()` de Sillage. Mêmes graine et mêmes
//! entrées, mêmes décisions : les simulations de test sont rejouables.

use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

/// Générateur déterministe du pilote.
#[derive(Debug, Clone)]
pub struct Rand(Pcg32);

impl Rand {
    /// Un générateur de graine `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(Pcg32::seed_from_u64(seed))
    }

    /// Tirage uniforme dans [0, 1) (`Math.random()`).
    pub fn unit(&mut self) -> f64 {
        f64::from(self.0.next_u32()) / 4_294_967_296.0
    }

    /// Tirage uniforme dans [min, max) (`rand(min, max)` de `flight/pilot.ts`).
    pub fn range(&mut self, min: f64, max: f64) -> f64 {
        min + self.unit() * (max - min)
    }

    /// Pile ou face.
    pub fn coin(&mut self) -> bool {
        self.unit() < 0.5
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tirages_bornes_et_rejouables() {
        let mut a = Rand::new(7);
        let mut b = Rand::new(7);
        for _ in 0..1000 {
            let x = a.range(14.0, 26.0);
            assert!((14.0..26.0).contains(&x));
            assert!((x - b.range(14.0, 26.0)).abs() < f64::EPSILON);
        }
    }
}
