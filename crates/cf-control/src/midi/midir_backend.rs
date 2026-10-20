//! Le backend `midir` : le vrai matériel (ALSA sous Linux, `CoreMIDI` sous macOS, `WinMM` sous Windows).

use midir::{Ignore, MidiInput, MidiInputConnection};

use super::{InputCallback, MidiBackend};
use crate::error::{Error, Result};

/// Nom sous lequel l'application se présente à l'hôte MIDI (visible dans `aconnect -l`, Audio MIDI Setup…).
const CLIENT_NAME: &str = "cyber-funk";

/// Les ports d'entrée du système, via `midir`.
#[derive(Debug, Default, Clone, Copy)]
pub struct MidirBackend;

impl MidirBackend {
    /// Un client `midir` configuré pour ce qu'on lit : les `SysEx` et l'Active Sensing ne servent à rien ici, mais
    /// l'horloge et les messages de transport (J3-03) passent.
    fn client() -> Result<MidiInput> {
        let mut input =
            MidiInput::new(CLIENT_NAME).map_err(|e| Error::Midi(format!("client MIDI inutilisable : {e}")))?;
        input.ignore(Ignore::SysexAndActiveSense);
        Ok(input)
    }
}

impl MidiBackend for MidirBackend {
    type Connection = MidiInputConnection<InputCallback>;

    fn list_ports(&mut self) -> Result<Vec<String>> {
        // Un client neuf à chaque scrutation : `midir` consomme le client en ouvrant un port, donc il n'y a pas de
        // client à garder en réserve, et le coût (ouvrir et fermer un client) est négligeable toutes les 2 s.
        let input = Self::client()?;
        // Un port dont le nom ne se lit pas est ignoré : on ne saurait ni le nommer dans un profil ni le retrouver.
        Ok(input
            .ports()
            .iter()
            .filter_map(|port| input.port_name(port).ok())
            .collect())
    }

    fn connect(&mut self, name: &str, callback: InputCallback) -> Result<Self::Connection> {
        let input = Self::client()?;
        let port = input
            .ports()
            .into_iter()
            .find(|port| input.port_name(port).is_ok_and(|n| n == name))
            .ok_or_else(|| Error::Midi(format!("le port « {name} » a disparu avant son ouverture")))?;
        input
            .connect(
                &port,
                "cyber-funk entrée",
                |_stamp_us, bytes, callback| callback.handle(bytes),
                callback,
            )
            .map_err(|e| Error::Midi(format!("ouverture du port « {name} » refusée : {e}")))
    }
}
