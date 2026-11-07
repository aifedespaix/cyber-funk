//! La carte « DJs de la soirée » : la liste des créneaux, réordonnable au glisser-déposer et par `Alt+↑/↓` sur la
//! ligne qui a le focus, ajout, duplication, suppression (refusée pour le dernier, règle de `edit::remove_slot`).
//!
//! `Alt+↑/↓` ne croise aucun raccourci : `cf-control` ne lie que ←/→ (GO, retour) et jamais avec Alt, et la coque
//! n'utilise ni les flèches ni Alt.

use bevy_egui::egui;
use cf_show::model::{DjSlot, Event};

use super::{Pending, Request, SettingsTab};
use crate::prepare::PrepSession;
use crate::prepare::Selection;
use crate::prepare::lists::{ListRow, reorderable};
use crate::prepare::panel::{Op, path_flagged, path_flagged_cues_only};

/// Focus clavier de la liste : la ligne focalisée à l'image précédente, et celle qui doit le recevoir (le focus suit
/// une ligne déplacée au clavier).
#[derive(Debug, Default)]
pub(crate) struct ListFocus {
    pub focused: Option<usize>,
    pub request: Option<usize>,
}

/// Durée prévue d'un créneau en minutes (horaires qui passent minuit compris), `None` sans horaires.
pub(crate) fn slot_minutes(slot: &DjSlot) -> Option<u32> {
    let schedule = slot.dj.schedule.as_ref()?;
    let start = u32::from(schedule.start.minutes_since_midnight());
    let end = u32::from(schedule.end.minutes_since_midnight());
    Some((end + 24 * 60 - start) % (24 * 60))
}

/// Le nom affiché d'un créneau (son identifiant si le nom est vide).
pub(crate) fn slot_label(slot: &DjSlot) -> &str {
    if slot.dj.name.is_empty() {
        slot.id.as_str()
    } else {
        &slot.dj.name
    }
}

/// Le DJ `target` est demandé : changement immédiat, ou confirmation si la fiche affichée (un DJ) a des
/// modifications non enregistrées. Rend `true` si la sélection a changé.
pub(crate) fn select_dj(session: &mut PrepSession, tab: &mut SettingsTab, target: usize) -> bool {
    let current = session.selection;
    if !current.event && current.slot != target && session.slot_modified(current.slot) {
        tab.pending = Some(Pending::SwitchDj(target));
        return false;
    }
    session.selection = Selection {
        slot: target,
        cue: None,
        event: false,
    };
    true
}

/// Dessine la carte. Les gestes sont rendus dans `ops` (structure) et `requests` (sélection d'un DJ).
pub(crate) fn draw(
    ui: &mut egui::Ui,
    event: &Event,
    selection: &mut Selection,
    flagged: &[String],
    focus: &mut ListFocus,
    ops: &mut Vec<Op>,
    requests: &mut Vec<Request>,
) {
    keyboard_moves(ui.ctx(), event, focus, ops);
    let total: u32 = event.slots.iter().filter_map(slot_minutes).sum();
    ui.horizontal(|ui| {
        ui.heading("DJs de la soirée");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.weak(format!("{} · {total} min", event.slots.len()));
        });
    });
    if ui
        .selectable_label(selection.event, "Réglages de l'événement")
        .clicked()
    {
        selection.event = true;
        selection.cue = None;
    }
    ui.separator();
    let rows: Vec<ListRow> = event
        .slots
        .iter()
        .enumerate()
        .map(|(i, slot)| ListRow {
            label: match slot_minutes(slot) {
                Some(minutes) => format!("{}. {}  ·  {minutes} min", i + 1, slot_label(slot)),
                None => format!("{}. {}", i + 1, slot_label(slot)),
            },
            selected: !selection.event && selection.slot == i,
            flagged: path_flagged(flagged, &format!("slots[{i}]")) && !path_flagged_cues_only(flagged, i),
        })
        .collect();
    egui::ScrollArea::vertical()
        .id_salt("dj_rows")
        .max_height((ui.available_height() - 90.0).max(48.0))
        .show(ui, |ui| {
            let action = reorderable(ui, "dj_row", &rows, focus.request.take());
            focus.focused = action.focused;
            if let Some(i) = action.select
                && (selection.event || selection.slot != i || selection.cue.is_some())
            {
                requests.push(Request::SelectDj(i));
            }
            if let Some((from, to)) = action.moved.filter(|(from, to)| from != to) {
                ops.push(Op::MoveSlot(from, to));
            }
        });
    ui.weak("Glisser, ou Alt+↑/↓ sur la ligne, pour réordonner.");
    ui.horizontal_wrapped(|ui| {
        if ui.button("+ Ajouter un DJ").clicked() {
            ops.push(Op::AddSlot);
        }
        let on_dj = !selection.event;
        if ui.add_enabled(on_dj, egui::Button::new("Dupliquer")).clicked() {
            ops.push(Op::DuplicateSlot(selection.slot));
        }
        if ui
            .add_enabled(on_dj && event.slots.len() > 1, egui::Button::new("Supprimer"))
            .on_disabled_hover_text("Une soirée garde au moins un DJ.")
            .clicked()
        {
            ops.push(Op::RemoveSlot(selection.slot));
        }
    });
}

/// `Alt+↑/↓` sur la ligne focalisée (état de l'image précédente) : déplace le DJ et garde le focus sur lui.
fn keyboard_moves(ctx: &egui::Context, event: &Event, focus: &mut ListFocus, ops: &mut Vec<Op>) {
    let Some(row) = focus.focused.filter(|&i| i < event.slots.len()) else {
        return;
    };
    let up = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowUp));
    let down = ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, egui::Key::ArrowDown));
    let target = if up && row > 0 {
        row - 1
    } else if down && row + 1 < event.slots.len() {
        row + 1
    } else {
        return;
    };
    ops.push(Op::MoveSlot(row, target));
    focus.request = Some(target);
}
