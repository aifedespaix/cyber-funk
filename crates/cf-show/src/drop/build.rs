//! Détection d'une montée (portage de `show/build.ts`).

/// Écart d'intensité (0..1) au-dessus de la moyenne lente qui compte comme « ça monte ».
const BUILD_GAP: f32 = 0.1;
/// Durée (s) de montée soutenue avant de la reconnaître.
const BUILD_SECONDS: f32 = 3.0;
/// Délai (s) minimal avant de réarmer la détection après une montée.
const BUILD_REARM: f32 = 25.0;
/// Constante de temps (s) de la moyenne lente.
const REFERENCE_SECONDS: f32 = 6.0;
/// Au-dessus de cette intensité on est déjà au sommet : ce n'est plus une montée.
const CEILING: f32 = 0.9;
/// Valeur de départ de la moyenne lente.
const REFERENCE_START: f32 = 0.4;

/// Détecte la montée d'un morceau : l'énergie dépasse sa moyenne lente depuis quelques secondes. C'est la même
/// logique que celle de `cinema/cues.ts` (cue « build ») de Sillage, reprise ici pour que le spectacle n'en dépende
/// pas ; les seuils sont identiques et à garder alignés.
#[derive(Debug, Clone)]
pub struct BuildDetector {
    reference: f32,
    rising_s: f32,
    since_s: f32,
    armed: bool,
}

impl Default for BuildDetector {
    fn default() -> Self {
        Self {
            reference: REFERENCE_START,
            rising_s: 0.0,
            since_s: BUILD_REARM,
            armed: true,
        }
    }
}

impl BuildDetector {
    /// Remet la détection à l'état de départ (nouveau set).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Vrai pendant une seule image : une montée vient d'être reconnue (une fois par épisode). `intensity` : 0..1 ;
    /// `dt` : durée (s) de l'image.
    pub fn update(&mut self, intensity: f32, dt: f32) -> bool {
        self.reference += (intensity - self.reference) * (1.0 - (-dt / REFERENCE_SECONDS).exp());
        let gap = intensity - self.reference;
        self.since_s += dt;
        if self.armed && gap > BUILD_GAP && intensity < CEILING {
            self.rising_s += dt;
        } else {
            self.rising_s = (self.rising_s - dt * 2.0).max(0.0);
        }
        if self.armed && self.rising_s > BUILD_SECONDS {
            self.armed = false;
            self.rising_s = 0.0;
            self.since_s = 0.0;
            return true;
        }
        if !self.armed && self.since_s > BUILD_REARM && gap < 0.03 {
            self.armed = true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Rejoue `seconds` à intensité constante et renvoie le temps (s) de la première détection, s'il y en a une.
    fn run(d: &mut BuildDetector, seconds: f32, intensity: f32) -> Option<f32> {
        let mut found = None;
        for i in 0..(seconds / DT) as u32 {
            if d.update(intensity, DT) && found.is_none() {
                found = Some(i as f32 * DT);
            }
        }
        found
    }

    #[test]
    fn steady_music_is_not_a_build() {
        let mut d = BuildDetector::default();
        assert_eq!(run(&mut d, 120.0, 0.4), None);
    }

    #[test]
    fn jump_above_average_is_recognised_once() {
        let mut d = BuildDetector::default();
        run(&mut d, 30.0, 0.3);
        let found = run(&mut d, 10.0, 0.7).expect("montée reconnue");
        assert!((3.0..5.0).contains(&found), "{found}");
        // Une seule fois par épisode, même si la montée dure.
        assert_eq!(run(&mut d, 10.0, 0.75), None);
    }

    #[test]
    fn ceiling_is_not_a_build() {
        let mut d = BuildDetector::default();
        run(&mut d, 30.0, 0.2);
        assert_eq!(run(&mut d, 10.0, 0.95), None);
    }

    #[test]
    fn rearms_after_the_energy_settles() {
        let mut d = BuildDetector::default();
        run(&mut d, 30.0, 0.3);
        assert!(run(&mut d, 6.0, 0.7).is_some());
        run(&mut d, 40.0, 0.3); // retombée : réarmement
        assert!(run(&mut d, 6.0, 0.7).is_some());
    }

    #[test]
    fn reset_restores_the_start_state() {
        let mut d = BuildDetector::default();
        run(&mut d, 30.0, 0.3);
        assert!(run(&mut d, 6.0, 0.7).is_some());
        d.reset();
        run(&mut d, 30.0, 0.3);
        assert!(run(&mut d, 6.0, 0.7).is_some());
    }
}
