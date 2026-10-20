//! Un hôte MIDI factice : des ports qu'on branche, débranche et alimente en octets, pour tester toute la chaîne
//! (superviseur, rebranchement à chaud, callback, mapper) sans contrôleur ni séquenceur ALSA.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::{InputCallback, MidiBackend};
use crate::error::{Error, Result};

/// La télécommande de l'hôte factice, à garder dans le test ; [`FakeMidi::backend`] en donne le backend à confier au
/// superviseur. Les deux partagent le même état.
#[derive(Debug, Clone, Default)]
pub struct FakeMidi {
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    /// Ports présents, dans l'ordre de branchement.
    ports: Vec<String>,
    /// Connexions ouvertes : (numéro de connexion, nom du port, callback).
    open: Vec<(u64, String, InputCallback)>,
    next_connection: u64,
    connect_attempts: u32,
    /// Nombre d'ouvertures à refuser avant de laisser passer.
    refusals_left: u32,
    /// Fait échouer `list_ports` (hôte MIDI inutilisable).
    host_down: bool,
}

impl FakeMidi {
    /// Un hôte sans aucun port.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Le backend à passer à [`MidiInput::start`](super::MidiInput::start).
    #[must_use]
    pub fn backend(&self) -> FakeBackend {
        FakeBackend { host: self.clone() }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Un test qui panique ailleurs ne doit pas empoisonner celui-ci : l'état reste cohérent à chaque instruction.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Branche un contrôleur : son port apparaît dans la liste.
    pub fn plug(&self, name: &str) {
        let mut state = self.lock();
        if !state.ports.iter().any(|p| p == name) {
            state.ports.push(name.to_owned());
        }
    }

    /// Débranche un contrôleur : son port disparaît de la liste. La connexion ouverte, elle, reste en place jusqu'à ce
    /// que le superviseur s'en aperçoive (comme avec un vrai hôte, qui ne prévient pas).
    pub fn unplug(&self, name: &str) {
        self.lock().ports.retain(|p| p != name);
    }

    /// Le contrôleur envoie ces octets. `false` si aucune connexion ouverte n'est sur ce port (le message est perdu,
    /// comme sur du matériel que personne n'écoute).
    #[must_use]
    pub fn send(&self, name: &str, bytes: &[u8]) -> bool {
        let mut state = self.lock();
        match state.open.iter_mut().find(|(_, port, _)| port == name) {
            Some((_, _, callback)) => {
                callback.handle(bytes);
                true
            }
            None => false,
        }
    }

    /// Les ports actuellement ouverts par le superviseur.
    #[must_use]
    pub fn open_ports(&self) -> Vec<String> {
        self.lock().open.iter().map(|(_, port, _)| port.clone()).collect()
    }

    /// Nombre total d'ouvertures tentées (réussies ou refusées) depuis le début.
    #[must_use]
    pub fn connect_attempts(&self) -> u32 {
        self.lock().connect_attempts
    }

    /// Refuse les `count` prochaines ouvertures (port occupé par un autre logiciel, par exemple).
    pub fn refuse_next_connects(&self, count: u32) {
        self.lock().refusals_left = count;
    }

    /// Fait échouer (ou rétablit) l'énumération des ports.
    pub fn set_host_down(&self, down: bool) {
        self.lock().host_down = down;
    }
}

/// Le backend factice : voir [`FakeMidi`].
#[derive(Debug)]
pub struct FakeBackend {
    host: FakeMidi,
}

/// Garde d'une connexion factice : la lâcher la ferme.
#[derive(Debug)]
pub struct FakeConnection {
    host: FakeMidi,
    id: u64,
}

impl Drop for FakeConnection {
    fn drop(&mut self) {
        let id = self.id;
        self.host.lock().open.retain(|(open_id, _, _)| *open_id != id);
    }
}

impl MidiBackend for FakeBackend {
    type Connection = FakeConnection;

    fn list_ports(&mut self) -> Result<Vec<String>> {
        let state = self.host.lock();
        if state.host_down {
            return Err(Error::Midi("hôte MIDI factice arrêté".into()));
        }
        Ok(state.ports.clone())
    }

    fn connect(&mut self, name: &str, callback: InputCallback) -> Result<Self::Connection> {
        let mut state = self.host.lock();
        state.connect_attempts += 1;
        if state.refusals_left > 0 {
            state.refusals_left -= 1;
            return Err(Error::Midi(format!("ouverture du port « {name} » refusée")));
        }
        if !state.ports.iter().any(|p| p == name) {
            return Err(Error::Midi(format!("le port « {name} » a disparu avant son ouverture")));
        }
        let id = state.next_connection;
        state.next_connection += 1;
        state.open.push((id, name.to_owned(), callback));
        Ok(FakeConnection {
            host: self.host.clone(),
            id,
        })
    }
}
