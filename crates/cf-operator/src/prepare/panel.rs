//! Le panneau de Préparation : barre de fichier, listes à gauche, formulaire de la sélection à droite.

use std::path::PathBuf;

use bevy_egui::egui;
use cf_show::model::Event;

use super::cue_form::cue_form;
use super::edit;
use super::forms::{event_form, slot_form};
use super::lists::{ListRow, reorderable};
use super::session::{PrepSession, Selection, Status};
use super::widgets::Touch;

/// Raccourcis de la Préparation. Annuler/rétablir passent avant les champs texte : l'historique est global, celui
/// d'un champ isolé ferait doublon.
const UNDO: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
const REDO: egui::KeyboardShortcut =
    egui::KeyboardShortcut::new(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::Z);
const REDO_ALT: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
const SAVE: egui::KeyboardShortcut = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);

/// Une modification structurelle demandée pendant l'image, appliquée une fois les formulaires dessinés.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    AddSlot,
    DuplicateSlot(usize),
    RemoveSlot(usize),
    MoveSlot(usize, usize),
    AddCue,
    DuplicateCue(usize),
    RemoveCue(usize),
    MoveCue(usize, usize),
}

/// Dessine la Préparation. `mode_button` ajoute le bouton « Passer en Live » à la barre.
pub(crate) fn draw(ui: &mut egui::Ui, session: &mut PrepSession, mode_button: impl FnOnce(&mut egui::Ui)) {
    shortcuts(ui.ctx(), session);
    toolbar(ui, session, mode_button);
    messages(ui, session);
    ui.separator();

    let mut draft = session.event().clone();
    let mut touch = Touch::default();
    let mut selection = session.selection;
    let mut ops = Vec::new();
    let looks = session.look_ids();
    let flagged = flagged_paths(session);

    egui::Panel::left("prep_lists")
        .default_size(280.0)
        .show(ui, |ui| lists(ui, &draft, &mut selection, &flagged, &mut ops));
    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical().show(ui, |ui| editor(ui, &mut draft, selection, &looks, &mut touch, &mut ops));
    });

    for op in ops {
        selection = apply(&mut draft, selection, op);
        touch.structural();
    }
    session.selection = selection;
    session.commit(draft, touch.key());
}

fn shortcuts(ctx: &egui::Context, session: &mut PrepSession) {
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
    if ctx.input_mut(|i| i.consume_shortcut(&SAVE)) {
        save_or_ask(session);
    }
}

/// Enregistre dans le fichier associé ; sans fichier, utilise le chemin saisi dans la barre.
fn save_or_ask(session: &mut PrepSession) {
    if session.save() {
        return;
    }
    let typed = session.path_input.trim().to_owned();
    if typed.is_empty() {
        session.status = Status::Error("Indique d'abord un chemin de fichier (Enregistrer sous).".to_owned());
    } else {
        session.save_as(&PathBuf::from(typed));
    }
}

fn toolbar(ui: &mut egui::Ui, session: &mut PrepSession, mode_button: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.heading("Préparation");
        if session.is_dirty() {
            ui.colored_label(egui::Color32::YELLOW, "● modifié");
        }
        mode_button(ui);
    });
    ui.horizontal_wrapped(|ui| {
        ui.label("Fichier");
        ui.add(
            egui::TextEdit::singleline(&mut session.path_input)
                .desired_width(320.0)
                .hint_text("soiree.cfshow"),
        );
        let typed = PathBuf::from(session.path_input.trim());
        let has_path = !session.path_input.trim().is_empty();
        if ui.add_enabled(has_path, egui::Button::new("Ouvrir")).clicked() {
            session.open(&typed);
        }
        let save_label = format!("Enregistrer  [{}]", ui.ctx().format_shortcut(&SAVE));
        if ui.button(save_label).clicked() {
            save_or_ask(session);
        }
        if ui
            .add_enabled(has_path, egui::Button::new("Enregistrer sous"))
            .clicked()
        {
            session.save_as(&typed);
        }
        ui.separator();
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
    });
}

fn messages(ui: &mut egui::Ui, session: &PrepSession) {
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
fn flagged_paths(session: &PrepSession) -> Vec<String> {
    session.issues().iter().map(|i| i.path.clone()).collect()
}

fn path_flagged(flagged: &[String], prefix: &str) -> bool {
    flagged.iter().any(|path| {
        path.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.') || rest.starts_with('['))
    })
}

fn lists(ui: &mut egui::Ui, event: &Event, selection: &mut Selection, flagged: &[String], ops: &mut Vec<Op>) {
    if ui
        .selectable_label(selection.event, "Réglages de l'événement")
        .clicked()
    {
        selection.event = true;
    }
    ui.separator();
    ui.strong("Créneaux");
    let rows: Vec<ListRow> = event
        .slots
        .iter()
        .enumerate()
        .map(|(i, slot)| ListRow {
            label: format!(
                "{}. {}",
                i + 1,
                if slot.dj.name.is_empty() {
                    slot.id.as_str()
                } else {
                    &slot.dj.name
                }
            ),
            selected: !selection.event && selection.slot == i && selection.cue.is_none(),
            flagged: path_flagged(flagged, &format!("slots[{i}]")) && !path_flagged_cues_only(flagged, i),
        })
        .collect();
    let action = reorderable(ui, "slot_row", &rows);
    if let Some(i) = action.select {
        *selection = Selection {
            slot: i,
            cue: None,
            event: false,
        };
    }
    if let Some((from, to)) = action.moved.filter(|(from, to)| from != to) {
        ops.push(Op::MoveSlot(from, to));
    }
    if ui.button("+ Créneau").clicked() {
        ops.push(Op::AddSlot);
    }

    let Some(slot) = event.slots.get(selection.slot) else {
        return;
    };
    ui.separator();
    ui.strong(format!(
        "Cues de {}",
        if slot.dj.name.is_empty() {
            slot.id.as_str()
        } else {
            &slot.dj.name
        }
    ));
    let rows: Vec<ListRow> = slot
        .cues
        .iter()
        .enumerate()
        .map(|(i, cue)| ListRow {
            label: format!("{}. {}", i + 1, cue.name),
            selected: !selection.event && selection.cue == Some(i),
            flagged: path_flagged(flagged, &format!("slots[{}].cues[{i}]", selection.slot)),
        })
        .collect();
    let action = reorderable(ui, "cue_row", &rows);
    if let Some(i) = action.select {
        *selection = Selection {
            slot: selection.slot,
            cue: Some(i),
            event: false,
        };
    }
    if let Some((from, to)) = action.moved.filter(|(from, to)| from != to) {
        ops.push(Op::MoveCue(from, to));
    }
    if ui.button("+ Cue").clicked() {
        ops.push(Op::AddCue);
    }
}

/// Vrai si les problèmes du créneau `slot` ne concernent que ses cues (la ligne du créneau reste alors neutre).
fn path_flagged_cues_only(flagged: &[String], slot: usize) -> bool {
    let own = format!("slots[{slot}]");
    let cues = format!("slots[{slot}].cues");
    flagged
        .iter()
        .filter(|p| path_flagged(std::slice::from_ref(p), &own))
        .all(|p| p.starts_with(&cues))
}

fn editor(
    ui: &mut egui::Ui,
    event: &mut Event,
    selection: Selection,
    looks: &[cf_show::model::LookId],
    touch: &mut Touch,
    ops: &mut Vec<Op>,
) {
    if selection.event {
        ui.heading("Événement");
        event_form(ui, event, looks, touch);
        return;
    }
    let slot_index = selection.slot;
    let slots_len = event.slots.len();
    let Some(slot) = event.slots.get_mut(slot_index) else {
        ui.weak("Aucun créneau.");
        return;
    };
    match selection.cue {
        None => {
            ui.heading(format!("Créneau {}", slot_index + 1));
            ui.horizontal(|ui| {
                if ui.button("Dupliquer").clicked() {
                    ops.push(Op::DuplicateSlot(slot_index));
                }
                if ui.add_enabled(slots_len > 1, egui::Button::new("Supprimer")).clicked() {
                    ops.push(Op::RemoveSlot(slot_index));
                }
                if ui.add_enabled(slot_index > 0, egui::Button::new("↑")).clicked() {
                    ops.push(Op::MoveSlot(slot_index, slot_index - 1));
                }
                if ui
                    .add_enabled(slot_index + 1 < slots_len, egui::Button::new("↓"))
                    .clicked()
                {
                    ops.push(Op::MoveSlot(slot_index, slot_index + 1));
                }
            });
            slot_form(ui, slot, touch);
        }
        Some(cue_index) => {
            let cues_len = slot.cues.len();
            let Some(cue) = slot.cues.get_mut(cue_index) else {
                return;
            };
            ui.heading(format!("Cue {}", cue_index + 1));
            ui.horizontal(|ui| {
                if ui.button("Dupliquer").clicked() {
                    ops.push(Op::DuplicateCue(cue_index));
                }
                if ui.add_enabled(cues_len > 1, egui::Button::new("Supprimer")).clicked() {
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
            cue_form(ui, cue, looks, touch);
        }
    }
}

/// Applique une modification structurelle au brouillon et renvoie la sélection qui suit.
fn apply(event: &mut Event, selection: Selection, op: Op) -> Selection {
    let slot = selection.slot;
    let keep = |cue| Selection {
        slot,
        cue,
        event: false,
    };
    match op {
        Op::AddSlot => edit::add_slot(event).map_or(selection, |i| Selection {
            slot: i,
            cue: None,
            event: false,
        }),
        Op::DuplicateSlot(i) => edit::duplicate_slot(event, i).map_or(selection, |n| Selection {
            slot: n,
            cue: None,
            event: false,
        }),
        Op::RemoveSlot(i) => edit::remove_slot(event, i).map_or(selection, |n| Selection {
            slot: n,
            cue: None,
            event: false,
        }),
        Op::MoveSlot(from, to) => edit::move_slot(event, from, to).map_or(selection, |n| Selection {
            slot: n,
            cue: None,
            event: false,
        }),
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
