//! Analyse d'une piste entière : le même `BeatAnalyzer` que le direct, déroulé sans attendre l'horloge, puis un
//! ajustement de grille sur les kicks trouvés et une recherche du temps 1 des mesures.

use std::sync::atomic::AtomicU64;

use cf_audio::feed::{ANALYSIS_SAMPLE_RATE, HOP, HOP_SECONDS};
use cf_audio::source::SourceKind;
use crossbeam_channel::Receiver;

use super::fit::{Fit, fit_grid};
use super::{GRID_VERSION, SectionSpan, TrackGrid};
use crate::analyzer::{Analyzer, StepContext};
use crate::beat_analyzer::BeatAnalyzer;
use crate::events::{AudioEvent, EventSink};
use crate::features::{AudioFeatures, Section};
use crate::tempo::BEATS_PER_BAR;

/// Confiance maximale d'une grille de fichier : une analyse sur toute la piste vaut mieux que le direct (0,8).
const FILE_MAX_CONFIDENCE: f64 = 0.95;

/// Écart quadratique (en fraction de période) qui annule la confiance.
const CONFIDENCE_RMS_LIMIT: f64 = 0.1;

/// Déroule l'analyse pas à pas sur le PCM livré par blocs, et garde ce qu'il faut pour la grille.
pub(crate) struct Collector {
    analyzer: BeatAnalyzer,
    out: AudioFeatures,
    tx: crossbeam_channel::Sender<AudioEvent>,
    rx: Receiver<AudioEvent>,
    dropped: AtomicU64,
    hop: [f32; HOP],
    filled: usize,
    step: u64,
    kicks: Vec<f64>,
    bpms: Vec<f32>,
    bass: Vec<f32>,
    sections: Vec<SectionSpan>,
}

impl Collector {
    pub(crate) fn new() -> Self {
        let (tx, rx) = crossbeam_channel::bounded(16);
        let mut analyzer = BeatAnalyzer::new();
        // Une démo n'est pas une capture : pas de gain automatique.
        analyzer.set_source(Some(SourceKind::Demo));
        analyzer.reset();
        Self {
            analyzer,
            out: AudioFeatures::NEUTRAL,
            tx,
            rx,
            dropped: AtomicU64::new(0),
            hop: [0.0; HOP],
            filled: 0,
            step: 0,
            kicks: Vec::new(),
            bpms: Vec::new(),
            bass: Vec::new(),
            sections: vec![SectionSpan {
                start_s: 0.0,
                section: Section::default(),
            }],
        }
    }

    /// Ajoute un bloc de PCM mono 48 kHz.
    pub(crate) fn push(&mut self, mut samples: &[f32]) {
        while !samples.is_empty() {
            let take = (HOP - self.filled).min(samples.len());
            self.hop[self.filled..self.filled + take].copy_from_slice(&samples[..take]);
            self.filled += take;
            samples = &samples[take..];
            if self.filled == HOP {
                self.step_once();
                self.filled = 0;
            }
        }
    }

    fn step_once(&mut self) {
        self.step += 1;
        let time_s = self.step as f64 * HOP_SECONDS;
        let ctx = StepContext {
            step: self.step,
            time_s,
            dt_s: HOP_SECONDS as f32,
            sample_rate: ANALYSIS_SAMPLE_RATE,
        };
        let mut sink = EventSink::new(&self.tx, &self.dropped, self.step, time_s);
        self.analyzer.step(&self.hop, &ctx, &mut self.out, &mut sink);
        while self.rx.try_recv().is_ok() {}
        if self.out.kick {
            self.kicks.push(time_s);
        }
        if let Some(bpm) = self.out.bpm {
            self.bpms.push(bpm);
        }
        self.bass.push(self.out.bass);
        if self.out.section_changed {
            self.sections.push(SectionSpan {
                start_s: time_s,
                section: self.out.section,
            });
        }
    }

    /// Termine l'analyse (le dernier pas partiel est jeté : moins de 11 ms) et bâtit la grille.
    /// `None` si la piste n'a pas de tempo exploitable (silence, ambiant).
    pub(crate) fn finish(self, source_hash: u64) -> Option<TrackGrid> {
        let duration_s = self.step as f64 * HOP_SECONDS;
        let hint = median(&self.bpms)?;
        let fit = fit_grid(&self.kicks, f64::from(hint))?;
        let first_bar_s = first_bar(&fit, &self.bass);
        let confidence =
            (FILE_MAX_CONFIDENCE * (1.0 - fit.rms_s / (CONFIDENCE_RMS_LIMIT * fit.period_s)).clamp(0.0, 1.0)) as f32;
        Some(TrackGrid {
            version: GRID_VERSION,
            source_hash,
            duration_s,
            bpm: 60.0 / fit.period_s,
            first_bar_s,
            confidence,
            kicks: fit.kept,
            sections: self.sections,
        })
    }
}

/// Médiane des tempos détectés (insensible aux hésitations du début de piste).
fn median(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f32::total_cmp);
    Some(sorted[sorted.len() / 2])
}

/// Temps 1 des mesures : parmi les quatre positions possibles dans la mesure, celle où les basses sont les plus
/// fortes en moyenne (le temps 1 porte le kick le plus appuyé). Le résultat est ramené dans `(-4 temps, 0]` pour que la
/// position en temps soit positive dès le début de la piste.
fn first_bar(fit: &Fit, bass: &[f32]) -> f64 {
    let bars = BEATS_PER_BAR as usize;
    let mut score = vec![0.0_f64; bars];
    let mut count = vec![0_u32; bars];
    let last_beat = ((bass.len() as f64 * HOP_SECONDS - fit.origin_s) / fit.period_s).floor() as i64;
    for n in 0..=last_beat.max(0) {
        let t = fit.origin_s + n as f64 * fit.period_s;
        let step = (t / HOP_SECONDS).round() as i64 - 1;
        // Pic des basses sur ±2 pas autour du temps : la détection n'est pas exactement alignée.
        let peak = (step - 2..=step + 2)
            .filter_map(|s| usize::try_from(s).ok().and_then(|s| bass.get(s)))
            .fold(0.0_f32, |a, &b| a.max(b));
        let slot = usize::try_from(n.rem_euclid(4)).unwrap_or(0);
        score[slot] += f64::from(peak);
        count[slot] += 1;
    }
    let best = (0..bars)
        .max_by(|&a, &b| {
            let ma = score[a] / f64::from(count[a].max(1));
            let mb = score[b] / f64::from(count[b].max(1));
            // À égalité, le plus petit indice gagne.
            ma.total_cmp(&mb).then(b.cmp(&a))
        })
        .unwrap_or(0);
    let bar_s = BEATS_PER_BAR * fit.period_s;
    let start = fit.origin_s + best as f64 * fit.period_s;
    start - (start / bar_s).ceil() * bar_s
}
