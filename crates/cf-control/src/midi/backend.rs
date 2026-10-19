//! Le contrat d'une source de ports MIDI : lister les ports d'entrée présents, en ouvrir un.
//!
//! Deux implémentations : [`MidirBackend`](super::MidirBackend) (le vrai matériel, via `midir`) et
//! [`FakeMidi`](super::FakeMidi) (ports branchés et débranchés à la main), pour tester le rebranchement à chaud sans
//! contrôleur.

use super::InputCallback;
use crate::error::Result;

/// Un hôte MIDI capable de lister et d'ouvrir des ports d'entrée.
///
/// Le superviseur ([`MidiInput`](super::MidiInput)) possède le backend sur son propre thread : `list_ports` et
/// `connect` y ont lieu, et la connexion y est détruite. Ni la connexion ni le backend n'ont donc besoin d'être
/// `Sync`.
pub trait MidiBackend: Send + 'static {
    /// Garde de la connexion : la lâcher ferme le port.
    type Connection;

    /// Les noms des ports d'entrée présents en ce moment, tels que l'hôte les donne (avec leurs numéros éventuels :
    /// voir [`canonical_port_name`](super::canonical_port_name)).
    ///
    /// # Errors
    /// L'hôte MIDI est inutilisable (pas de séquenceur ALSA, permissions…).
    fn list_ports(&mut self) -> Result<Vec<String>>;

    /// Ouvre le port `name` (un nom rendu par [`MidiBackend::list_ports`]) ; `callback.handle` est appelé à chaque
    /// paquet d'octets reçu, sur le thread de l'hôte.
    ///
    /// # Errors
    /// Le port a disparu entre la liste et l'ouverture, ou l'hôte refuse l'ouverture.
    fn connect(&mut self, name: &str, callback: InputCallback) -> Result<Self::Connection>;
}
