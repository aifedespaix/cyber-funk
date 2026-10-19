//! Attente de la musique en pré-intro (portage de `show/wait.ts`).

/// Pré-intro : au moins 45 s avant toute sortie.
pub const PRE_INTRO_MIN: f32 = 45.0;
/// Durée (s) d'énergie soutenue (~8 s) qui déclenche la sortie.
pub const PRE_INTRO_SUSTAIN: f32 = 8.0;
/// Intensité (0..1) à partir de laquelle la musique compte comme « là ».
const PRE_INTRO_ENERGY: f32 = 0.45;

/// Attente de la musique en pré-intro : le temps passé et la durée d'énergie soutenue.
#[derive(Debug, Clone, Default)]
pub struct MusicWait {
    elapsed_s: f32,
    energetic_s: f32,
}

impl MusicWait {
    /// Remet les compteurs à zéro.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Avance de `dt` (s) ; vrai quand la musique est franchement là (après le minimum). `live` : une source audio
    /// est active ; `intensity` : 0..1.
    pub fn update(&mut self, dt: f32, live: bool, intensity: f32) -> bool {
        self.elapsed_s += dt;
        self.energetic_s = if live && intensity >= PRE_INTRO_ENERGY {
            self.energetic_s + dt
        } else {
            (self.energetic_s - dt * 2.0).max(0.0)
        };
        self.elapsed_s >= PRE_INTRO_MIN && self.energetic_s >= PRE_INTRO_SUSTAIN
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 0.1;

    fn run(w: &mut MusicWait, seconds: f32, live: bool, intensity: f32) -> bool {
        let mut done = false;
        for _ in 0..(seconds / DT).round() as u32 {
            done = w.update(DT, live, intensity);
        }
        done
    }

    #[test]
    fn never_expires_before_minimum_even_with_energy() {
        let mut w = MusicWait::default();
        assert!(!run(&mut w, 40.0, true, 1.0));
    }

    #[test]
    fn expires_after_minimum_with_sustained_energy() {
        let mut w = MusicWait::default();
        assert!(run(&mut w, 46.0, true, 0.9));
    }

    #[test]
    fn silence_or_no_source_never_expires() {
        let mut w = MusicWait::default();
        assert!(!run(&mut w, 120.0, false, 1.0));
        assert!(!run(&mut w, 120.0, true, 0.1));
    }

    #[test]
    fn energy_decays_twice_as_fast_and_reset_clears() {
        let mut w = MusicWait::default();
        run(&mut w, 40.0, true, 0.1); // temps passé, pas d'énergie
        run(&mut w, 6.0, true, 0.9); // 6 s d'énergie < 8 s
        assert!(!w.update(DT, true, 0.9));
        run(&mut w, 3.0, true, 0.0); // -6 s
        assert!(!run(&mut w, 4.0, true, 0.9));
        w.reset();
        assert!(!run(&mut w, 10.0, true, 0.9));
    }
}
