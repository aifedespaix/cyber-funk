//! Le pont des contrôles (J3-06) : clavier de Bevy, threads MIDI, arbitrage du tempo, profils des créneaux.
//!
//! [`ControlsPlugin`] ne connaît que les `Action` de `cf-control`. Il tient, en `CfSet::Input` :
//! 1. le clavier des deux fenêtres (sauf quand egui écrit du texte, voir [`TextInputFocus`]) ;
//! 2. le flux MIDI (notes, CC, horloge), traduit par le `Mapper` ;
//! 3. l'arbitre de tempo, qui complète `Res<AudioFeatures>` (source, BPM, phase, mesure, phrase) ;
//!
//! et publie les actions en messages [`ControlAction`], que le chef du spectacle ([`ShowPlugin`](crate::ShowPlugin))
//! lit en `CfSet::Show`. Au chargement d'un créneau (`ShowEvent::SlotLoaded`), la pile de profils est rechargée.
//!
//! Ordre dans l'image : `AudioPump` (J1-06) ─▶ clavier ─▶ MIDI ─▶ tempo, tous dans `CfSet::Input`.

mod bridge;
mod keyboard;
mod library;
mod profiles;
mod status;

use std::time::Instant;

use bevy::prelude::*;
use cf_control::{Action, FakeMidi, MidiConfig, MidiInput, Switch, TimedAction};

pub use bridge::{ControlBridge, submit_action};
pub use keyboard::{key_code, key_event, logical_char, modifiers};
pub use library::{ControllerLibrary, LibraryError};
pub(crate) use profiles::apply_slot_profiles;
pub use status::{ControlsStatus, LastInput};

use crate::CfSet;
use crate::audio::{AudioCommand, AudioPump};

/// Une action produite par un contrôle (clavier, MIDI, OSC, interface), avec l'instant de l'entrée. Lue par le chef du
/// spectacle en `CfSet::Show`.
#[derive(Message, Debug, Clone, PartialEq, Deref)]
pub struct ControlAction(pub TimedAction);

/// Vrai quand un champ texte d'egui a le focus : le clavier ne déclenche alors aucune action. Écrit par la fenêtre
/// opérateur (`cf-operator`), lu par le pont clavier.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextInputFocus(pub bool);

/// D'où vient le MIDI.
#[derive(Debug, Clone, Default)]
pub enum MidiChoice {
    /// Les ports de la machine (`midir`). Une machine sans MIDI lance quand même le show.
    #[default]
    System,
    /// Pas de MIDI (tests, `--no-midi`).
    Off,
    /// Un hôte factice, pour les tests sans matériel.
    Fake(FakeMidi),
}

/// Réglages des contrôles.
#[derive(Debug, Clone, Default)]
pub struct ControlsConfig {
    /// Source du MIDI.
    pub midi: MidiChoice,
    /// Profils de contrôle livrés (`assets/controllers`), pour ceux que l'événement et les créneaux désignent.
    pub library: ControllerLibrary,
}

/// Branche les contrôles. À ajouter après `CorePlugin` et `CfAudioPlugin` (qui déclarent `CfSet::Input` et
/// `AudioPump`), avant ou après `ShowPlugin` indifféremment.
#[derive(Debug, Default)]
pub struct ControlsPlugin {
    /// Réglages.
    pub config: ControlsConfig,
}

impl ControlsPlugin {
    /// Plugin avec ces réglages.
    #[must_use]
    pub fn new(config: ControlsConfig) -> Self {
        Self { config }
    }
}

impl Plugin for ControlsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ControlAction>()
            .add_message::<AudioCommand>()
            .init_resource::<TextInputFocus>()
            .init_resource::<ControlsStatus>();
        // Même origine pour le MIDI et le reste : tous les `time_s` (MIDI, tap, arbitre) sont sur une seule horloge.
        let epoch = Instant::now();
        let midi_config = MidiConfig {
            epoch,
            ..MidiConfig::default()
        };
        let started = match &self.config.midi {
            MidiChoice::Off => Ok(None),
            MidiChoice::System => MidiInput::open(midi_config).map(Some),
            MidiChoice::Fake(fake) => MidiInput::start(fake.backend(), midi_config).map(Some),
        };
        let midi = match started {
            Ok(input) => input,
            Err(error) => {
                tracing::error!(%error, "contrôles : MIDI désactivé");
                app.world_mut()
                    .resource_mut::<ControlsStatus>()
                    .notes
                    .push(error.to_string());
                None
            }
        };
        match ControlBridge::new(midi, self.config.library.clone(), epoch) {
            Ok(bridge) => {
                app.insert_resource(bridge);
            }
            Err(error) => {
                // Le clavier par défaut est livré avec le code : il ne peut pas être invalide.
                tracing::error!(%error, "contrôles : clavier par défaut refusé, pont désactivé");
                return;
            }
        }
        app.add_systems(
            Update,
            (
                keyboard::read_keyboard,
                bridge::read_midi,
                bridge::update_tempo,
                forward_playback,
            )
                .chain()
                .after(AudioPump)
                .in_set(CfSet::Input),
        );
    }
}

/// `Action::Playback` (famille App) pilote le lecteur de démo, comme le bouton du panneau audio.
fn forward_playback(mut actions: MessageReader<ControlAction>, mut commands: MessageWriter<AudioCommand>) {
    for action in actions.read() {
        if let Action::Playback(switch) = &action.0.action {
            commands.write(match switch {
                Switch::On => AudioCommand::DemoPlay,
                Switch::Off => AudioCommand::DemoPause,
                Switch::Toggle => AudioCommand::DemoToggle,
            });
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
