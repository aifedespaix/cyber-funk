//! Les entrées brutes, avant correspondance : ce que les pilotes (pont clavier de Bevy, thread MIDI, serveur OSC)
//! envoient au [`Mapper`](crate::Mapper).
//!
//! Les événements MIDI ne portent pas le nom de leur port mais un [`PortId`] : le callback MIDI (J3-02) n'a ainsi
//! rien à allouer, et le mapper retrouve le nom grâce à [`Mapper::set_port_name`](crate::Mapper::set_port_name).

use crate::keyboard::KeyEvent;

/// Numéro d'un port MIDI ouvert, attribué par la couche MIDI pour la durée de la connexion. Les profils, eux,
/// désignent les ports par leur nom (les numéros changent au rebranchement).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortId(pub u16);

/// Une entrée brute horodatée.
#[derive(Debug, Clone, PartialEq)]
pub struct RawInput {
    /// Instant de réception, en secondes, sur l'horloge monotone commune de l'application. Recopié tel quel dans la
    /// [`TimedAction`](crate::TimedAction) produite (tap tempo, compensation de latence).
    pub time_s: f64,
    pub event: RawEvent,
}

/// Le contenu d'une entrée brute, par famille de pilote.
#[derive(Debug, Clone, PartialEq)]
pub enum RawEvent {
    /// Appui ou relâchement d'une touche.
    Key(KeyEvent),
    /// Note MIDI. `velocity` 1..=127 : appui ; 0 : relâchement (Note Off et Note On vélocité 0 arrivent ainsi).
    /// `channel` 0..=15 (le canal « 1 » des fiches techniques est 0 ici).
    MidiNote {
        port: PortId,
        channel: u8,
        note: u8,
        velocity: u8,
    },
    /// Control Change MIDI, valeur 7 bits 0..=127. Le décodage (position absolue ou décalage d'encodeur) dépend du
    /// binding, pas du message : voir [`RelativeEncoding`](crate::RelativeEncoding).
    MidiCc {
        port: PortId,
        channel: u8,
        cc: u8,
        value: u8,
    },
    /// Program Change MIDI (`program` 0..=127, `channel` 0..=15). Les CC 14 bits n'ont pas d'événement propre : leurs
    /// deux moitiés arrivent comme des [`RawEvent::MidiCc`] et c'est le [`Mapper`](crate::Mapper) qui les recompose,
    /// parce que seul le profil sait quels CC forment une paire.
    MidiProgram { port: PortId, channel: u8, program: u8 },
    /// Message OSC. `value` : premier argument numérique s'il y en a un (0..1 pour un potard, 1/0 pour un bouton,
    /// décalage signé pour un encodeur), `None` sinon (simple impulsion).
    Osc { address: String, value: Option<f32> },
}

impl RawInput {
    /// Raccourci pour une entrée clavier.
    #[must_use]
    pub fn key(time_s: f64, event: KeyEvent) -> Self {
        Self {
            time_s,
            event: RawEvent::Key(event),
        }
    }
}
