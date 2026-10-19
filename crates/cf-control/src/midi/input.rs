//! L'entrée MIDI vue de l'application : une poignée sur le superviseur de ports et le canal d'événements.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use super::supervisor::{Command, Supervisor};
use super::{MidiBackend, MidiEvent, MidiStats, MidirBackend, match_port_name};
use crate::error::{Error, Result};
use crate::raw::PortId;

/// Période de scrutation des ports (skill `midi-control`) : un contrôleur branché ou rebranché est repris en moins de
/// ce délai.
pub const DEFAULT_SCAN_PERIOD: Duration = Duration::from_secs(2);
/// Capacité du canal entre les callbacks et l'application, en éléments (≈ 100 Ko). Un contrôleur DJ très bavard
/// (molettes, horloge) produit quelques centaines de messages par seconde : cela tient plusieurs secondes de retard
/// de lecture.
pub const DEFAULT_QUEUE_CAPACITY: usize = 2048;
/// Ports ignorés quand on ouvre « tous les ports » : la boucle logicielle d'ALSA ne porte aucun contrôleur.
const IGNORED_WHEN_ALL: [&str; 1] = ["Midi Through"];

/// Quels ports ouvrir.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PortSelection {
    /// Tous les ports d'entrée présents (sauf la boucle logicielle d'ALSA), au fur et à mesure qu'ils apparaissent.
    #[default]
    All,
    /// Ces ports, désignés par leur nom (correspondance souple, voir [`match_port_name`]). Un nom qui correspond à
    /// plusieurs ports ouvre ceux qui correspondent exactement, et à défaut ceux qui le contiennent.
    Named(Vec<String>),
}

impl PortSelection {
    /// Parmi les ports présents, ceux à ouvrir.
    #[must_use]
    pub fn pick<'a>(&self, listed: &'a [String]) -> Vec<&'a str> {
        match self {
            Self::All => listed
                .iter()
                .map(String::as_str)
                .filter(|name| !IGNORED_WHEN_ALL.iter().any(|ignored| name.starts_with(ignored)))
                .collect(),
            Self::Named(wanted) => {
                let mut picked: Vec<&str> = Vec::new();
                for want in wanted {
                    let best = listed
                        .iter()
                        .filter_map(|name| match_port_name(want, name).map(|quality| (quality, name.as_str())))
                        .min_by_key(|(quality, _)| *quality);
                    let Some((best_quality, _)) = best else { continue };
                    for name in listed {
                        if match_port_name(want, name) == Some(best_quality) && !picked.contains(&name.as_str()) {
                            picked.push(name);
                        }
                    }
                }
                picked
            }
        }
    }
}

/// Réglages de l'entrée MIDI.
#[derive(Debug, Clone)]
pub struct MidiConfig {
    pub selection: PortSelection,
    /// Période de scrutation des ports (branchement, débranchement).
    pub scan_period: Duration,
    /// Capacité du canal borné des callbacks vers l'application.
    pub queue_capacity: usize,
    /// Origine de [`MidiEvent::time_s`] : à partager avec les autres sources (audio, clavier) pour que tous les
    /// horodatages soient sur la même horloge.
    pub epoch: Instant,
}

impl Default for MidiConfig {
    fn default() -> Self {
        Self {
            selection: PortSelection::All,
            scan_period: DEFAULT_SCAN_PERIOD,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            epoch: Instant::now(),
        }
    }
}

/// Un port qui apparaît ou disparaît, dans l'ordre exact des messages (même canal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PortEvent {
    /// Le port est ouvert ; tous les messages qui suivent dans le flux portent ce [`PortId`]. `name` est le nom donné
    /// par l'hôte. À transmettre à [`Mapper::set_port_name`](crate::Mapper::set_port_name).
    Connected { port: PortId, name: String },
    /// Le port a disparu ou n'est plus sélectionné ; aucun message de ce [`PortId`] ne suit.
    Disconnected { port: PortId, name: String },
}

/// Ce qui sort du canal : un message ou un changement de ports, dans l'ordre.
#[derive(Debug, Clone, PartialEq)]
pub enum MidiItem {
    Message(MidiEvent),
    Port(PortEvent),
}

/// Un port ouvert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectedPort {
    pub port: PortId,
    pub name: String,
}

/// L'état des ports vu par l'interface (liste des ports, lesquels sont ouverts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PortsSnapshot {
    /// Les ports présents à la dernière scrutation.
    pub available: Vec<String>,
    /// Ceux que l'application a ouverts.
    pub connected: Vec<ConnectedPort>,
}

/// État partagé entre le superviseur (qui écrit) et l'application (qui lit un instantané).
pub(super) type SharedPorts = Arc<Mutex<PortsSnapshot>>;

/// L'entrée MIDI : un thread superviseur ouvre et rouvre les ports ; les callbacks poussent dans un canal borné que
/// l'application vide avec [`MidiInput::try_recv`].
///
/// Lâcher la poignée arrête le superviseur et ferme les ports.
#[derive(Debug)]
pub struct MidiInput {
    items: Receiver<MidiItem>,
    commands: Sender<Command>,
    ports: SharedPorts,
    stats: Arc<MidiStats>,
    thread: Option<JoinHandle<()>>,
}

impl MidiInput {
    /// Démarre l'entrée sur le matériel du système (`midir`).
    ///
    /// # Errors
    /// Le thread superviseur n'a pas pu démarrer. Un hôte MIDI absent n'est pas une erreur ici : le superviseur le
    /// journalise et réessaie à chaque scrutation (une machine sans MIDI doit pouvoir lancer le show).
    pub fn open(config: MidiConfig) -> Result<Self> {
        Self::start(MidirBackend, config)
    }

    /// Démarre l'entrée sur un backend donné ([`FakeMidi`](super::FakeMidi) pour les tests).
    ///
    /// # Errors
    /// Le thread superviseur n'a pas pu démarrer.
    pub fn start<B: MidiBackend>(backend: B, config: MidiConfig) -> Result<Self> {
        let (items_tx, items) = crossbeam_channel::bounded(config.queue_capacity.max(1));
        let (commands, commands_rx) = crossbeam_channel::unbounded();
        let ports = SharedPorts::default();
        let stats = Arc::new(MidiStats::default());
        let supervisor_ports = Arc::clone(&ports);
        let supervisor_stats = Arc::clone(&stats);
        let thread = std::thread::Builder::new()
            .name("cf-midi".into())
            .spawn(move || {
                Supervisor::new(backend, config, items_tx, commands_rx, supervisor_ports, supervisor_stats).run();
            })
            .map_err(|e| Error::Midi(format!("thread MIDI impossible à démarrer : {e}")))?;
        Ok(Self {
            items,
            commands,
            ports,
            stats,
            thread: Some(thread),
        })
    }

    /// Le prochain élément du flux, sans jamais attendre. À appeler en boucle, une fois par image, jusqu'à `None`.
    ///
    /// Ne jamais lire ce canal avec une attente bloquante : un lecteur bloqué fait prendre un verrou au callback.
    #[must_use]
    pub fn try_recv(&self) -> Option<MidiItem> {
        // Vide ou fermé : rien à lire dans les deux cas.
        self.items.try_recv().ok()
    }

    /// Change les ports à ouvrir ; la scrutation est refaite tout de suite.
    pub fn select(&self, selection: PortSelection) {
        // Le superviseur ne s'arrête qu'avec la poignée : une erreur d'envoi n'arrive pas en marche normale.
        let _ = self.commands.send(Command::Select(selection));
    }

    /// Force une scrutation des ports sans attendre la période (bouton « actualiser » de l'interface).
    pub fn rescan(&self) {
        let _ = self.commands.send(Command::Rescan);
    }

    /// Les ports présents et ouverts, à la dernière scrutation.
    #[must_use]
    pub fn ports(&self) -> PortsSnapshot {
        lock(&self.ports).clone()
    }

    /// Compteurs des callbacks (messages livrés, messages jetés par un canal plein).
    #[must_use]
    pub fn stats(&self) -> &MidiStats {
        &self.stats
    }
}

pub(super) fn lock(ports: &SharedPorts) -> MutexGuard<'_, PortsSnapshot> {
    // L'instantané est réécrit en entier à chaque scrutation : un empoisonnement ne laisse rien d'incohérent.
    ports.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Drop for MidiInput {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            // Un superviseur qui a paniqué n'a plus rien à nous dire : le panic est déjà sorti sur stderr.
            let _ = thread.join();
        }
    }
}
