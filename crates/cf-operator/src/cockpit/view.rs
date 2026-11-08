//! La vue Live : trois colonnes. DJ à gauche, état du show et chiffres au centre, audio et playlist à droite.
//!
//! Trop étroite pour trois colonnes ([`NARROW_WIDTH`]), la colonne des DJ devient un bandeau au-dessus du reste.

use bevy_egui::egui;
use cf_render::audio::{AudioCommand, AudioSource, AudioStatus};

use super::audio_column::{self, AudioHeaderOut};
use super::center::{self, LiveStats};
use super::dj_column::{self, DjCard, DjColumnEvent};
use super::playlist_view::{self, PlaylistOut, PlaylistUi};
use crate::run_state::ShowRunState;

/// Largeur de la fenêtre en dessous de laquelle les DJ passent en bandeau (points egui).
pub(crate) const NARROW_WIDTH: f32 = 900.0;
/// Largeur initiale de la colonne des DJ.
const DJ_COLUMN_WIDTH: f32 = 230.0;
/// Largeur initiale de la colonne audio.
const AUDIO_COLUMN_WIDTH: f32 = 410.0;

/// Ce que la vue lit pour une image.
#[derive(Debug)]
pub(crate) struct LiveInput<'a> {
    pub cards: &'a [DjCard],
    pub active_dj: Option<usize>,
    pub dj_name: Option<&'a str>,
    pub run: &'a ShowRunState,
    pub elapsed_s: Option<f64>,
    pub stats: LiveStats,
    pub audio: Option<&'a AudioStatus>,
    pub midi: bool,
    /// Le noir est allumé.
    pub blackout: bool,
}

/// Ce que l'opérateur a fait pendant l'image.
#[derive(Debug)]
pub(crate) struct LiveOut {
    pub dj: DjColumnEvent,
    /// Le bouton Démarrer / Arrêter a été pressé.
    pub press: bool,
    /// Le bouton Noir a été pressé.
    pub noir: bool,
    pub audio: Vec<AudioCommand>,
    pub midi: Option<bool>,
    pub add_files: bool,
}

impl Default for LiveOut {
    fn default() -> Self {
        Self {
            dj: DjColumnEvent::None,
            press: false,
            noir: false,
            audio: Vec::new(),
            midi: None,
            add_files: false,
        }
    }
}

/// Dessine la vue dans le panneau central de la fenêtre.
pub(crate) fn draw(root: &mut egui::Ui, input: &LiveInput<'_>, playlist: &mut PlaylistUi) -> LiveOut {
    let mut out = LiveOut::default();
    let narrow = root.available_width() < NARROW_WIDTH;

    if narrow {
        egui::Panel::top("cockpit_dj_strip").show(root, |ui| {
            out.dj = dj_strip(ui, input.cards, input.active_dj);
        });
    } else {
        egui::Panel::left("cockpit_djs")
            .default_size(DJ_COLUMN_WIDTH)
            .resizable(true)
            .show(root, |ui| {
                out.dj = dj_column::dj_cards(ui, input.cards, input.active_dj);
            });
    }
    egui::Panel::right("cockpit_audio")
        .default_size(AUDIO_COLUMN_WIDTH)
        .resizable(true)
        .show(root, |ui| {
            audio_side(ui, input, playlist, &mut out);
        });
    egui::CentralPanel::default().show(root, |ui| {
        let centre = center::center(
            ui,
            input.run,
            &input.stats,
            input.dj_name,
            input.elapsed_s,
            input.blackout,
        );
        out.press = centre.press;
        out.noir = centre.noir;
    });
    out
}

/// Le bandeau des DJ des fenêtres étroites : une pastille par DJ.
fn dj_strip(ui: &mut egui::Ui, cards: &[DjCard], active: Option<usize>) -> DjColumnEvent {
    let mut event = DjColumnEvent::None;
    ui.horizontal_wrapped(|ui| {
        ui.strong("DJ");
        for (index, card) in cards.iter().enumerate() {
            let label = egui::RichText::new(&card.name).color(card.color);
            if ui.add(egui::Button::selectable(active == Some(index), label)).clicked() && active != Some(index) {
                event = DjColumnEvent::Pick(index);
            }
        }
        if ui.button("+ Nouveau").clicked() {
            event = DjColumnEvent::New;
        }
    });
    event
}

fn audio_side(ui: &mut egui::Ui, input: &LiveInput<'_>, playlist: &mut PlaylistUi, out: &mut LiveOut) {
    let Some(status) = input.audio else {
        ui.weak("Audio non branché (CfAudioPlugin absent).");
        return;
    };
    let AudioHeaderOut { commands, midi } = audio_column::audio_header(ui, status, input.midi);
    out.audio.extend(commands);
    out.midi = midi;
    ui.separator();
    if status.source != AudioSource::Demo {
        ui.weak("Source micro : la playlist est en pause.");
        return;
    }
    out.audio.extend(playlist_view::player(ui, status));
    ui.add_space(6.0);
    let PlaylistOut { commands, add_files } = playlist_view::playlist(ui, status, playlist);
    out.audio.extend(commands);
    out.add_files = add_files;
}
