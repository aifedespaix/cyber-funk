//! Import / export de la soirée : ouvrir ou écrire un fichier d'événement RON choisi par son chemin (le champ texte de
//! la barre de fichier d'avant ; aucune boîte de dialogue native, donc aucune dépendance de plus).
//!
//! * **Importer** (`EventDocument::open`) : le contenu remplace l'édition et reste à enregistrer dans le profil actif.
//! * **Exporter** (`save_as` d'une copie) : écrit l'événement ailleurs sans changer de profil ni d'état « modifié ».

use std::path::PathBuf;

use bevy_egui::egui;

use super::{Pending, SettingsTab};
use crate::prepare::{PrepSession, Status};

/// La barre d'import / export.
pub(crate) fn bar(ui: &mut egui::Ui, session: &mut PrepSession, tab: &mut SettingsTab) {
    ui.horizontal_wrapped(|ui| {
        ui.label("Fichier");
        ui.add(
            egui::TextEdit::singleline(&mut session.path_input)
                .desired_width(320.0)
                .hint_text("chemin/vers/soiree.ron"),
        );
        let typed = session.path_input.trim().to_owned();
        let has_path = !typed.is_empty();
        if ui
            .add_enabled(has_path, egui::Button::new("Importer"))
            .on_hover_text("Ouvre ce fichier d'événement dans l'édition (à enregistrer ensuite dans le profil).")
            .clicked()
        {
            super::guard(session, tab, Pending::Import(PathBuf::from(&typed)));
        }
        if ui
            .add_enabled(has_path, egui::Button::new("Exporter"))
            .on_hover_text("Écrit une copie de l'événement dans ce fichier (le profil actif ne change pas).")
            .clicked()
        {
            export(session, &PathBuf::from(&typed));
        }
    });
}

/// Exporte, sauf un événement invalide (il ne se rouvrirait pas).
fn export(session: &mut PrepSession, path: &std::path::Path) -> bool {
    if !session.issues().is_empty() {
        session.status = Status::Error(format!(
            "Export refusé : {} problème(s) à corriger dans l'événement.",
            session.issues().len()
        ));
        return false;
    }
    session.export(path)
}
