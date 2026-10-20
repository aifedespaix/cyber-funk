//! `SectionTracker` : lit la forme du morceau (portage de `engine/sections.ts`, sans la teinte `mood`, qui ira en J4-08).
//!
//! L'énergie récente comparée à l'énergie des dernières secondes distingue intro/break (calme), corps du morceau
//! (groove) et refrain/drop (sommet). Tous les lissages sont des exponentielles corrigées par le pas de temps `dt` :
//! le résultat est le même à 60 i/s (Sillage, golden) et au pas du thread d'analyse (512 échantillons, 93,75 i/s), à
//! l'échantillonnage près.

use crate::features::Section;

/// Constante de temps (s) de l'énergie rapide.
const FAST_SECONDS: f64 = 1.6;
/// Constante de temps (s) de l'énergie lente.
const SLOW_SECONDS: f64 = 14.0;
/// Constante de temps (s) du lissage de l'intensité (`MOOD_SECONDS` de Sillage).
const MOOD_SECONDS: f64 = 2.5;
/// Délai minimal (s) entre deux changements de section.
pub const MIN_BETWEEN_CHANGES: f64 = 6.0;
/// Seuils des trois sections, avec une marge pour ne pas osciller à la frontière.
const LOW_EDGE: f64 = 0.34;
const HIGH_EDGE: f64 = 0.66;
const HYSTERESIS: f64 = 0.06;
/// Mémoire (s) du plus haut volume récent : un morceau peu compressé atteint quand même le sommet.
const CEILING_SECONDS: f64 = 45.0;
const DEFAULT_CEILING: f64 = 0.52;
const MIN_SPAN: f64 = 0.2;
const MAX_SPAN: f64 = 0.4;
/// Intensité de départ (et après `reset`).
pub const INITIAL_INTENSITY: f64 = 0.4;

/// Suit la forme du morceau à partir du volume global `loud` du détecteur de temps.
#[derive(Debug, Clone)]
pub struct SectionTracker {
    /// 0 (calme) → 1 (sommet), lissé.
    intensity: f64,
    section: Section,
    fast: f64,
    slow: f64,
    ceiling: f64,
    since_change: f64,
}

impl SectionTracker {
    /// Suiveur neuf : section `groove`, intensité 0,4, comme Sillage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            intensity: INITIAL_INTENSITY,
            section: Section::Groove,
            fast: 0.0,
            slow: 0.0,
            ceiling: DEFAULT_CEILING,
            since_change: MIN_BETWEEN_CHANGES,
        }
    }

    /// Intensité lissée, 0 (calme) → 1 (sommet).
    #[must_use]
    pub fn intensity(&self) -> f64 {
        self.intensity
    }

    /// Section courante.
    #[must_use]
    pub fn section(&self) -> Section {
        self.section
    }

    /// Avance d'un pas de `dt` secondes avec le volume `loud` (0..1). Renvoie la nouvelle section si elle vient de
    /// changer, sinon `None`.
    pub fn update(&mut self, loud: f64, dt: f64) -> Option<Section> {
        self.fast += (loud - self.fast) * (1.0 - (-dt / FAST_SECONDS).exp());
        self.slow += (loud - self.slow) * (1.0 - (-dt / SLOW_SECONDS).exp());

        // Le sommet de l'échelle suit le plus haut volume récent : 0,4 pour un morceau très fort (comme avant), moins
        // pour un doux.
        self.ceiling = self.fast.max(self.ceiling * (-dt / CEILING_SECONDS).exp());
        let span = (self.ceiling - 0.12).clamp(MIN_SPAN, MAX_SPAN);
        let absolute = ((self.fast - 0.12) / span).clamp(0.0, 1.0);
        let relative = ((self.fast - self.slow) / 0.15).clamp(-1.0, 1.0) * 0.5 + 0.5;
        let target = absolute * 0.6 + relative * 0.4;
        self.intensity += (target - self.intensity) * (1.0 - (-dt / MOOD_SECONDS).exp());

        self.since_change += dt;
        let next = self.classify(self.intensity);
        if next == self.section || self.since_change < MIN_BETWEEN_CHANGES {
            return None;
        }
        self.section = next;
        self.since_change = 0.0;
        Some(next)
    }

    /// Après un changement de titre ou de source : on repart d'une ambiance neutre.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    fn classify(&self, t: f64) -> Section {
        let h = HYSTERESIS;
        match self.section {
            Section::Calm if t > LOW_EDGE + h => {
                if t > HIGH_EDGE + h {
                    Section::Peak
                } else {
                    Section::Groove
                }
            }
            Section::Calm => Section::Calm,
            Section::Peak if t < HIGH_EDGE - h => {
                if t < LOW_EDGE - h {
                    Section::Calm
                } else {
                    Section::Groove
                }
            }
            Section::Peak => Section::Peak,
            Section::Groove if t < LOW_EDGE - h => Section::Calm,
            Section::Groove if t > HIGH_EDGE + h => Section::Peak,
            Section::Groove => Section::Groove,
        }
    }
}

impl Default for SectionTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pas du thread d'analyse (512 échantillons à 48 kHz).
    const STEP: f64 = 512.0 / 48_000.0;

    /// Applique `loud(t)` pendant `seconds` au pas `dt` ; renvoie les changements `(temps, section)`.
    fn run(
        tracker: &mut SectionTracker,
        start: f64,
        seconds: f64,
        dt: f64,
        loud: impl Fn(f64) -> f64,
    ) -> Vec<(f64, Section)> {
        let mut changes = Vec::new();
        let steps = (seconds / dt).round() as u64;
        for i in 1..=steps {
            let t = start + i as f64 * dt;
            if let Some(section) = tracker.update(loud(t), dt) {
                changes.push((t, section));
            }
        }
        changes
    }

    #[test]
    fn starts_in_groove_at_neutral_intensity() {
        let t = SectionTracker::new();
        assert_eq!(t.section(), Section::Groove);
        assert!((t.intensity() - 0.4).abs() < f64::EPSILON);
    }

    #[test]
    fn steady_loud_signal_reaches_peak() {
        let mut t = SectionTracker::new();
        let changes = run(&mut t, 0.0, 40.0, STEP, |_| 0.7);
        assert_eq!(t.section(), Section::Peak);
        assert_eq!(changes.len(), 1);
        // Le délai minimal ne s'applique pas au premier changement (`sinceChange` démarre à `MIN_BETWEEN_CHANGES`).
        assert!(changes[0].0 < 8.0, "peak à {}", changes[0].0);
    }

    #[test]
    fn silence_falls_to_calm() {
        let mut t = SectionTracker::new();
        run(&mut t, 0.0, 5.0, STEP, |_| 0.0);
        assert_eq!(t.section(), Section::Calm);
        // Le plancher de l'intensité en silence est 0,2 : 0,4 × `relative` neutre (0,5).
        assert!(t.intensity() < 0.3);
    }

    #[test]
    fn progressive_build_goes_calm_groove_peak_in_order() {
        let mut t = SectionTracker::new();
        // Calme 20 s, puis une montée linéaire de 0,05 à 0,75 sur 60 s, puis tenue.
        let loud = |s: f64| {
            if s < 20.0 {
                0.05
            } else {
                (0.05 + (s - 20.0) / 60.0 * 0.7).min(0.75)
            }
        };
        let changes = run(&mut t, 0.0, 110.0, STEP, loud);
        let sections: Vec<Section> = changes.iter().map(|c| c.1).collect();
        assert_eq!(sections, [Section::Calm, Section::Groove, Section::Peak], "{changes:?}");
        // Pas de saut calme → sommet : chaque changement est séparé d'au moins 6 s.
        assert!(
            changes
                .windows(2)
                .all(|w| w[1].0 - w[0].0 >= MIN_BETWEEN_CHANGES - 1e-9)
        );
    }

    #[test]
    fn break_after_peak_returns_to_calm() {
        let mut t = SectionTracker::new();
        run(&mut t, 0.0, 30.0, STEP, |_| 0.7);
        assert_eq!(t.section(), Section::Peak);
        let changes = run(&mut t, 30.0, 40.0, STEP, |_| 0.03);
        assert_eq!(t.section(), Section::Calm, "{changes:?}");
        assert!(changes.iter().all(|c| c.0 - 30.0 >= 0.0));
    }

    #[test]
    fn drop_after_break_jumps_to_peak_within_a_few_seconds() {
        let mut t = SectionTracker::new();
        run(&mut t, 0.0, 30.0, STEP, |_| 0.7);
        run(&mut t, 30.0, 30.0, STEP, |_| 0.03);
        assert_eq!(t.section(), Section::Calm);
        let changes = run(&mut t, 60.0, 20.0, STEP, |_| 0.7);
        let (at, section) = changes.last().copied().expect("un changement attendu");
        assert_eq!(section, Section::Peak);
        assert!(at - 60.0 < 10.0, "sommet atteint {} s après le drop", at - 60.0);
    }

    #[test]
    fn minimum_delay_between_changes_holds() {
        let mut t = SectionTracker::new();
        // Volume qui oscille vite entre fort et nul : les sections ne changent jamais à moins de 6 s d'écart.
        let changes = run(&mut t, 0.0, 120.0, STEP, |s| {
            if ((s / 3.0) as u64).is_multiple_of(2) { 0.8 } else { 0.0 }
        });
        assert!(
            changes
                .windows(2)
                .all(|w| w[1].0 - w[0].0 >= MIN_BETWEEN_CHANGES - 1e-9)
        );
    }

    #[test]
    fn same_transitions_at_60_hz_and_at_analysis_step() {
        let loud = |s: f64| match s {
            s if s < 15.0 => 0.6,
            s if s < 40.0 => 0.04,
            _ => 0.65,
        };
        let a = run(&mut SectionTracker::new(), 0.0, 80.0, 1.0 / 60.0, loud);
        let b = run(&mut SectionTracker::new(), 0.0, 80.0, STEP, loud);
        let kinds = |c: &[(f64, Section)]| c.iter().map(|x| x.1).collect::<Vec<_>>();
        assert_eq!(kinds(&a), kinds(&b));
        for (x, y) in a.iter().zip(&b) {
            assert!((x.0 - y.0).abs() < 0.1, "{x:?} contre {y:?}");
        }
    }

    #[test]
    fn reset_returns_to_initial_state() {
        let mut t = SectionTracker::new();
        run(&mut t, 0.0, 30.0, STEP, |_| 0.7);
        t.reset();
        assert_eq!(t.section(), Section::Groove);
        assert!((t.intensity() - 0.4).abs() < f64::EPSILON);
    }
}
