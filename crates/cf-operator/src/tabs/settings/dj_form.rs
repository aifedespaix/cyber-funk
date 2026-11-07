//! La fiche du DJ sélectionné (ou les réglages de l'événement) : les formulaires existants de la Préparation (nom,
//! logo, thème, horaires, mode, profil audio, profils de contrôle, puis les cues), avec un pied fixe : état
//! « modifications non enregistrées », `Annuler` (revient à la version enregistrée) et `Enregistrer` (`Ctrl+S`).
//!
//! Les champs fictifs de la maquette (style, couleur d'enseigne libre) n'existent pas dans le modèle : pas ici non
//! plus.

use bevy_egui::egui;
use cf_show::model::{Event, LookId};

use super::Request;
use crate::prepare::Selection;
use crate::prepare::cue_form::cue_form;
use crate::prepare::forms::{event_form, slot_form};
use crate::prepare::lists::{ListRow, reorderable};
use crate::prepare::panel::{Op, SAVE, path_flagged};
use crate::prepare::widgets::Touch;

/// La fiche, pour une image.
pub(crate) struct DjFormView<'a> {
    pub event: &'a mut Event,
    pub selection: &'a mut Selection,
    pub looks: &'a [LookId],
    pub library_ids: &'a [String],
    pub flagged: &'a [String],
    /// La fiche affichée diffère de sa version enregistrée.
    pub modified: bool,
    pub touch: &'a mut Touch,
    pub ops: &'a mut Vec<Op>,
    pub requests: &'a mut Vec<Request>,
}

impl DjFormView<'_> {
    /// Dessine la fiche : le pied d'abord (il reste en bas, hors défilement), puis le formulaire.
    pub(crate) fn show(self, ui: &mut egui::Ui) {
        let Self {
            event,
            selection,
            looks,
            library_ids,
            flagged,
            modified,
            touch,
            ops,
            requests,
        } = self;
        egui::Panel::bottom("dj_form_footer").show(ui, |ui| footer(ui, modified, requests));
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("dj_form").show(ui, |ui| {
                if selection.event {
                    ui.heading("Réglages de l'événement");
                    event_form(ui, event, looks, library_ids, touch);
                    return;
                }
                let index = selection.slot;
                let Some(slot) = event.slots.get_mut(index) else {
                    ui.weak("Aucun DJ.");
                    return;
                };
                ui.heading(format!("Édition du DJ {}", index + 1));
                slot_form(ui, slot, library_ids, touch);
                ui.separator();
                cues(ui, slot, index, selection, looks, flagged, touch, ops);
            });
        });
    }
}

fn footer(ui: &mut egui::Ui, modified: bool, requests: &mut Vec<Request>) {
    ui.horizontal(|ui| {
        if modified {
            ui.colored_label(egui::Color32::YELLOW, "● Modifications non enregistrées");
        } else {
            ui.colored_label(egui::Color32::LIGHT_GREEN, "✓ Enregistré");
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let save = format!("Enregistrer  [{}]", ui.ctx().format_shortcut(&SAVE));
            if ui.button(save).clicked() {
                requests.push(Request::Save);
            }
            if ui
                .add_enabled(modified, egui::Button::new("Annuler"))
                .on_hover_text("Revient à la version enregistrée de cette fiche.")
                .clicked()
            {
                requests.push(Request::Revert);
            }
        });
    });
}

/// Les cues du DJ : liste réordonnable, puis le formulaire du cue choisi.
#[allow(clippy::too_many_arguments)] // Les morceaux de la fiche, empruntés séparément.
fn cues(
    ui: &mut egui::Ui,
    slot: &mut cf_show::model::DjSlot,
    slot_index: usize,
    selection: &mut Selection,
    looks: &[LookId],
    flagged: &[String],
    touch: &mut Touch,
    ops: &mut Vec<Op>,
) {
    ui.strong("Cues");
    let rows: Vec<ListRow> = slot
        .cues
        .iter()
        .enumerate()
        .map(|(i, cue)| ListRow {
            label: format!("{}. {}", i + 1, cue.name),
            selected: selection.cue == Some(i),
            flagged: path_flagged(flagged, &format!("slots[{slot_index}].cues[{i}]")),
        })
        .collect();
    let action = reorderable(ui, "cue_row", &rows, None);
    if let Some(i) = action.select {
        selection.cue = (selection.cue != Some(i)).then_some(i);
    }
    if let Some((from, to)) = action.moved.filter(|(from, to)| from != to) {
        ops.push(Op::MoveCue(from, to));
    }
    let cues_len = slot.cues.len();
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Cue").clicked() {
            ops.push(Op::AddCue);
        }
        let Some(cue_index) = selection.cue else {
            return;
        };
        if ui.button("Dupliquer le cue").clicked() {
            ops.push(Op::DuplicateCue(cue_index));
        }
        if ui
            .add_enabled(cues_len > 1, egui::Button::new("Supprimer le cue"))
            .clicked()
        {
            ops.push(Op::RemoveCue(cue_index));
        }
        if ui.add_enabled(cue_index > 0, egui::Button::new("↑")).clicked() {
            ops.push(Op::MoveCue(cue_index, cue_index - 1));
        }
        if ui
            .add_enabled(cue_index + 1 < cues_len, egui::Button::new("↓"))
            .clicked()
        {
            ops.push(Op::MoveCue(cue_index, cue_index + 1));
        }
    });
    let Some(cue) = selection.cue.and_then(|i| slot.cues.get_mut(i)) else {
        return;
    };
    ui.separator();
    ui.strong(format!("Cue {}", selection.cue.map_or(0, |i| i + 1)));
    cue_form(ui, cue, looks, touch);
}
