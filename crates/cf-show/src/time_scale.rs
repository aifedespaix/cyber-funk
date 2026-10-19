//! Facteur de temps du monde, « bullet-time » (portage de `show/time-scale.ts`).

/// Rapidité (s) avec laquelle le monde ralentit, puis (s) avec laquelle il reprend son allure.
const SLOW_SECONDS: f32 = 0.12;
const RELEASE_SECONDS: f32 = 0.6;
const MIN_SCALE: f32 = 0.1;

/// Facteur de temps du monde (bullet-time). On demande un ralenti pour une durée ; le facteur y descend vite, le
/// tient, puis remonte progressivement à 1 (jamais d'à-coup). Une nouvelle demande plus lente prend le pas sur une
/// en cours.
#[derive(Debug, Clone)]
pub struct TimeScale {
    /// Facteur courant, dans `0.1..=1` (1 = allure normale).
    pub value: f32,
    target: f32,
    /// Durée (s) de maintien restante.
    hold: f32,
}

impl Default for TimeScale {
    fn default() -> Self {
        Self {
            value: 1.0,
            target: 1.0,
            hold: 0.0,
        }
    }
}

impl TimeScale {
    /// Demande un ralenti `scale` (borné à `0.1..=1`) tenu `seconds` (s).
    pub fn request(&mut self, scale: f32, seconds: f32) {
        let wanted = scale.clamp(MIN_SCALE, 1.0);
        if self.hold > 0.0 && wanted > self.target {
            return;
        }
        self.target = wanted;
        self.hold = seconds.max(0.0);
    }

    /// Avance de `dt` (s) et renvoie le facteur courant.
    pub fn update(&mut self, dt: f32) -> f32 {
        if self.hold > 0.0 {
            self.hold -= dt;
            self.value += (self.target - self.value) * (1.0 - (-dt / SLOW_SECONDS).exp());
        } else {
            self.target = 1.0;
            self.value += (1.0 - self.value) * (1.0 - (-dt / RELEASE_SECONDS).exp());
            if self.value > 0.998 {
                self.value = 1.0;
            }
        }
        self.value
    }

    /// Retour à l'allure normale, sans transition.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn run(ts: &mut TimeScale, seconds: f32) {
        for _ in 0..(seconds / DT) as u32 {
            ts.update(DT);
        }
    }

    #[test]
    fn slows_then_returns_to_exactly_one() {
        let mut ts = TimeScale::default();
        ts.request(0.2, 1.0);
        run(&mut ts, 0.8);
        assert!(ts.value < 0.25, "{}", ts.value);
        run(&mut ts, 0.5); // maintien écoulé
        assert!(ts.value > 0.2);
        run(&mut ts, 6.0);
        assert!((ts.value - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn slower_request_overrides_but_faster_does_not() {
        let mut ts = TimeScale::default();
        ts.request(0.3, 2.0);
        ts.request(0.8, 5.0); // plus rapide : ignorée
        run(&mut ts, 1.0);
        assert!(ts.value < 0.35);
        ts.request(0.1, 2.0); // plus lente : prend le pas
        run(&mut ts, 1.0);
        assert!(ts.value < 0.15);
    }

    #[test]
    fn scale_is_clamped_and_reset_works() {
        let mut ts = TimeScale::default();
        ts.request(0.0, 10.0);
        run(&mut ts, 3.0);
        assert!(ts.value >= MIN_SCALE - 1e-3);
        ts.reset();
        assert!((ts.value - 1.0).abs() < f32::EPSILON);
        assert!((ts.update(DT) - 1.0).abs() < f32::EPSILON);
    }
}
