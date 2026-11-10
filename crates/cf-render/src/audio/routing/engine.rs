//! Le moteur de routage : applique la matrice aux `AudioFeatures` de l'image et remplit [`FilteredSignals`].

use cf_analysis::AudioFeatures;
use cf_show::limiter::FlashLimiter;

use super::chain::{Chain, Frame};
use super::matrix::RoutingMatrix;
use super::signals::{FilteredSignals, visual_phase};
use super::types::Target;

/// Tempo supposé sans analyse.
const FALLBACK_BPM: f32 = 120.0;

/// Le moteur. Les chaînes sont créées à l'ouverture (ou au changement de matrice, hors du show) : l'image n'alloue pas.
#[derive(Debug, Clone)]
pub struct RoutingEngine {
    chains: Vec<Chain>,
    limiter: FlashLimiter,
}

impl RoutingEngine {
    /// Moteur pour une matrice validée.
    #[must_use]
    pub fn new(matrix: &RoutingMatrix) -> Self {
        Self { chains: matrix.routes.iter().map(Chain::new).collect(), limiter: FlashLimiter::new() }
    }

    /// Nombre de routes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.chains.len()
    }

    /// Aucune route.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }

    /// Le limiteur des routes qui sont des variations de luminance.
    #[must_use]
    pub const fn limiter(&self) -> &FlashLimiter {
        &self.limiter
    }

    /// Avance d'une image.
    ///
    /// `latency_s` : somme de la latence audio et de la latence de rendu, pour la phase vue à l'écran ;
    /// `flashes` et `breathing` : la politique du show, appliquée au limiteur.
    pub fn update(
        &mut self,
        out: &mut FilteredSignals,
        features: &AudioFeatures,
        dt: f32,
        now: f64,
        latency_s: f32,
        (flashes, breathing): (bool, bool),
    ) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.25) } else { 0.0 };
        let period_s = 60.0 / features.bpm.filter(|b| b.is_finite() && *b > 20.0).unwrap_or(FALLBACK_BPM);
        // La phase de mesure couvre 4 temps ; celle du temps en est la partie fractionnaire.
        let bar_phase = visual_phase(features.phase, latency_s, period_s * 4.0);
        out.bar_phase = bar_phase;
        out.beat_phase = (bar_phase * 4.0).rem_euclid(1.0);
        out.kick_wave.age(dt);

        self.limiter.set_policy(flashes, breathing);
        self.limiter.step(dt);
        out.clear_targets();
        let frame = Frame { features, dt, now, bar_phase };
        for chain in &mut self.chains {
            let value = chain.step(&frame, &mut self.limiter);
            let target: Target = chain.target();
            if value > out.get(target) {
                out.set(target, value);
            }
        }
        // Spectre des fenêtres : le détail par bande, avec la crête de 200 ms de la route `WindowMeter`.
        let fall = (-dt / 0.2).exp();
        for (shown, &now_band) in out.spectrum.iter_mut().zip(features.spectrum.iter()) {
            *shown = now_band.max(*shown * fall);
        }
    }
}
