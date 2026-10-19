//! Analyseur de kicks et de tempo branché sur le thread d'analyse : émulation de l'`AnalyserNode` puis `BeatDetector`.
//!
//! J1-05 y ajoutera les sections et l'intensité ; l'arbitrage des sources de tempo (`tempo_source`, `confidence`,
//! phase, mesure) viendra en J3-03 : ces champs restent à leur valeur neutre ici.

use cf_audio::feed::HOP;

use crate::analyser_emulation::{AnalyserEmulation, BIN_COUNT, FFT_SIZE, smoothing_for_step};
use crate::analyzer::{Analyzer, StepContext};
use crate::beat::{BeatDetector, BeatState};
use crate::events::{AudioEventKind, EventSink};
use crate::features::AudioFeatures;

/// Analyseur de kicks et de tempo (`engine/beat.ts` derrière un `AnalyserNode`).
#[derive(Debug)]
pub struct BeatAnalyzer {
    analyser: AnalyserEmulation,
    detector: BeatDetector,
}

impl BeatAnalyzer {
    /// Analyseur réglé pour les pas du thread d'analyse (512 échantillons à 48 kHz) : le lissage de l'analyseur est
    /// ramené à la même mémoire en secondes que celui de Sillage à 60 lectures par seconde
    /// (voir [`smoothing_for_step`]).
    #[must_use]
    pub fn new() -> Self {
        Self::with_smoothing(smoothing_for_step(HOP as f64 / 48_000.0))
    }

    /// Analyseur avec une constante de lissage explicite (0,35 pour reproduire Sillage lu à 60 i/s).
    #[must_use]
    pub fn with_smoothing(smoothing: f64) -> Self {
        Self {
            analyser: AnalyserEmulation::new(smoothing),
            detector: BeatDetector::new(),
        }
    }

    /// Dernier état du détecteur.
    #[must_use]
    pub fn beat_state(&self) -> &BeatState {
        self.detector.state()
    }

    /// Spectre en octets du dernier pas (pour les vumètres et le diagnostic).
    #[must_use]
    pub fn spectrum(&self) -> &[u8; BIN_COUNT] {
        self.analyser.bytes()
    }

    fn publish(state: &BeatState, out: &mut AudioFeatures) {
        out.kick = state.kick;
        out.bass = state.bass as f32;
        out.mid = state.mid as f32;
        out.high = state.high as f32;
        out.loud = state.loud as f32;
        out.pulse = state.pulse as f32;
        out.count = state.count;
        out.beat = state.beat;
        out.bpm = state.bpm;
    }
}

impl Default for BeatAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyzer for BeatAnalyzer {
    fn step(&mut self, hop: &[f32; HOP], ctx: &StepContext, out: &mut AudioFeatures, events: &mut EventSink<'_>) {
        self.analyser.push(hop);
        let spectrum = self.analyser.byte_frequency_data();
        let state = self
            .detector
            .update(Some(spectrum), ctx.sample_rate, FFT_SIZE, ctx.time_s, f64::from(ctx.dt_s));
        Self::publish(state, out);
        if state.kick {
            events.emit(AudioEventKind::Kick);
        }
    }

    fn idle(&mut self, out: &mut AudioFeatures) {
        self.detector.rest();
        Self::publish(self.detector.state(), out);
    }

    /// Nouvelle source ou nouvelle piste : `beat.reset()` de Sillage (tempo oublié) et analyseur neuf, car la
    /// fenêtre et le lissage d'une autre source n'ont plus de sens.
    fn reset(&mut self) {
        self.detector.reset();
        self.analyser.reset();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU64;

    use super::*;

    #[test]
    fn silence_publishes_neutral_music_fields() {
        let (tx, _rx) = crossbeam_channel::bounded(8);
        let dropped = AtomicU64::new(0);
        let mut analyzer = BeatAnalyzer::new();
        let mut out = AudioFeatures::NEUTRAL;
        let ctx = StepContext {
            step: 1,
            time_s: 512.0 / 48_000.0,
            dt_s: (512.0_f64 / 48_000.0) as f32,
            sample_rate: 48_000,
        };
        let mut sink = EventSink::new(&tx, &dropped, 1, ctx.time_s);
        analyzer.step(&[0.0; HOP], &ctx, &mut out, &mut sink);
        assert!(!out.kick && out.bpm.is_none() && out.count == 0);
        assert!(out.bass.abs() < f32::EPSILON && out.loud.abs() < f32::EPSILON);
    }
}
