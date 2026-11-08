//! La fiche du DJ sélectionné (ou les réglages de l'événement) : nom, logo et thème pour un DJ, avec un pied fixe :
//! état « modifications non enregistrées », `Annuler` (revient à la version enregistrée) et `Enregistrer` (`Ctrl+S`).
//!
//! La conduite d'un DJ (cues, mode, profil audio) est automatique : elle n'est plus proposée ici.

use bevy_egui::egui;
use cf_show::model::{Event, LookId};

use super::Request;
use crate::prepare::Selection;
use crate::prepare::forms::{event_form, slot_form};
use crate::prepare::panel::SAVE;
use crate::prepare::widgets::Touch;

/// La fiche, pour une image.
pub(crate) struct DjFormView<'a> {
    pub event: &'a mut Event,
    pub selection: &'a mut Selection,
    pub looks: &'a [LookId],
    pub library_ids: &'a [String],
    /// La fiche affichée diffère de sa version enregistrée.
    pub modified: bool,
    pub touch: &'a mut Touch,
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
            modified,
            touch,
            requests,
        } = self;
        egui::Panel::bottom("dj_form_footer").show(ui, |ui| footer(ui, modified, requests));
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("dj_form").show(ui, |ui| {
                if selection.event {
                    ui.heading("Réglages de la soirée");
                    event_form(ui, event, looks, library_ids, touch);
                    return;
                }
                let index = selection.slot;
                let Some(slot) = event.slots.get_mut(index) else {
                    ui.weak("Aucun DJ.");
                    return;
                };
                ui.heading(format!("DJ {}", index + 1));
                slot_form(ui, slot, touch);
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
