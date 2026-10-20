//! Le thread superviseur : scrute les ports, ouvre ceux qu'on veut, referme ceux qui disparaissent et les rouvre dès
//! qu'ils reviennent, en les retrouvant par leur nom.
//!
//! Ce thread n'est pas temps réel : il peut allouer, verrouiller (l'instantané des ports) et journaliser. Les
//! callbacks, eux, ne passent jamais par ici.
//!
//! Un hôte MIDI ne prévient pas d'un débranchement : on le déduit de l'absence du port dans la liste. Limite connue :
//! un contrôleur débranché puis rebranché entre deux scrutations, avec exactement le même nom d'hôte, passe
//! inaperçu (sous ALSA les numéros `client:port` changent presque toujours, ce qui suffit à le détecter).

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};

use super::callback::InputCallback;
use super::input::{SharedPorts, lock};
use super::{ConnectedPort, MidiBackend, MidiConfig, MidiItem, MidiStats, PortEvent, PortSelection};
use crate::raw::PortId;

/// Combien de temps le superviseur attend de la place dans le canal pour annoncer un port. Le canal n'est plein que si
/// l'application ne le lit plus ; passé ce délai, l'annonce est abandonnée plutôt que de bloquer l'arrêt.
const ANNOUNCE_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub(super) enum Command {
    Select(PortSelection),
    Rescan,
    Stop,
}

struct Connected<C> {
    /// Garde de la connexion : la lâcher ferme le port.
    connection: C,
    port: PortId,
    name: String,
}

pub(super) struct Supervisor<B: MidiBackend> {
    backend: B,
    config: MidiConfig,
    items: Sender<MidiItem>,
    commands: Receiver<Command>,
    shared: SharedPorts,
    stats: Arc<MidiStats>,
    connections: Vec<Connected<B::Connection>>,
    next_port: u16,
    /// Ports dont l'ouverture a déjà été signalée en échec : on ne journalise qu'une fois par panne, pas toutes les 2 s.
    failed: HashSet<String>,
    host_down_logged: bool,
}

impl<B: MidiBackend> Supervisor<B> {
    pub(super) fn new(
        backend: B,
        config: MidiConfig,
        items: Sender<MidiItem>,
        commands: Receiver<Command>,
        shared: SharedPorts,
        stats: Arc<MidiStats>,
    ) -> Self {
        Self {
            backend,
            config,
            items,
            commands,
            shared,
            stats,
            connections: Vec::new(),
            next_port: 0,
            failed: HashSet::new(),
            host_down_logged: false,
        }
    }

    pub(super) fn run(mut self) {
        tracing::debug!(selection = ?self.config.selection, "MIDI : superviseur démarré");
        self.scan();
        loop {
            match self.commands.recv_timeout(self.config.scan_period) {
                Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Command::Select(selection)) => {
                    tracing::info!(?selection, "MIDI : changement de sélection des ports");
                    self.config.selection = selection;
                    self.scan();
                }
                Ok(Command::Rescan) | Err(RecvTimeoutError::Timeout) => self.scan(),
            }
        }
        // Referme tout proprement : l'application voit chaque port partir.
        while let Some(connection) = self.connections.pop() {
            self.announce_closed(connection);
        }
    }

    fn scan(&mut self) {
        let listed = match self.backend.list_ports() {
            Ok(listed) => {
                self.host_down_logged = false;
                listed
            }
            Err(error) => {
                // Les connexions en place sont gardées : une énumération qui échoue une fois ne doit pas couper un
                // contrôleur qui marche. Elles partiront à la première énumération réussie qui ne les contient plus.
                if !self.host_down_logged {
                    tracing::warn!(%error, "MIDI : ports illisibles, nouvel essai à la prochaine scrutation");
                    self.host_down_logged = true;
                }
                return;
            }
        };
        let wanted: Vec<String> = self
            .config
            .selection
            .pick(&listed)
            .into_iter()
            .map(str::to_owned)
            .collect();

        // Ports disparus ou plus voulus.
        let (keep, gone): (Vec<_>, Vec<_>) = std::mem::take(&mut self.connections)
            .into_iter()
            .partition(|c| wanted.contains(&c.name));
        self.connections = keep;
        for connection in gone {
            tracing::info!(port = %connection.name, "MIDI : port fermé (débranché ou désélectionné)");
            self.announce_closed(connection);
        }
        self.failed.retain(|name| wanted.contains(name));

        // Ports nouveaux ou revenus.
        for name in wanted {
            if !self.connections.iter().any(|c| c.name == name) {
                self.open(name);
            }
        }

        let mut shared = lock(&self.shared);
        shared.available = listed;
        shared.connected = self
            .connections
            .iter()
            .map(|c| ConnectedPort {
                port: c.port,
                name: c.name.clone(),
            })
            .collect();
    }

    fn open(&mut self, name: String) {
        let port = self.allocate_port();
        let (callback, gate) = InputCallback::new(port, self.items.clone(), Arc::clone(&self.stats), self.config.epoch);
        let connection = match self.backend.connect(&name, callback) {
            Ok(connection) => connection,
            Err(error) => {
                if self.failed.insert(name.clone()) {
                    tracing::warn!(port = %name, %error, "MIDI : ouverture impossible, nouvel essai à la prochaine scrutation");
                }
                return;
            }
        };
        // Le callback est déjà vivant mais son portillon est fermé : il jette tout jusqu'à ce que l'application ait
        // l'annonce du port, afin qu'aucun message ne la précède.
        let announced = self.announce(PortEvent::Connected {
            port,
            name: name.clone(),
        });
        if !announced {
            drop(connection);
            return;
        }
        gate.store(true, std::sync::atomic::Ordering::Release);
        self.failed.remove(&name);
        tracing::info!(port = %name, id = port.0, "MIDI : port ouvert");
        self.connections.push(Connected { connection, port, name });
    }

    /// Ferme la connexion (plus aucun callback après) puis l'annonce : tous les messages du port précèdent l'annonce
    /// dans le canal.
    fn announce_closed(&self, connection: Connected<B::Connection>) {
        let Connected { connection, port, name } = connection;
        drop(connection);
        self.announce(PortEvent::Disconnected { port, name });
    }

    fn announce(&self, event: PortEvent) -> bool {
        match self.items.send_timeout(MidiItem::Port(event), ANNOUNCE_TIMEOUT) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "MIDI : annonce de port perdue (canal plein ou fermé)");
                false
            }
        }
    }

    /// Un numéro de port neuf, jamais celui d'une connexion ouverte. Les numéros ne sont pas recyclés tout de suite
    /// (ils montent, puis bouclent après 65 536 ouvertures) pour qu'un message en retard d'une ancienne connexion ne soit
    /// jamais attribué à un autre contrôleur.
    fn allocate_port(&mut self) -> PortId {
        loop {
            let candidate = PortId(self.next_port);
            self.next_port = self.next_port.wrapping_add(1);
            if !self.connections.iter().any(|c| c.port == candidate) {
                return candidate;
            }
        }
    }
}
