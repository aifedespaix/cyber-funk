//! Le callback temps réel d'un port MIDI ouvert.
//!
//! Règles (skill `realtime-audio`, §« Callback ») : aucune allocation, aucun verrou, aucun journal, aucune I/O. Le
//! callback analyse les octets, horodate et pousse dans un canal borné par `try_send` ; si le canal est plein, le
//! message est jeté et compté (`MidiStats::dropped`), jamais attendu.
//!
//! `try_send` d'un canal `crossbeam` borné ne prend un verrou que s'il y a un lecteur *bloqué* en attente : les
//! lecteurs de ce module (`MidiInput::try_recv`) ne bloquent jamais, et le callback reste donc sans verrou.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use assert_no_alloc::assert_no_alloc;
use crossbeam_channel::{Sender, TrySendError};

use super::{MidiEvent, MidiItem, MidiParser};
use crate::raw::PortId;

/// Compteurs partagés entre les callbacks et l'application (lisibles sans verrou).
#[derive(Debug, Default)]
pub struct MidiStats {
    delivered: AtomicU64,
    dropped: AtomicU64,
}

impl MidiStats {
    /// Messages décodés et mis dans le canal.
    #[must_use]
    pub fn delivered(&self) -> u64 {
        self.delivered.load(Ordering::Relaxed)
    }

    /// Messages décodés mais jetés parce que le canal était plein (l'application ne lit pas assez vite).
    #[must_use]
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Le portillon d'un port : tant qu'il est fermé, le callback jette les messages. Il s'ouvre une fois la connexion
/// annoncée à l'application ([`PortEvent::Connected`](super::PortEvent::Connected)), pour qu'aucun message ne la
/// précède : sans cela, le mapper recevrait des notes d'un port dont il ne connaît pas encore le nom.
pub(crate) type Gate = Arc<AtomicBool>;

/// Ce que le backend appelle à chaque paquet d'octets reçu sur un port. `Send`, car `midir` l'exécute sur son propre
/// thread.
#[derive(Debug)]
pub struct InputCallback {
    port: PortId,
    parser: MidiParser,
    events: Sender<MidiItem>,
    stats: Arc<MidiStats>,
    epoch: Instant,
    gate: Gate,
}

impl InputCallback {
    pub(crate) fn new(port: PortId, events: Sender<MidiItem>, stats: Arc<MidiStats>, epoch: Instant) -> (Self, Gate) {
        let gate = Gate::default();
        let callback = Self {
            port,
            parser: MidiParser::new(),
            events,
            stats,
            epoch,
            gate: Arc::clone(&gate),
        };
        (callback, gate)
    }

    /// Traite un paquet d'octets reçu du pilote. Appelé par le backend, sur le thread MIDI de l'hôte ; ne fait
    /// jamais d'allocation (en debug, une allocation avorte le processus : `assert_no_alloc`).
    pub fn handle(&mut self, bytes: &[u8]) {
        assert_no_alloc(|| self.handle_bytes(bytes));
    }

    fn handle_bytes(&mut self, bytes: &[u8]) {
        if !self.gate.load(Ordering::Acquire) {
            return;
        }
        // Horodatage à la réception : un seul pour tout le paquet.
        let time_s = self.epoch.elapsed().as_secs_f64();
        let Self {
            port,
            parser,
            events,
            stats,
            ..
        } = self;
        parser.parse(bytes, |message| {
            let event = MidiEvent {
                time_s,
                port: *port,
                message,
            };
            // Canal plein ou lecteur parti : on jette et on compte. `Disconnected` n'est pas une panne à signaler
            // ici (l'application s'arrête) ; les deux cas comptent comme jetés.
            let counter = match events.try_send(MidiItem::Message(event)) {
                Ok(()) => &stats.delivered,
                Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => &stats.dropped,
            };
            counter.fetch_add(1, Ordering::Relaxed);
        });
    }
}
