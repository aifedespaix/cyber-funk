//! Analyse du signal audio, sans dépendance à Bevy.
//!
//! Thread d'analyse à pas fixe (512 échantillons à 48 kHz) : FFT, détection des kicks et du tempo (portage de
//! `beat.ts` de Sillage), sections calme / groove / sommet (`sections.ts`), gain automatique du micro, arbitrage des
//! sources de tempo. Le résultat est publié dans un `AudioFeatures` lu par le reste de l'application.
//!
//! Mise en route (sans Bevy) :
//!
//! ```no_run
//! use cf_analysis::{AnalysisConfig, AnalysisThread, NeutralAnalyzer};
//! use cf_audio::capture::{Capture, CaptureConfig, CpalBackend};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let (analysis, mut outputs) = AnalysisThread::spawn(NeutralAnalyzer, AnalysisConfig::default())?;
//! let _capture = Capture::start(CpalBackend::new(), CaptureConfig::default(), analysis.control())?;
//! let features = outputs.features.latest();
//! let _ = (features.live, features.time_s);
//! # Ok(())
//! # }
//! ```

mod analyzer;
mod events;
mod features;
mod thread;

pub use analyzer::{Analyzer, NeutralAnalyzer, StepContext};
pub use events::{AudioEvent, AudioEventKind, EventSink};
pub use features::{AudioFeatures, Section, TempoSource};
pub use thread::{
    AnalysisConfig, AnalysisControl, AnalysisError, AnalysisOutputs, AnalysisStats, AnalysisStatsSnapshot,
    AnalysisThread, FeaturesReader, LATENCY_TARGET,
};
