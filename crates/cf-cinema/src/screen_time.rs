//! Temps d'écran de chaque plan (`cinema/screen-time.ts`), pondéré par l'ancienneté. Sert à plafonner un plan
//! (l'habitacle) à une part du temps, en comptant aussi le plan en cours.
//!
//! Sillage indexait par id dans une `Map` ; ici par rang dans le catalogue, dans un tableau dimensionné une fois.

/// Mémoire (s) de la part d'écran : les vieux plans pèsent de moins en moins.
const DECAY_SECONDS: f64 = 900.0;

/// Temps d'écran par plan du catalogue.
#[derive(Debug, Clone, Default)]
pub struct ScreenTime {
    seconds: Vec<f64>,
    total: f64,
    id: Option<usize>,
    since: f64,
}

impl ScreenTime {
    /// Pour un catalogue de `shots` plans.
    #[must_use]
    pub fn new(shots: usize) -> Self {
        Self {
            seconds: vec![0.0; shots],
            ..Self::default()
        }
    }

    /// Oublie tout.
    pub fn reset(&mut self) {
        self.seconds.fill(0.0);
        self.total = 0.0;
        self.id = None;
        self.since = 0.0;
    }

    /// Le plan `id` prend l'écran à l'instant `now` ; le précédent est clos.
    pub fn switch_to(&mut self, id: usize, now: f64) {
        if let Some(previous) = self.id {
            let duration = (now - self.since).max(0.0);
            let k = (-duration / DECAY_SECONDS).exp();
            for value in &mut self.seconds {
                *value *= k;
            }
            self.total *= k;
            if let Some(value) = self.seconds.get_mut(previous) {
                *value += duration;
            }
            self.total += duration;
        }
        self.id = Some(id);
        self.since = now;
    }

    /// Part d'écran de `id`, 0 → 1, plan en cours compris (0 tant qu'on n'a rien vu).
    #[must_use]
    pub fn share(&self, id: usize, now: f64) -> f64 {
        let open = if self.id.is_some() {
            (now - self.since).max(0.0)
        } else {
            0.0
        };
        let total = self.total + open;
        if total < 1.0 {
            return 0.0;
        }
        let own = self.seconds.get(id).copied().unwrap_or(0.0) + if self.id == Some(id) { open } else { 0.0 };
        own / total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_d_ecran_avec_le_plan_en_cours() {
        let mut screen = ScreenTime::new(2);
        assert!(screen.share(0, 0.0).abs() < 1e-12);
        screen.switch_to(0, 0.0);
        screen.switch_to(1, 10.0);
        // 10 s du plan 0 (amorties de e^(−10/900)), puis 10 s ouvertes du plan 1.
        let k = (-10.0_f64 / 900.0).exp();
        let expected = 10.0 / (10.0 + 10.0);
        assert!((screen.share(1, 20.0) - expected).abs() < 1e-12);
        assert!((screen.share(0, 20.0) - 10.0 / 20.0).abs() < 1e-12);
        screen.switch_to(0, 20.0);
        let total = 10.0 * k + 10.0;
        assert!((screen.share(1, 20.0) - 10.0 / total).abs() < 1e-9);
    }
}
