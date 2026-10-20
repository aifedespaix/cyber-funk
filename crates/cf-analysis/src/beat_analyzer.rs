//! Analyseur de kicks et de tempo branché sur le thread d'analyse : émulation de l'`AnalyserNode` puis `BeatDetector`.
//!
//! Sur une source de capture (micro, entrée ligne), le spectre passe d'abord par le gain automatique par bande
//! (`BandGain`) pour retrouver l'échelle d'un fichier musical ; le volume `loud` du détecteur alimente ensuite le
//! `SectionTracker` (section, intensité). L'arbitrage des sources de tempo (`tempo_source`, `confidence`, phase,
//! mesure) viendra en J3-03 : ces champs restent à leur valeur neutre ici.

use cf_audio::feed::{ANALYSIS_SAMPLE_RATE, HOP};
use cf_audio::source::SourceKind;

use crate::analyser_emulation::{AnalyserEmulation, BIN_COUNT, FFT_SIZE, smoothing_for_step};
use crate::analyzer::{Analyzer, StepContext};
use crate::band_gain::MicGain;
use crate::beat::{BeatDetector, BeatState};
use crate::events::{AudioEventKind, EventSink};
use crate::features::AudioFeatures;
use crate::sections::SectionTracker;

/// Durée d'une image de référence de Sillage (s) : le gain du micro compte son temps en images à 60 i/s.
const REFERENCE_FRAME_S: f64 = 1.0 / 60.0;

/// Analyseur de kicks et de tempo (`engine/beat.ts` derrière un `AnalyserNode`).
#[derive(Debug)]
pub struct BeatAnalyzer {
    analyser: AnalyserEmulation,
    detector: BeatDetector,
    sections: SectionTracker,
    mic_gain: MicGain,
    /// La source analysée est une capture : le gain automatique s'applique.
    capture: bool,
    /// Spectre après gain, quand `capture` est vrai.
    scaled: [u8; BIN_COUNT],
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
            sections: SectionTracker::new(),
            mic_gain: MicGain::new(ANALYSIS_SAMPLE_RATE),
            capture: false,
            scaled: [0; BIN_COUNT],
        }
    }

    /// Dernier état du détecteur.
    #[must_use]
    pub fn beat_state(&self) -> &BeatState {
        self.detector.state()
    }

    /// Dernier état du suiveur de sections.
    #[must_use]
    pub fn sections(&self) -> &SectionTracker {
        &self.sections
    }

    /// Gains courants du micro (basses, médiums, aigus) ; `[1, 1, 1]` hors capture.
    #[must_use]
    pub fn mic_gains(&self) -> [f64; 3] {
        self.mic_gain.gains()
    }

    /// Spectre en octets vu par le détecteur au dernier pas, après le gain du micro si la source est une capture
    /// (pour les vumètres et le diagnostic).
    #[must_use]
    pub fn spectrum(&self) -> &[u8; BIN_COUNT] {
        if self.capture { &self.scaled } else { self.analyser.bytes() }
    }

    fn publish(&self, state: &BeatState, out: &mut AudioFeatures) {
        out.kick = state.kick;
        out.bass = state.bass as f32;
        out.mid = state.mid as f32;
        out.high = state.high as f32;
        out.loud = state.loud as f32;
        out.pulse = state.pulse as f32;
        out.count = state.count;
        out.beat = state.beat;
        out.bpm = state.bpm;
        out.section = self.sections.section();
        out.intensity = self.sections.intensity() as f32;
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
        let raw = self.analyser.byte_frequency_data();
        let spectrum: &[u8; BIN_COUNT] = if self.capture {
            let frames = f64::from(ctx.dt_s) / REFERENCE_FRAME_S;
            self.mic_gain.apply(raw, &mut self.scaled, frames);
            &self.scaled
        } else {
            raw
        };
        let state = self.detector.update(
            Some(spectrum),
            ctx.sample_rate,
            FFT_SIZE,
            ctx.time_s,
            f64::from(ctx.dt_s),
        );
        let kick = state.kick;
        let changed = self.sections.update(state.loud, f64::from(ctx.dt_s));
        // `state` emprunte le détecteur : on le relit après avoir avancé les sections, qui sont un autre champ.
        let state = *self.detector.state();
        self.publish(&state, out);
        out.section_changed = changed.is_some();
        if kick {
            events.emit(AudioEventKind::Kick);
        }
        if let Some(section) = changed {
            events.emit(AudioEventKind::SectionChanged { section });
        }
    }

    fn idle(&mut self, out: &mut AudioFeatures) {
        self.detector.rest();
        let state = *self.detector.state();
        self.publish(&state, out);
    }

    /// Le gain du micro ne vaut que pour la capture : micro d'un portable, ou entrée ligne (Sillage n'a que le micro,
    /// voir la PR).
    fn set_source(&mut self, source: Option<SourceKind>) {
        self.capture = source == Some(SourceKind::Capture);
    }

    /// Nouvelle source ou nouvelle piste : `beat.reset()` et `sections.reset()` de Sillage (tempo et ambiance oubliés), gains du micro remis à 1 et analyseur neuf, car la
    /// fenêtre et le lissage d'une autre source n'ont plus de sens.
    fn reset(&mut self) {
        self.detector.reset();
        self.analyser.reset();
        self.sections.reset();
        self.mic_gain.reset();
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
