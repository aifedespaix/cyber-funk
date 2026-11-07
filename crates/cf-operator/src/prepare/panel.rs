//! Les pièces communes de l'édition : raccourcis d'historique, messages et problèmes de validation, modifications
//! structurelles. La mise en page (liste des DJ, fiche, profils) est celle de l'onglet Réglages
//! (`tabs::settings`).

use bevy_egui::egui;
use cf_show::model::Event;

use super::edit;
use super::session::{PrepSession, Selection, Status};

/// Raccourcis de la Préparation. Annuler/rétablir passent avant les champs texte : l'historique est global, celui
/// d'un champ isolé ferait doublon.
pub(crate) const UNDO: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
pub(crate) const REDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::Z);
const REDO_ALT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
pub(crate) const SAVE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);

/// Une modification structurelle demandée pendant l'image, appliquée une fois les formulaires dessinés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    AddSlot,
    DuplicateSlot(usize),
    RemoveSlot(usize),
    MoveSlot(usize, usize),
    AddCue,
    DuplicateCue(usize),
    RemoveCue(usize),
    MoveCue(usize, usize),
}

/// Annuler / rétablir au clavier, et fermeture du regroupement d'éditions. Rend `true` si `Ctrl+S` a été pressé
/// (l'appelant décide où enregistrer).
pub(crate) fn shortcuts(ctx: &egui::Context, session: &mut PrepSession) -> bool {
    // Plus de champ en cours de manipulation : la prochaine frappe ouvre une nouvelle étape d'annulation.
    if !ctx.egui_wants_keyboard_input() && !ctx.input(|i| i.pointer.any_down()) {
        session.break_group();
    }
    // Le plus spécifique d'abord : Ctrl+Z correspondrait aussi à Ctrl+Maj+Z.
    if ctx.input_mut(|i| i.consume_shortcut(&REDO) || i.consume_shortcut(&REDO_ALT)) {
        session.redo();
    } else if ctx.input_mut(|i| i.consume_shortcut(&UNDO)) {
        session.undo();
    }
    ctx.input_mut(|i| i.consume_shortcut(&SAVE))
}

/// Boutons Annuler / Rétablir de la barre.
pub(crate) fn history_buttons(ui: &mut egui::Ui, session: &mut PrepSession) {
    let undo_label = format!("Annuler  [{}]", ui.ctx().format_shortcut(&UNDO));
    if ui
        .add_enabled(session.can_undo(), egui::Button::new(undo_label))
        .clicked()
    {
        session.undo();
    }
    let redo_label = format!("Rétablir  [{}]", ui.ctx().format_shortcut(&REDO));
    if ui
        .add_enabled(session.can_redo(), egui::Button::new(redo_label))
        .clicked()
    {
        session.redo();
    }
}

/// Le dernier résultat d'une action de fichier, puis les problèmes de validation de l'événement.
pub(crate) fn messages(ui: &mut egui::Ui, session: &PrepSession) {
    match &session.status {
        Status::Quiet => {}
        Status::Info(text) => {
            ui.weak(text);
        }
        Status::Error(text) => {
            ui.colored_label(egui::Color32::LIGHT_RED, text);
        }
    }
    if !session.issues().is_empty() {
        egui::CollapsingHeader::new(format!("{} problème(s) dans l'événement", session.issues().len()))
            .default_open(true)
            .show(ui, |ui| {
                for issue in session.issues() {
                    ui.colored_label(egui::Color32::LIGHT_RED, format!("{} : {}", issue.path, issue.message));
                }
            });
    }
}

/// Les préfixes de chemin (`slots[1]`, `slots[1].cues[0]`) qui portent un problème, pour colorer les listes.
pub(crate) fn flagged_paths(session: &PrepSession) -> Vec<String> {
    session.issues().iter().map(|i| i.path.clone()).collect()
}

pub(crate) fn path_flagged(flagged: &[String], prefix: &str) -> bool {
    flagged.iter().any(|path| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
    })
}

/// Vrai si les problèmes du créneau `slot` ne concernent que ses cues (la ligne du créneau reste alors neutre).
pub(crate) fn path_flagged_cues_only(flagged: &[String], slot: usize) -> bool {
    let own = format!("slots[{slot}]");
    let cues = format!("slots[{slot}].cues");
    flagged
        .iter()
        .filter(|p| path_flagged(std::slice::from_ref(p), &own))
        .all(|p| p.starts_with(&cues))
}

/// Applique une modification structurelle au brouillon et renvoie la sélection qui suit.
pub(crate) fn apply(event: &mut Event, selection: Selection, op: Op) -> Selection {
    let slot = selection.slot;
    let keep = |cue| Selection {
        slot,
        cue,
        event: false,
    };
    let dj = |slot| Selection {
        slot,
        cue: None,
        event: false,
    };
    match op {
        Op::AddSlot => edit::add_slot(event).map_or(selection, dj),
        Op::DuplicateSlot(i) => edit::duplicate_slot(event, i).map_or(selection, dj),
        Op::RemoveSlot(i) => edit::remove_slot(event, i).map_or(selection, dj),
        Op::MoveSlot(from, to) => edit::move_slot(event, from, to).map_or(selection, dj),
        Op::AddCue => edit::add_cue(event, slot).map_or(selection, |i| keep(Some(i))),
        Op::DuplicateCue(i) => edit::duplicate_cue(event, slot, i).map_or(selection, |n| keep(Some(n))),
        Op::RemoveCue(i) => edit::remove_cue(event, slot, i).map_or(selection, |n| keep(Some(n))),
        Op::MoveCue(from, to) => edit::move_cue(event, slot, from, to).map_or(selection, |n| keep(Some(n))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_chemin_est_signale_pour_lui_et_ses_descendants() {
        let flagged = vec!["slots[1].cues[0].name".to_owned()];
        assert!(path_flagged(&flagged, "slots[1]"));
        assert!(path_flagged(&flagged, "slots[1].cues[0]"));
        assert!(!path_flagged(&flagged, "slots[1].cues[1]"));
        assert!(!path_flagged(&flagged, "slots[10]"));
    }

    #[test]
    fn les_problemes_de_cues_ne_colorent_pas_la_ligne_du_creneau() {
        let only_cues = vec!["slots[0].cues[2].look".to_owned()];
        assert!(path_flagged_cues_only(&only_cues, 0));
        let own = vec!["slots[0].dj.name".to_owned()];
        assert!(!path_flagged_cues_only(&own, 0));
    }
}
