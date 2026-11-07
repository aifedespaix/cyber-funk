//! L'aide `?` : la liste des raccourcis globaux, lue dans la table unique [`SHORTCUTS`] (celle des infobulles).
//!
//! Les raccourcis de la vue Live sont affichés sur ses propres boutons (`live::commands`) ; ceux du clavier par défaut
//! (cues, drop, DJ…) sont dans le panneau des contrôles de l'onglet Réglages.

use bevy_egui::egui;

use super::shortcuts::SHORTCUTS;

/// Les lignes de l'aide, dans l'ordre affiché.
pub(crate) fn rows() -> &'static [(&'static str, &'static str)] {
    SHORTCUTS
}

/// La fenêtre d'aide, tant que `open` est vrai (sa croix la ferme).
pub(crate) fn window(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    egui::Window::new("Raccourcis")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 72.0))
        .show(ctx, draw);
}

/// Le contenu de l'aide (séparé de la fenêtre pour être testé sans écran).
pub(crate) fn draw(ui: &mut egui::Ui) {
    egui::Grid::new("shell_help")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            for (key, what) in rows() {
                ui.monospace(*key);
                ui.label(*what);
                ui.end_row();
            }
        });
    ui.separator();
    ui.weak("Vue Live : le raccourci de chaque bouton est écrit dessus.");
    ui.weak("Clavier par défaut (cues, drop, DJ…) : onglet Réglages, panneau des contrôles.");
}
