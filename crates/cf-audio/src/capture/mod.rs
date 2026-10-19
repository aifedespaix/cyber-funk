//! Capture d'une entrée audio (micro, entrée ligne d'une carte son) avec reconnexion automatique.
//!
//! ```text
//! périphérique ──callback──▶ FeedWriter ──rtrb──▶ InputFeed ──FeedSink::attach──▶ thread d'analyse
//!                                  ▲
//!        superviseur (thread « cf-capture ») : ouvre par nom, surveille pannes et flux figés, rouvre
//! ```
//!
//! Le superviseur possède le backend et le flux ouvert. Quand le périphérique disparaît (callback d'erreur, ou plus
//! aucun appel du callback pendant [`CaptureConfig::stall_timeout`]), il ferme le flux, passe en
//! [`CaptureState::Waiting`] et retente l'ouverture par nom toutes les [`CaptureConfig::retry_interval`]. Chaque
//! ouverture réussie confie un nouveau flux au thread d'analyse, qui le range à la place de l'ancien : l'analyse ne
//! redémarre jamais. Pendant l'attente, l'analyse publie `live = false` (le flux est en panne ou fermé).

mod backend;
mod cpal_backend;
mod device;
mod supervisor;
mod synthetic;

use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::Sender;

use crate::feed::FeedError;

pub use backend::{FeedSink, InputBackend, OpenedInput};
pub use cpal_backend::CpalBackend;
pub use device::{
    ConfigRange, DeviceSelector, InputDeviceInfo, InputFormat, SampleType, choose_format, resolve_device,
};
pub use synthetic::{Signal, SyntheticBackend, SyntheticBus, SyntheticDevice, SyntheticStream};

use supervisor::{Command, Supervisor};

/// Erreurs de la capture.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// Le système n'a pas de périphérique d'entrée par défaut.
    #[error("aucun périphérique d'entrée par défaut")]
    NoDefaultDevice,
    /// Aucun périphérique ne correspond au nom demandé (débranché, ou mal orthographié).
    #[error("aucun périphérique d'entrée ne correspond à « {0} »")]
    DeviceNotFound(String),
    /// Le fragment de nom désigne plusieurs périphériques.
    #[error("plusieurs périphériques correspondent à « {query} » : {}", candidates.join(", "))]
    AmbiguousDevice {
        /// Nom ou fragment demandé.
        query: String,
        /// Noms complets des périphériques qui correspondent.
        candidates: Vec<String>,
    },
    /// Le périphérique ne propose aucun format géré (i16, u16, i32, f32).
    #[error("« {0} » ne propose aucun format d'entrée utilisable (i16, u16, i32 ou f32)")]
    NoUsableFormat(String),
    /// Erreur de l'hôte audio (`cpal`).
    #[error("hôte audio : {0}")]
    Backend(#[from] cpal::Error),
    /// Erreur de construction du flux vers l'analyse.
    #[error(transparent)]
    Feed(#[from] FeedError),
    /// Le système a refusé de créer un thread.
    #[error("impossible de démarrer le thread « {name} » : {source}")]
    Spawn {
        /// Nom du thread.
        name: &'static str,
        /// Cause système.
        #[source]
        source: std::io::Error,
    },
}

/// Réglages de la capture.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureConfig {
    /// Périphérique à ouvrir, et à rouvrir après un débranchement.
    pub device: DeviceSelector,
    /// Délai entre deux tentatives d'ouverture quand le périphérique est absent.
    pub retry_interval: Duration,
    /// Durée sans aucun appel du callback au-delà de laquelle le flux est considéré comme figé et rouvert (certains
    /// backends ne signalent pas la disparition d'un périphérique : il se tait, simplement).
    pub stall_timeout: Duration,
    /// Taille du ring vers l'analyse (s). Assez pour absorber un accroc du thread d'analyse ; au-delà on jette (et on
    /// compte) plutôt que d'accumuler du retard.
    pub ring_seconds: f32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            device: DeviceSelector::Default,
            retry_interval: Duration::from_millis(500),
            stall_timeout: Duration::from_secs(1),
            ring_seconds: 0.5,
        }
    }
}

/// État de la capture, pour l'interface opérateur.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    /// Le superviseur démarre.
    Starting,
    /// Un flux est ouvert et confié à l'analyse.
    Connected {
        /// Nom du périphérique ouvert.
        device: String,
        /// Format ouvert.
        format: InputFormat,
    },
    /// Pas de flux : périphérique absent ou en panne, nouvelle tentative à intervalle régulier.
    Waiting {
        /// Dernière raison (erreur d'ouverture ou panne du flux).
        reason: String,
        /// Tentatives d'ouverture échouées depuis la dernière réussite.
        attempts: u32,
    },
    /// La capture est arrêtée.
    Stopped,
}

/// Capture supervisée : un thread qui ouvre, surveille et rouvre le périphérique choisi.
///
/// La lâcher arrête la capture (le thread superviseur ferme le flux puis se termine).
#[derive(Debug)]
pub struct Capture {
    commands: Sender<Command>,
    state: Arc<Mutex<CaptureState>>,
    join: Option<JoinHandle<()>>,
}

impl Capture {
    /// Démarre la capture : le superviseur tente une première ouverture tout de suite, puis surveille.
    ///
    /// # Errors
    ///
    /// [`CaptureError::Spawn`] si le thread superviseur ne peut pas être créé. Une ouverture qui échoue n'est pas une
    /// erreur ici : la capture passe en [`CaptureState::Waiting`] et réessaie.
    pub fn start<B: InputBackend, K: FeedSink>(
        backend: B,
        config: CaptureConfig,
        sink: K,
    ) -> Result<Self, CaptureError> {
        let (commands, inbox) = crossbeam_channel::bounded(16);
        let state = Arc::new(Mutex::new(CaptureState::Starting));
        let shared = Arc::clone(&state);
        let join = thread::Builder::new()
            .name("cf-capture".to_owned())
            // Le superviseur est construit sur son thread : le flux ouvert (`cpal::Stream`) n'a pas à être `Send`.
            .spawn(move || Supervisor::new(backend, config, sink, inbox, shared).run())
            .map_err(|source| CaptureError::Spawn {
                name: "cf-capture",
                source,
            })?;
        Ok(Self {
            commands,
            state,
            join: Some(join),
        })
    }

    /// Change de périphérique : ferme le flux courant et ouvre le nouveau (ou attend qu'il apparaisse).
    pub fn select(&self, device: DeviceSelector) {
        // Échec = superviseur déjà terminé (arrêt en cours) : il n'y a plus rien à piloter.
        let _ = self.commands.send(Command::Select(device));
    }

    /// État courant.
    #[must_use]
    pub fn state(&self) -> CaptureState {
        self.state.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // Échec = superviseur déjà terminé ; on rejoint quand même son thread.
        let _ = self.commands.send(Command::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
