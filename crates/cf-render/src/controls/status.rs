//! Ce que le panneau opérateur affiche des contrôles : ports vus, dernier message, dernière action, tempo.
//!
//! Écrit par les systèmes du pont, lu par l'interface. Rien ici ne formate de texte : le panneau formate lui-même, à
//! l'image où il dessine (pas d'allocation par message reçu côté pont).

use bevy::prelude::*;
use cf_control::{Action, KeyEvent, MidiEvent, PortsSnapshot, Transport};

/// Le dernier geste reçu, avant correspondance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LastInput {
    /// Touche du clavier (fenêtre projecteur ou opérateur).
    Key(KeyEvent),
    /// Message MIDI de commande (note, CC, Program Change) ou autre message non horloge.
    Midi(MidiEvent),
}

/// État des contrôles pour l'interface.
#[derive(Resource, Debug, Clone)]
pub struct ControlsStatus {
    /// Le MIDI est démarré (faux : désactivé ou impossible à démarrer, voir `notes`).
    pub midi_running: bool,
    /// Ports MIDI présents et ouverts, rafraîchis quelques fois par seconde.
    pub ports: PortsSnapshot,
    /// Messages MIDI livrés par les callbacks depuis le démarrage.
    pub midi_delivered: u64,
    /// Messages jetés par un canal plein.
    pub midi_dropped: u64,
    /// Dernier geste reçu.
    pub last_input: Option<LastInput>,
    /// Dernière action produite (et son instant, en secondes sur l'horloge des contrôles).
    pub last_action: Option<(f64, Action)>,
    /// Tempo de l'horloge MIDI (BPM), s'il y en a une.
    pub clock_bpm: Option<f64>,
    /// État du transport MIDI.
    pub transport: Transport,
    /// Tempo tapé (BPM).
    pub tap_bpm: Option<f64>,
    /// Remarques à montrer (démarrage MIDI impossible, profil refusé…).
    pub notes: Vec<String>,
}

impl Default for ControlsStatus {
    fn default() -> Self {
        Self {
            midi_running: false,
            ports: PortsSnapshot::default(),
            midi_delivered: 0,
            midi_dropped: 0,
            last_input: None,
            last_action: None,
            clock_bpm: None,
            transport: Transport::Unknown,
            tap_bpm: None,
            notes: Vec::new(),
        }
    }
}
