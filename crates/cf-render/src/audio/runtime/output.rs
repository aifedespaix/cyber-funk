//! Choix de la sortie audio du lecteur de démo, à chaud.
//!
//! Changer de carte son rouvre le lecteur sur la nouvelle sortie, au même titre, à la même position et dans le même
//! état (lecture ou pause, volume, mode). Le flux d'analyse est rebranché sur le nouveau lecteur. Si la nouvelle
//! sortie ne s'ouvre pas, l'ancien lecteur continue et la raison se lit dans `AudioStatus::notes`.

use cf_audio::capture::DeviceSelector;
use cf_audio::player::{CpalOutput, NullOutput, OutputBackend, Player, PlayerConfig};
use std::sync::Arc;

use cf_audio::playlist::{Playlist, Track};

use super::{AudioRuntime, NULL_OUTPUT_RATE};
use crate::audio::config::OutputChoice;

/// En dessous de cette position (s), inutile de se replacer : on repart du début du titre.
const MIN_RESUME_S: f64 = 0.5;

impl AudioRuntime {
    /// Relit la liste des sorties audio. Sans carte son réelle (sortie nulle ou désactivée), la liste reste vide.
    pub(in crate::audio) fn refresh_outputs(&mut self) {
        if self.output != OutputChoice::Default {
            self.outputs.clear();
            return;
        }
        match CpalOutput::device_names() {
            Ok(names) => self.outputs = names,
            Err(error) => {
                tracing::warn!(%error, "audio : liste des sorties illisible");
                self.outputs.clear();
                self.notes.push(format!("sorties audio illisibles : {error}"));
            }
        }
    }

    /// Rouvre le lecteur sur `device`. Un nom introuvable retombe sur la sortie par défaut, avec une note.
    pub(in crate::audio) fn select_output(&mut self, device: DeviceSelector) {
        let Some(old) = self.player.as_ref() else {
            self.notes.push("pas de lecteur : choix de la sortie ignoré".to_owned());
            return;
        };
        let state = old.state();
        let tracks = old.tracks().to_vec();
        let Some((backend, device)) = self.open_output(&device) else {
            return;
        };
        let playlist = Playlist::new(tracks, state.track, self.playlist_seed);
        let config = PlayerConfig {
            mode: state.mode,
            volume: state.volume,
            muted: state.muted,
        };
        let mut player = match Player::new(playlist, config, backend) {
            Ok(player) => player,
            Err(error) => {
                tracing::warn!(%error, "audio : la nouvelle sortie ne s'ouvre pas, ancienne sortie conservée");
                self.notes.push(format!("sortie impossible à ouvrir : {error}"));
                return;
            }
        };
        if let Some(feed) = player.take_analysis_feed() {
            self.control.attach(feed);
        }
        if state.position_s > MIN_RESUME_S {
            player.seek(state.position_s);
        }
        if state.playing {
            player.play();
        }
        // L'ancien lecteur est lâché ici : sa sortie s'arrête une fois le nouveau branché.
        self.player = Some(player);
        self.output_device = device;
    }

    /// Remplace la playlist. Sans lecteur (aucune piste au démarrage), le premier ajout en crée un sur la sortie
    /// choisie.
    pub(in crate::audio) fn set_playlist(&mut self, tracks: Vec<Track>) {
        self.tracks = Arc::from(tracks.clone());
        if let Some(player) = self.player.as_mut() {
            player.set_tracks(tracks);
            return;
        }
        if tracks.is_empty() {
            return;
        }
        let Some((backend, device)) = self.open_output(&self.output_device.clone()) else {
            return;
        };
        self.output_device = device;
        let playlist = Playlist::new(tracks, Some(0), self.playlist_seed);
        match Player::new(playlist, PlayerConfig::default(), backend) {
            Ok(mut player) => {
                if let Some(feed) = player.take_analysis_feed() {
                    self.control.attach(feed);
                }
                self.player = Some(player);
                // La source Démo redevient possible : on la relance si c'est elle que l'opérateur a choisie.
                self.applied = None;
            }
            Err(error) => self
                .notes
                .push(format!("lecteur de démo impossible à démarrer : {error}")),
        }
    }

    /// Ouvre la sortie demandée et rend aussi celle qui est réellement utilisée (la sortie par défaut après un repli).
    fn open_output(&mut self, device: &DeviceSelector) -> Option<(Box<dyn OutputBackend>, DeviceSelector)> {
        match (self.output, device) {
            (OutputChoice::Disabled, _) => None,
            (OutputChoice::Null { speed }, _) => Some((
                Box::new(NullOutput::new(NULL_OUTPUT_RATE).with_speed(speed)),
                device.clone(),
            )),
            (OutputChoice::Default, DeviceSelector::Name(name)) => match CpalOutput::by_name(name) {
                Ok(output) => Some((Box::new(output), device.clone())),
                Err(error) => {
                    tracing::warn!(%error, device = %name, "audio : sortie introuvable, repli sur la sortie par défaut");
                    self.notes
                        .push(format!("sortie « {name} » introuvable, repli sur la sortie par défaut"));
                    self.default_output()
                }
            },
            (OutputChoice::Default, DeviceSelector::Default) => self.default_output(),
        }
    }

    fn default_output(&mut self) -> Option<(Box<dyn OutputBackend>, DeviceSelector)> {
        match CpalOutput::default_device() {
            Ok(output) => Some((Box::new(output), DeviceSelector::Default)),
            Err(error) => {
                self.notes.push(format!("pas de sortie audio : {error}"));
                None
            }
        }
    }
}
