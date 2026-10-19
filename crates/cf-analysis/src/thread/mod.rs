//! Le thread d'analyse à pas fixe.
//!
//! ```text
//! sources ──InputFeed──▶ AnalysisControl::attach ─┐
//! opérateur ─────────────▶ AnalysisControl::select ┤   thread « cf-analysis » (jamais redémarré)
//!                                                  └─▶ un emplacement par sorte de source, une seule analysée
//!                                                      FeedReader (→ 48 kHz) ─▶ pas de 512 ─▶ Analyzer::step
//!                                                      ├─▶ triple buffer : AudioFeatures (le plus récent)
//!                                                      └─▶ canal borné : AudioEvent datés (try_send)
//! ```
//!
//! Les sources inactives restent attachées et sont vidées à chaque tour (« chaudes ») : basculer de l'une à l'autre
//! est immédiat et ne mêle jamais un reste ancien au signal frais. Un flux de capture rouvert après un débranchement
//! remplace simplement le précédent dans son emplacement.

mod runner;
mod stats;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use cf_audio::capture::FeedSink;
use cf_audio::feed::InputFeed;
use cf_audio::source::SourceKind;
use crossbeam_channel::{Receiver, Sender};

use crate::analyzer::Analyzer;
use crate::events::AudioEvent;
use crate::features::AudioFeatures;

pub use stats::{AnalysisStats, AnalysisStatsSnapshot, LATENCY_TARGET};

use runner::Runner;

/// Erreurs du thread d'analyse.
#[derive(Debug, thiserror::Error)]
pub enum AnalysisError {
    /// Le système a refusé de créer le thread.
    #[error("impossible de démarrer le thread d'analyse : {0}")]
    Spawn(#[source] std::io::Error),
}

/// Réglages du thread d'analyse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalysisConfig {
    /// Source analysée au démarrage.
    pub initial_source: Option<SourceKind>,
    /// Durée sans aucun échantillon au-delà de laquelle `live` passe à faux. Assez long pour couvrir les plus gros
    /// blocs livrés par un pilote (quelques dizaines de ms), assez court pour que le monde ralentisse vite.
    pub starve_timeout: Duration,
    /// Attente entre deux tours quand rien n'est prêt. Le callback ne réveille jamais le thread (pas d'appel système
    /// côté temps réel) : c'est ce sondage qui borne la part « attente » de la latence.
    pub poll_interval: Duration,
    /// Capacité du canal d'événements.
    pub event_capacity: usize,
    /// Période des bilans du journal (latence, pertes).
    pub log_interval: Duration,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            initial_source: Some(SourceKind::Capture),
            starve_timeout: Duration::from_millis(200),
            poll_interval: Duration::from_millis(1),
            event_capacity: 256,
            log_interval: Duration::from_secs(2),
        }
    }
}

/// Ordres envoyés au thread d'analyse.
#[derive(Debug)]
enum Command {
    Attach(InputFeed),
    Detach(SourceKind),
    Select(Option<SourceKind>),
    ResetAnalysis,
}

/// Télécommande du thread d'analyse, clonable et utilisable depuis n'importe quel thread (sauf un callback temps
/// réel : l'envoi peut allouer).
#[derive(Debug, Clone)]
pub struct AnalysisControl {
    tx: Sender<Command>,
}

impl AnalysisControl {
    /// Confie un flux : il remplace le flux de la même sorte de source, s'il y en avait un.
    pub fn attach(&self, feed: InputFeed) {
        self.send(Command::Attach(feed));
    }

    /// Retire le flux d'une sorte de source.
    pub fn detach(&self, kind: SourceKind) {
        self.send(Command::Detach(kind));
    }

    /// Choisit la source analysée (`None` : aucune). Bascule à chaud, sans redémarrer le thread ; l'analyseur
    /// repart d'un état neutre.
    pub fn select(&self, kind: Option<SourceKind>) {
        self.send(Command::Select(kind));
    }

    /// Remet l'analyseur à zéro sans changer de source (changement de piste du lecteur de démo).
    pub fn reset_analysis(&self) {
        self.send(Command::ResetAnalysis);
    }

    fn send(&self, command: Command) {
        if self.tx.send(command).is_err() {
            tracing::warn!("analyse : ordre ignoré, le thread d'analyse est arrêté");
        }
    }
}

impl FeedSink for AnalysisControl {
    fn attach(&mut self, feed: InputFeed) {
        Self::attach(self, feed);
    }
}

/// Lecture du dernier `AudioFeatures` publié, sans attente.
pub struct FeaturesReader {
    output: triple_buffer::Output<AudioFeatures>,
}

impl std::fmt::Debug for FeaturesReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FeaturesReader")
            .field("updated", &self.output.updated())
            .finish_non_exhaustive()
    }
}

impl FeaturesReader {
    /// Le plus récent état publié (le même qu'au dernier appel si rien de neuf n'a été publié).
    pub fn latest(&mut self) -> &AudioFeatures {
        self.output.read()
    }

    /// Vrai si un état plus récent que le dernier lu attend.
    #[must_use]
    pub fn updated(&self) -> bool {
        self.output.updated()
    }
}

/// Ce que l'analyse produit, à confier au consommateur (le pont Bevy en J1-06).
#[derive(Debug)]
pub struct AnalysisOutputs {
    /// Dernier état publié.
    pub features: FeaturesReader,
    /// Événements datés (canal borné).
    pub events: Receiver<AudioEvent>,
}

/// Le thread d'analyse. Le lâcher l'arrête et le rejoint.
#[derive(Debug)]
pub struct AnalysisThread {
    control: AnalysisControl,
    stats: Arc<AnalysisStats>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl AnalysisThread {
    /// Démarre le thread d'analyse avec `analyzer`.
    ///
    /// # Errors
    ///
    /// [`AnalysisError::Spawn`] si le thread ne peut pas être créé.
    pub fn spawn<A: Analyzer>(analyzer: A, config: AnalysisConfig) -> Result<(Self, AnalysisOutputs), AnalysisError> {
        let (command_tx, command_rx) = crossbeam_channel::bounded(64);
        let (event_tx, event_rx) = crossbeam_channel::bounded(config.event_capacity.max(1));
        let (publisher, output) = triple_buffer::triple_buffer(&AudioFeatures::NEUTRAL);
        let stats = Arc::new(AnalysisStats::default());
        let stop = Arc::new(AtomicBool::new(false));
        let runner = Runner::new(
            analyzer,
            config,
            command_rx,
            publisher,
            event_tx,
            Arc::clone(&stats),
            Arc::clone(&stop),
        );
        let join = thread::Builder::new()
            .name("cf-analysis".to_owned())
            .spawn(move || runner.run())
            .map_err(AnalysisError::Spawn)?;
        let thread = Self {
            control: AnalysisControl { tx: command_tx },
            stats,
            stop,
            join: Some(join),
        };
        let outputs = AnalysisOutputs {
            features: FeaturesReader { output },
            events: event_rx,
        };
        Ok((thread, outputs))
    }

    /// Une télécommande (à donner à la capture comme [`FeedSink`], au lecteur de démo, à l'interface opérateur).
    #[must_use]
    pub fn control(&self) -> AnalysisControl {
        self.control.clone()
    }

    /// Compteurs du thread.
    #[must_use]
    pub fn stats(&self) -> AnalysisStatsSnapshot {
        self.stats.snapshot()
    }
}

impl Drop for AnalysisThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(join) = self.join.take()
            && join.join().is_err()
        {
            tracing::error!("analyse : le thread d'analyse s'est terminé en paniquant");
        }
    }
}
