//! Gouttes sur l'objectif (`PostFX.setLens` de Sillage) : l'état qui pilote la couche de gouttes de l'étalonnage.

/// Vitesse (par s) à laquelle les gouttes apparaissent quand l'averse monte.
const LENS_RISE: f32 = 0.6;
/// Vitesse (par s) à laquelle elles sèchent : une dizaine de secondes.
const LENS_FALL: f32 = 0.1;

/// Gouttes sur l'objectif : elles apparaissent vite avec l'averse, ne sèchent qu'en une dizaine de secondes, et
/// glissent d'autant plus vite que la voiture va vite.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Lens {
    /// Quantité de gouttes, 0 → 1 (`uLens`).
    pub amount: f32,
    /// Horloge propre aux gouttes (`uRainTime`), en s « de glissement ». En `f64` : sur un set de 6 h, un cumul en
    /// `f32` perdrait les incréments d'une image.
    pub rain_time: f64,
}

impl Lens {
    /// Avance d'une image. `rain` : averse, 0 → 1 ; `speed_ratio` : vitesse rapportée à la vitesse de croisière,
    /// 0 → 1 ; `dt_s` : durée de l'image.
    pub fn update(&mut self, rain: f32, speed_ratio: f32, dt_s: f32) {
        let target = if rain.is_finite() { rain.clamp(0.0, 1.0) } else { 0.0 };
        let rate = if target > self.amount { LENS_RISE } else { LENS_FALL };
        let gap = target - self.amount;
        self.amount += gap.signum() * gap.abs().min(rate * dt_s.max(0.0));
        let speed = if speed_ratio.is_finite() {
            speed_ratio.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.rain_time += f64::from(dt_s.max(0.0) * (0.4 + 1.2 * speed));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_gouttes_viennent_vite_et_sechent_lentement() {
        let mut lens = Lens::default();
        for _ in 0..60 {
            lens.update(1.0, 0.0, 1.0 / 60.0);
        }
        assert!((lens.amount - 0.6).abs() < 1e-3, "{}", lens.amount);
        for _ in 0..60 {
            lens.update(0.0, 0.0, 1.0 / 60.0);
        }
        assert!((lens.amount - 0.5).abs() < 1e-3, "{}", lens.amount);
    }

    #[test]
    fn elles_glissent_plus_vite_quand_on_roule_vite() {
        let mut slow = Lens::default();
        let mut fast = Lens::default();
        slow.update(1.0, 0.0, 1.0);
        fast.update(1.0, 1.0, 1.0);
        assert!((slow.rain_time - 0.4).abs() < 1e-6);
        assert!((fast.rain_time - 1.6).abs() < 1e-6);
    }

    #[test]
    fn des_entrees_non_numeriques_ne_mouillent_pas() {
        let mut lens = Lens::default();
        lens.update(f32::NAN, f32::INFINITY, 1.0);
        assert!(lens.amount.abs() < f32::EPSILON);
        assert!(lens.rain_time.is_finite());
    }
}
