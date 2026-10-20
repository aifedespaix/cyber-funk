//! Tap tempo : l'opérateur frappe les temps (`Action::TapTempo`), le tempo est la droite ajustée sur ses frappes.
//!
//! Une série commence à la première frappe, qui est le temps 1 d'une mesure ; elle s'arrête après un silence de
//! [`TAP_RESET_S`] : la frappe suivante en ouvre une nouvelle. Comme pour l'horloge, une régression sur les frappes
//! plutôt qu'une moyenne d'intervalles : une frappe en avance ne décale pas tout le tempo.

use super::TempoEstimate;
use super::fit::{Fit, History};

/// Frappes gardées pour le calcul : deux mesures. Au-delà, l'opérateur corrige une dérive, les anciennes frappes
/// gêneraient.
const TAP_WINDOW: usize = 8;

/// Silence (s) au-delà duquel la frappe suivante ouvre une nouvelle série (sous 30 BPM).
pub const TAP_RESET_S: f64 = 2.0;

/// Frappes plus rapprochées que ce délai (s) : un rebond de bouton, ignoré (au-dessus de 400 BPM).
const DEBOUNCE_S: f64 = 0.15;

/// Bornes du tempo publié (BPM).
const BPM_RANGE: (f64, f64) = (30.0, 300.0);

/// Confiance maximale du tap : une frappe humaine reste moins précise qu'une détection établie.
const TAP_MAX_CONFIDENCE: f64 = 0.6;

/// Estimateur de tap tempo. Voir le module.
#[derive(Debug, Clone)]
pub struct TapTempo {
    taps: History<TAP_WINDOW>,
    /// Frappes de la série en cours, la première comprise.
    series: u32,
    fit: Option<Fit>,
}

impl Default for TapTempo {
    fn default() -> Self {
        Self::new()
    }
}

impl TapTempo {
    /// Aucun tap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            taps: History::new(),
            series: 0,
            fit: None,
        }
    }

    /// Oublie toutes les frappes.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Une frappe à `time_s` (s, horloge monotone de l'application : le `time_s` de la `TimedAction`).
    pub fn tap(&mut self, time_s: f64) {
        match self.taps.last() {
            Some(last) if time_s - last < DEBOUNCE_S => return,
            Some(last) if time_s - last > TAP_RESET_S => {
                self.taps.clear();
                self.series = 0;
            }
            _ => {}
        }
        self.taps.push(time_s);
        self.series += 1;
        if let Some(fit) = self.taps.fit_last(TAP_WINDOW)
            && (BPM_RANGE.0..=BPM_RANGE.1).contains(&(60.0 / fit.interval_s))
        {
            self.fit = Some(fit);
        }
    }

    /// Frappes de la série en cours.
    #[must_use]
    pub fn taps(&self) -> u32 {
        self.series
    }

    /// Tempo tapé (BPM), dès deux frappes. Reste connu après la fin d'une série : c'est le tempo de secours.
    #[must_use]
    pub fn bpm(&self) -> Option<f64> {
        self.fit.map(|f| 60.0 / f.interval_s)
    }

    /// Estimation à offrir à l'arbitre : la dernière frappe ajustée est le temps `série − 1`, la première frappe étant
    /// le temps 1 de la mesure (`bar_aligned`). Le tap n'expire pas : l'arbitre l'extrapole tant qu'aucune source plus
    /// sûre n'est là.
    #[must_use]
    pub fn estimate(&self) -> Option<TempoEstimate> {
        let fit = self.fit?;
        let fill = (fit.count as f64 / TAP_WINDOW as f64).min(1.0);
        let steadiness = (1.0 - fit.rms_s / (0.25 * fit.interval_s)).clamp(0.0, 1.0);
        // La position de la dernière frappe compte dans la série en cours ; si la série vient de repartir sans
        // nouveau tempo (une seule frappe), l'ancien tempo est ancré sur cette frappe.
        let (time_s, beat) = if self.series >= 2 {
            (fit.last_s, f64::from(self.series - 1))
        } else {
            (self.taps.last().unwrap_or(fit.last_s), 0.0)
        };
        Some(TempoEstimate {
            time_s,
            beat,
            bpm: 60.0 / fit.interval_s,
            bar_aligned: true,
            confidence: (TAP_MAX_CONFIDENCE * fill.max(0.25) * steadiness) as f32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_taps_give_tempo_and_downbeat() {
        let mut tap = TapTempo::new();
        assert!(tap.estimate().is_none());
        for k in 0..4 {
            tap.tap(10.0 + 0.5 * f64::from(k));
        }
        let e = tap.estimate().expect("tempo après quatre frappes");
        assert!((e.bpm - 120.0).abs() < 1e-9);
        assert!((e.beat - 3.0).abs() < 1e-12);
        assert!((e.time_s - 11.5).abs() < 1e-9);
        assert!(e.bar_aligned);
        assert!(e.confidence > 0.0 && e.confidence <= 0.6);
    }

    #[test]
    fn early_tap_is_averaged_out() {
        let mut tap = TapTempo::new();
        for (k, offset) in [0.0, 0.02, -0.03, 0.01, 0.0, -0.02, 0.03, 0.0].iter().enumerate() {
            tap.tap(5.0 + 0.5 * k as f64 + offset);
        }
        let bpm = tap.bpm().expect("tempo");
        assert!((bpm - 120.0).abs() < 2.0, "{bpm}");
    }

    #[test]
    fn silence_starts_new_series_and_bounce_is_ignored() {
        let mut tap = TapTempo::new();
        tap.tap(1.0);
        tap.tap(1.05); // rebond
        tap.tap(1.5);
        assert_eq!(tap.taps(), 2);
        tap.tap(10.0);
        assert_eq!(tap.taps(), 1);
        // L'ancien tempo reste publié, ancré sur la nouvelle frappe.
        let e = tap.estimate().expect("tempo conservé");
        assert!((e.bpm - 120.0).abs() < 1e-9);
        assert!((e.time_s - 10.0).abs() < 1e-12 && e.beat.abs() < 1e-12);
        tap.tap(10.4);
        assert!((tap.bpm().expect("nouveau tempo") - 150.0).abs() < 1e-6);
    }
}
