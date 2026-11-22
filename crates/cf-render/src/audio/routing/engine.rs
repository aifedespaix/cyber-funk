//! Le moteur de routage : applique la matrice aux `AudioFeatures` de l'image et remplit [`FilteredSignals`].

use bevy::prelude::Resource;
use cf_analysis::AudioFeatures;
use cf_show::limiter::FlashLimiter;

use super::chain::{Chain, Frame};
use super::matrix::RoutingMatrix;
use super::signals::{FilteredSignals, visual_phase};
use super::trust::{GridTrust, trust_target};
use super::types::Target;

/// Tempo supposé sans analyse.
const FALLBACK_BPM: f32 = 120.0;
/// Confiance au-dessus de laquelle l'horloge de danse passe sur la grille, et en dessous de laquelle elle la quitte :
/// l'écart évite qu'une confiance qui vacille autour d'un seuil unique déclenche de fausses mesures.
const DANCE_ON: f32 = 0.6;
const DANCE_OFF: f32 = 0.4;

/// Le moteur. Les chaînes sont créées à l'ouverture (ou au changement de matrice, hors du show) : l'image n'alloue pas.
#[derive(Resource, Debug, Clone)]
pub struct RoutingEngine {
    chains: Vec<Chain>,
    limiter: FlashLimiter,
    trust: GridTrust,
    /// Phase de mesure de l'image d'avant et temps continu accumulé (voir `FilteredSignals::grid_beat`).
    prev_bar_phase: Option<f32>,
    grid_beat: f64,
    /// Mesures entières écoulées (une par bouclage de la phase de mesure) : ancre `grid_beat` sur le temps 1.
    bars: i64,
    /// L'horloge de danse suit-elle la grille ? (hystérésis `DANCE_ON` / `DANCE_OFF`)
    dance_on_grid: bool,
    /// Décalage appliqué à la source courante pour que l'horloge de danse reste continue au changement de source.
    dance_shift: f64,
    /// Dernière valeur de l'horloge de danse (jamais en recul).
    dance: f64,
}

impl RoutingEngine {
    /// Moteur pour une matrice validée.
    #[must_use]
    pub fn new(matrix: &RoutingMatrix) -> Self {
        Self {
            chains: matrix.routes.iter().map(Chain::new).collect(),
            limiter: FlashLimiter::new(),
            trust: GridTrust::new(),
            prev_bar_phase: None,
            grid_beat: 0.0,
            bars: 0,
            dance_on_grid: false,
            dance_shift: 0.0,
            dance: 0.0,
        }
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

    /// L'horloge de danse : la grille quand elle est fiable, les kicks sinon, continue au changement de source.
    fn step_dance(&mut self, out: &mut FilteredSignals) {
        let kicks = f64::from(out.kick_count);
        let wants_grid = if self.dance_on_grid {
            out.grid_trust >= DANCE_OFF
        } else {
            out.grid_trust >= DANCE_ON
        };
        if wants_grid != self.dance_on_grid {
            self.dance_on_grid = wants_grid;
            self.dance_shift = if wants_grid {
                // Vers la grille : le décalage est un multiple de 4 (le temps 1 reste sur les mesures) et jamais en recul.
                ((self.dance - self.grid_beat) / 4.0).ceil() * 4.0
            } else {
                self.dance - kicks
            };
        }
        let raw = if self.dance_on_grid { self.grid_beat } else { kicks };
        self.dance = self.dance.max(raw + self.dance_shift);
        out.dance = self.dance;
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
        let period_s = 60.0
            / features
                .bpm
                .filter(|b| b.is_finite() && *b > 20.0)
                .unwrap_or(FALLBACK_BPM);
        // La phase de mesure couvre 4 temps ; celle du temps en est la partie fractionnaire.
        let bar_phase = visual_phase(features.phase, latency_s, period_s * 4.0);
        out.bar_phase = bar_phase;
        out.beat_phase = (bar_phase * 4.0).rem_euclid(1.0);
        out.kick_wave.age(dt);
        // Temps continu ancré sur la mesure : `grid_beat = 4 × (mesures bouclées + phase)`. Un bouclage vers l'avant compte
        // une mesure, un bouclage vers l'arrière (correction de grille) en retire une ; la valeur ne recule jamais (une
        // correction vers l'arrière est avalée : le mouvement s'arrête un instant). Le temps 1 tombe ainsi sur les
        // multiples de 4, ce dont `bar_started` a besoin.
        if let Some(prev) = self.prev_bar_phase {
            let delta = bar_phase - prev;
            if delta < -0.5 {
                self.bars += 1;
            } else if delta > 0.5 {
                self.bars -= 1;
            }
        }
        self.prev_bar_phase = Some(bar_phase);
        let anchored = (self.bars as f64 + f64::from(bar_phase)) * 4.0;
        self.grid_beat = self.grid_beat.max(anchored);
        out.grid_beat = self.grid_beat;
        out.kick_beat = features.beat;
        out.transient_accent = features.transient;
        out.onset_accent = features.onset;
        out.hat_accent = features.hat;
        out.grid_trust = self.trust.step(
            dt,
            trust_target(features.tempo_source, features.confidence, features.live),
        );
        self.step_dance(out);

        self.limiter.set_policy(flashes, breathing);
        self.limiter.step(dt);
        out.clear_targets();
        let frame = Frame {
            features,
            dt,
            now,
            bar_phase,
        };
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
