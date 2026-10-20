//! Grille de temps tirée de l'audio : un oscillateur au tempo détecté, recalé en douceur sur les kicks.
//!
//! Le détecteur (`beat.ts`) donne un tempo et des kicks, pas une position. La grille avance au tempo détecté ; chaque
//! kick tire la position vers le temps entier le plus proche (une fraction de l'écart seulement : un kick en avance
//! ou un contretemps ne fait pas sauter la grille). L'écart moyen entre kicks et temps entiers donne la confiance.
//! La mesure n'est pas connue (`bar_aligned` faux) : l'opérateur la cale avec `Resync`.

use super::TempoReading;

/// Part de l'écart de phase corrigée à chaque kick.
const KICK_PULL: f64 = 0.3;

/// Lissage de l'écart moyen |kick − temps entier| (par kick).
const ERROR_SMOOTHING: f64 = 0.1;

/// Écart de départ (en temps) : confiance nulle tant que les kicks n'ont pas confirmé la grille.
const INITIAL_ERROR: f64 = 0.25;

/// Confiance maximale de l'audio : une détection reste moins sûre qu'une horloge.
const AUDIO_MAX_CONFIDENCE: f64 = 0.8;

/// Grille de temps de l'audio. Voir le module.
#[derive(Debug, Clone)]
pub struct AudioBeatGrid {
    beat: f64,
    last_s: Option<f64>,
    error: f64,
}

impl Default for AudioBeatGrid {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBeatGrid {
    /// Grille vide.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            beat: 0.0,
            last_s: None,
            error: INITIAL_ERROR,
        }
    }

    /// Oublie la grille (nouvelle source, nouvelle piste).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Avance la grille jusqu'à `now_s` avec le dernier état de l'analyse (`live`, `bpm`, `kick` d'`AudioFeatures`) et
    /// rend la lecture à offrir à l'arbitre. `None` sans signal ou sans tempo détecté : la grille repartira au retour.
    pub fn observe(&mut self, now_s: f64, live: bool, bpm: Option<f32>, kick: bool) -> Option<TempoReading> {
        let Some(bpm) = bpm.filter(|b| live && b.is_finite() && *b > 0.0) else {
            self.reset();
            return None;
        };
        let bpm = f64::from(bpm);
        let first_kick = self.error >= INITIAL_ERROR;
        if let Some(last) = self.last_s {
            self.beat += (now_s - last).max(0.0) * bpm / 60.0;
        }
        self.last_s = Some(now_s);
        if kick {
            let error = self.beat.round() - self.beat;
            if first_kick {
                // Premier kick : il fixe la grille d'un coup.
                self.beat = self.beat.round();
                self.error = INITIAL_ERROR * (1.0 - ERROR_SMOOTHING);
            } else {
                self.beat += KICK_PULL * error;
                self.error += ERROR_SMOOTHING * (error.abs() - self.error);
            }
        }
        Some(TempoReading {
            time_s: now_s,
            beat: self.beat,
            bpm,
            bar_aligned: false,
            confidence: self.confidence(),
        })
    }

    /// Confiance 0..1 : un écart moyen d'un quart de temps l'annule.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        (AUDIO_MAX_CONFIDENCE * (1.0 - self.error / INITIAL_ERROR).clamp(0.0, 1.0)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kicks_on_the_beat_build_confidence() {
        let mut grid = AudioBeatGrid::new();
        assert!(grid.observe(0.0, true, None, false).is_none());
        assert!(grid.observe(0.0, false, Some(120.0), true).is_none());
        let dt = 0.01;
        let mut last = None;
        for step in 0..1_000 {
            let t = 1.0 + f64::from(step) * dt;
            // Kicks toutes les 0,5 s, décalés de 30 ms par rapport au départ de la grille.
            let kick = step % 50 == 3;
            last = grid.observe(t, true, Some(120.0), kick);
        }
        let reading = last.expect("lecture");
        assert!(reading.confidence > 0.6, "{}", reading.confidence);
        // Les kicks tombent sur des temps entiers.
        let at_kick = reading.beat_at(1.0 + 953.0 * dt);
        assert!((at_kick - at_kick.round()).abs() < 0.02, "{at_kick}");
        assert!(!reading.bar_aligned);
    }

    #[test]
    fn offbeat_kick_barely_moves_the_grid() {
        let mut grid = AudioBeatGrid::new();
        let _ = grid.observe(0.0, true, Some(120.0), true);
        for k in 1..20 {
            let _ = grid.observe(f64::from(k) * 0.5, true, Some(120.0), true);
        }
        let before = grid.observe(10.0, true, Some(120.0), false).expect("lecture").beat;
        let after = grid.observe(10.2, true, Some(120.0), true).expect("lecture").beat;
        // 0,2 s = 0,4 temps ; le kick à contretemps ne tire que de 30 % de l'écart.
        assert!((after - before - (0.4 - 0.3 * 0.4)).abs() < 1e-9, "{}", after - before);
    }
}
