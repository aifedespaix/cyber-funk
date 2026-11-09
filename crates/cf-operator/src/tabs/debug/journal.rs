//! Le bandeau « Journal » de l'onglet Debug : ce que le monde a fait des dernières demandes.
//!
//! Chaque clic « Lancer » (ou note MIDI) y laisse une ligne : en attente, lancé, ou **refusé avec la raison** (« une
//! structure occupe déjà la route », « une arche est trop proche »…). C'est ce qui manquait : un événement qui ne
//! s'affichait pas ne disait pas pourquoi. Le bandeau ne lit que le journal ; il n'écrit rien.

use bevy_egui::egui;
use cf_render::{EventJournal, JournalEntry, JournalState};

/// Hauteur du bandeau (px).
pub(crate) const JOURNAL_HEIGHT: f32 = 118.0;

/// Nombre de lignes montrées (les plus récentes).
const LINES: usize = 12;

/// Hauteur de fenêtre en dessous de laquelle le bandeau se replie (la grille a priorité).
pub(crate) const MIN_HEIGHT_FOR_JOURNAL: f32 = 420.0;

const OK: egui::Color32 = egui::Color32::from_rgb(110, 210, 140);
const REFUSED: egui::Color32 = egui::Color32::from_rgb(235, 100, 100);
const WAITING: egui::Color32 = egui::Color32::from_rgb(230, 190, 90);

/// La couleur, le symbole et le texte d'une ligne.
pub(crate) fn describe(entry: &JournalEntry) -> (egui::Color32, &'static str, String) {
    match entry.state {
        JournalState::Started => (OK, "✔", "lancé".to_owned()),
        JournalState::Pending => (WAITING, "…", "en attente".to_owned()),
        JournalState::Refused(reason) => (REFUSED, "✖", format!("refusé : {}", reason.text())),
    }
}

/// Le bandeau. `None` : le journal est absent (monde absent), on le dit.
pub(crate) fn draw(ui: &mut egui::Ui, journal: Option<&EventJournal>) {
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_height(JOURNAL_HEIGHT - 16.0);
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.strong("Journal");
            ui.weak("ce que le monde a fait de tes demandes");
        });
        let Some(journal) = journal else {
            ui.weak("Journal absent (monde absent).");
            return;
        };
        if journal.is_empty() {
            ui.weak("Aucune demande pour l'instant.");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("debug-journal")
            .max_height(JOURNAL_HEIGHT - 40.0)
            .show(ui, |ui| {
                for entry in journal.iter().rev().take(LINES) {
                    let (color, mark, text) = describe(entry);
                    ui.horizontal(|ui| {
                        ui.colored_label(color, mark);
                        ui.monospace(format!("{:7.1}s", entry.at_s));
                        ui.label(egui::RichText::new(&entry.event_id).strong());
                        ui.colored_label(color, text);
                    });
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::{EventOrigin, RefusalReason};

    #[test]
    fn a_refusal_shows_its_reason() {
        let mut journal = EventJournal::default();
        journal.request(1.0, "tower");
        journal.refused(1.0, "tower", RefusalReason::StructureBusy);
        let entry = journal.iter().next();
        let (_, mark, text) = describe(entry.expect("une ligne"));
        assert_eq!(mark, "✖");
        assert!(text.contains("structure"), "{text}");
    }

    #[test]
    fn a_started_line_is_green_and_a_pending_one_is_amber() {
        let mut journal = EventJournal::default();
        journal.started(1.0, "chase", EventOrigin::Manual);
        journal.request(2.0, "storm");
        let lines: Vec<_> = journal.iter().map(describe).collect();
        assert_eq!(lines[0].0, OK);
        assert_eq!(lines[1].0, WAITING);
    }
}
