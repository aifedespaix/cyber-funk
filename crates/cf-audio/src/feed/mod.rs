//! Flux d'entrée vers l'analyse : un ring `rtrb` d'échantillons mono par source, rempli par un callback temps réel
//! et vidé par le thread d'analyse.
//!
//! ```text
//! callback (cpal, synthétique, lecteur de démo)              thread d'analyse
//!   FeedWriter::push_interleaved ──rtrb<f32>──▶ InputFeed ─▶ FeedReader ─▶ pas de 512 échantillons à 48 kHz
//!                                └─rtrb<Mark>─┘ (horodatage de chaque bloc, pour mesurer la latence)
//! ```
//!
//! Le côté écriture ([`FeedWriter`]) n'alloue pas, ne verrouille pas, ne journalise pas et ne fait aucune I/O : il
//! convertit le format, mixe en mono, copie dans le ring et incrémente des compteurs atomiques. Si le ring est plein,
//! les échantillons en trop sont jetés et comptés ([`FeedStats::dropped`]), jamais attendus.

mod reader;
mod sample;
mod stats;

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use assert_no_alloc::assert_no_alloc;

use crate::source::SourceKind;

pub use reader::{FeedReader, HopStatus};
pub use sample::{InputSample, mono_frame};
pub use stats::{FaultReporter, FeedFault, FeedStats};

/// Fréquence d'échantillonnage de l'analyse (Hz). Toute source est rééchantillonnée vers cette fréquence.
pub const ANALYSIS_SAMPLE_RATE: u32 = 48_000;

/// Taille du pas d'analyse (échantillons à [`ANALYSIS_SAMPLE_RATE`]) : 512 / 48 000 ≈ 10,7 ms.
pub const HOP: usize = 512;

/// Durée d'un pas d'analyse (s).
pub const HOP_SECONDS: f64 = HOP as f64 / ANALYSIS_SAMPLE_RATE as f64;

/// Capacité du ring des marques (une par appel du callback). Au rythme le plus rapide raisonnable (un appel par
/// milliseconde), elle couvre une seconde de retard du thread d'analyse ; au-delà on perd des mesures de latence,
/// jamais d'audio.
const MARKS_CAPACITY: usize = 1024;

/// Erreurs de construction ou de lecture d'un flux d'entrée.
#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    /// Fréquence d'échantillonnage nulle.
    #[error("fréquence d'échantillonnage invalide : {0} Hz")]
    InvalidSampleRate(u32),
    /// Capacité de ring nulle.
    #[error("capacité de ring nulle")]
    ZeroCapacity,
    /// Le rééchantillonneur n'a pas pu être construit pour ce couple de fréquences.
    #[error("rééchantillonneur {from} Hz → 48 kHz : {source}")]
    Resampler {
        /// Fréquence de la source (Hz).
        from: u32,
        /// Cause renvoyée par `rubato`.
        #[source]
        source: rubato::ResamplerConstructionError,
    },
    /// Erreur pendant le rééchantillonnage (tailles de tampons incohérentes : bogue, pas un cas d'exploitation).
    #[error("rééchantillonnage : {0}")]
    Resample(#[from] rubato::ResampleError),
    /// Tampon du rééchantillonneur trop petit pour le nombre de trames demandé (bogue, pas un cas d'exploitation).
    #[error("tampon de rééchantillonnage trop petit pour {frames} trames")]
    BufferSize {
        /// Trames demandées.
        frames: usize,
    },
}

/// Horodatage d'un bloc écrit par le callback : `end` est le nombre total d'échantillons écrits après ce bloc
/// (borne exclusive), `at` l'instant où le callback a reçu le bloc.
#[derive(Debug, Clone, Copy)]
struct Mark {
    end: u64,
    at: Instant,
}

/// Côté écriture d'un flux, à déplacer dans le callback temps réel.
///
/// Garanties de [`FeedWriter::push_interleaved`] : aucune allocation (vérifié par `assert_no_alloc` dans les tests,
/// et à l'exécution en debug si l'application installe son allocateur), aucun verrou (anneaux `rtrb` et atomiques
/// uniquement), aucun journal, aucune I/O. La seule lecture d'horloge est `Instant::now()`, monotone et non
/// bloquante (vDSO sous Linux, `mach_absolute_time` sous macOS, `QueryPerformanceCounter` sous Windows).
pub struct FeedWriter {
    samples: rtrb::Producer<f32>,
    marks: rtrb::Producer<Mark>,
    written: u64,
    stats: Arc<FeedStats>,
}

impl fmt::Debug for FeedWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeedWriter")
            .field("written", &self.written)
            .field("free_slots", &self.samples.slots())
            .finish_non_exhaustive()
    }
}

impl FeedWriter {
    /// Pousse un bloc entrelacé de `channels` canaux : conversion en f32, mixage mono, copie dans le ring.
    ///
    /// À appeler depuis le callback temps réel. Une source en pause (lecteur de démo arrêté) ne doit rien pousser :
    /// c'est l'absence d'échantillons qui fait passer `live` à faux côté analyse.
    pub fn push_interleaved<S: InputSample>(&mut self, data: &[S], channels: usize) {
        assert_no_alloc(|| self.push_frames(data, channels));
    }

    fn push_frames<S: InputSample>(&mut self, data: &[S], channels: usize) {
        let at = Instant::now();
        let channels = channels.max(1);
        let frames = data.len() / channels;
        let room = frames.min(self.samples.slots());
        self.stats.callbacks.fetch_add(1, Ordering::Relaxed);
        if room < frames {
            self.stats.dropped.fetch_add((frames - room) as u64, Ordering::Relaxed);
        }
        if room == 0 {
            return;
        }
        // La marque part avant les échantillons : quand le lecteur voit les échantillons (acquire sur le ring audio),
        // la marque écrite plus tôt par ce même thread lui est forcément visible. Ring des marques plein : on perd
        // une mesure de latence, rien d'autre ; l'erreur est volontairement ignorée.
        let _ = self.marks.push(Mark {
            end: self.written + room as u64,
            at,
        });
        if let Ok(chunk) = self.samples.write_chunk_uninit(room) {
            let pushed = chunk.fill_from_iter(data.chunks_exact(channels).map(mono_frame));
            self.written += pushed as u64;
        }
        self.stats.written.store(self.written, Ordering::Relaxed);
    }

    /// Place libre dans le ring (échantillons mono).
    #[must_use]
    pub fn free_slots(&self) -> usize {
        self.samples.slots()
    }

    /// Un rapporteur de pannes pour le callback d'erreur du backend.
    #[must_use]
    pub fn fault_reporter(&self) -> FaultReporter {
        FaultReporter {
            stats: Arc::clone(&self.stats),
        }
    }

    /// Compteurs partagés de ce flux.
    #[must_use]
    pub fn stats(&self) -> &Arc<FeedStats> {
        &self.stats
    }
}

/// Côté lecture d'un flux, à confier au thread d'analyse (voir [`FeedReader`]).
pub struct InputFeed {
    kind: SourceKind,
    sample_rate: u32,
    samples: rtrb::Consumer<f32>,
    marks: rtrb::Consumer<Mark>,
    stats: Arc<FeedStats>,
}

impl fmt::Debug for InputFeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputFeed")
            .field("kind", &self.kind)
            .field("sample_rate", &self.sample_rate)
            .field("pending", &self.samples.slots())
            .finish_non_exhaustive()
    }
}

impl InputFeed {
    /// Sorte de source qui alimente ce flux.
    #[must_use]
    pub fn kind(&self) -> SourceKind {
        self.kind
    }

    /// Fréquence d'échantillonnage du flux (Hz), avant rééchantillonnage.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Compteurs partagés de ce flux.
    #[must_use]
    pub fn stats(&self) -> &Arc<FeedStats> {
        &self.stats
    }

    /// Vrai quand le côté écriture a été lâché (flux fermé) : plus rien n'arrivera au-delà de ce qui reste.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.samples.is_abandoned()
    }
}

/// Crée un flux : le côté écriture pour le callback, le côté lecture pour l'analyse.
///
/// `capacity` est la taille du ring en échantillons mono (voir [`capacity_for`]). C'est ici, hors temps réel, que
/// se font toutes les allocations du flux.
///
/// # Errors
///
/// [`FeedError::InvalidSampleRate`] si `sample_rate` est nul, [`FeedError::ZeroCapacity`] si `capacity` est nulle.
pub fn feed(kind: SourceKind, sample_rate: u32, capacity: usize) -> Result<(FeedWriter, InputFeed), FeedError> {
    if sample_rate == 0 {
        return Err(FeedError::InvalidSampleRate(sample_rate));
    }
    if capacity == 0 {
        return Err(FeedError::ZeroCapacity);
    }
    let (samples_tx, samples_rx) = rtrb::RingBuffer::new(capacity);
    let (marks_tx, marks_rx) = rtrb::RingBuffer::new(MARKS_CAPACITY);
    let stats = Arc::new(FeedStats::default());
    let writer = FeedWriter {
        samples: samples_tx,
        marks: marks_tx,
        written: 0,
        stats: Arc::clone(&stats),
    };
    let input = InputFeed {
        kind,
        sample_rate,
        samples: samples_rx,
        marks: marks_rx,
        stats,
    };
    Ok((writer, input))
}

/// Capacité de ring (échantillons mono) couvrant `seconds` secondes à `sample_rate`, au moins un pas d'analyse.
#[must_use]
pub fn capacity_for(sample_rate: u32, seconds: f32) -> usize {
    ((f64::from(sample_rate) * f64::from(seconds.max(0.0))).ceil() as usize).max(HOP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_parameters() {
        assert!(matches!(
            feed(SourceKind::Capture, 0, 16),
            Err(FeedError::InvalidSampleRate(0))
        ));
        assert!(matches!(
            feed(SourceKind::Capture, 48_000, 0),
            Err(FeedError::ZeroCapacity)
        ));
    }

    #[test]
    fn push_mixes_to_mono_and_counts() {
        let (mut writer, mut input) = feed(SourceKind::Capture, 48_000, 16).unwrap();
        writer.push_interleaved(&[i16::MAX, i16::MAX, 0, 0, i16::MIN, i16::MIN], 2);
        assert_eq!(input.stats().written(), 3);
        assert_eq!(input.stats().callbacks(), 1);
        assert_eq!(input.stats().dropped(), 0);
        let mut out = [0.0_f32; 3];
        let (popped, rest) = input.samples.pop_partial_slice(&mut out);
        assert_eq!(popped.len(), 3);
        assert_eq!(rest.len(), 0);
        assert!((out[0] - 32_767.0 / 32_768.0).abs() < 1e-6);
        assert!(out[1].abs() < 1e-6);
        assert!((out[2] + 1.0).abs() < 1e-6);
        let mark = input.marks.pop().unwrap();
        assert_eq!(mark.end, 3);
    }

    #[test]
    fn full_ring_drops_and_counts_instead_of_blocking() {
        let (mut writer, input) = feed(SourceKind::Capture, 48_000, 4).unwrap();
        writer.push_interleaved(&[0.1_f32; 6], 1);
        assert_eq!(input.stats().written(), 4);
        assert_eq!(input.stats().dropped(), 2);
        writer.push_interleaved(&[0.1_f32; 3], 1);
        assert_eq!(input.stats().written(), 4);
        assert_eq!(input.stats().dropped(), 5);
        assert_eq!(input.stats().callbacks(), 2);
    }

    #[test]
    fn partial_trailing_frame_is_ignored() {
        let (mut writer, input) = feed(SourceKind::Capture, 48_000, 16).unwrap();
        writer.push_interleaved(&[0.5_f32, 0.5, 0.5], 2);
        assert_eq!(input.stats().written(), 1);
    }

    #[test]
    fn first_fault_wins() {
        let (writer, input) = feed(SourceKind::Capture, 48_000, 16).unwrap();
        let reporter = writer.fault_reporter();
        assert_eq!(input.stats().fault(), None);
        reporter.report(FeedFault::DeviceLost);
        reporter.report(FeedFault::Backend);
        reporter.report_xrun();
        assert_eq!(input.stats().fault(), Some(FeedFault::DeviceLost));
        assert_eq!(input.stats().xruns(), 1);
    }

    #[test]
    fn closing_the_writer_is_visible_to_the_reader() {
        let (writer, input) = feed(SourceKind::Demo, 44_100, 16).unwrap();
        assert!(!input.is_closed());
        drop(writer);
        assert!(input.is_closed());
    }

    #[test]
    fn capacity_covers_at_least_one_hop() {
        assert_eq!(capacity_for(48_000, 0.5), 24_000);
        assert_eq!(capacity_for(44_100, 0.0), HOP);
    }
}
