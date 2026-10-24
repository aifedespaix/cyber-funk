//! La vue Live : mise en page de `docs/ARCHITECTURE.md` §2.5, édition verrouillée.
//!
//! ```text
//! haut      frise des créneaux  ◀ DJ précédent [ Aya ▸ KRMN (en cours) ▸ Lune ] DJ suivant ▶
//! centre    liste de cues (joué / en cours / suivant, compte à rebours)  +  GO · BACK · HOLD
//! droite    tempo et source, vumètres, section, drop préparé, santé (i/s, GPU, MIDI)
//! bas       macros et master, pads, sécurité (Blackout, Look calme, Flashs)
//! ```
//!
//! Tout est lu dans `ShowState` et la feuille de conduite ; rien n'est tenu en copie. `draw` ne touche pas à Bevy : il
//! écrit des `Action` dans `Out::orders` et se teste sans écran. Aucun écran de cette vue n'est dessiné dans le
//! projecteur : elle vit dans le contexte egui de la fenêtre opérateur.

use bevy_egui::egui::{self, Color32, RichText};
use cf_control::Action;
use cf_render::audio::AudioFeatures;
use cf_render::{ControlsStatus, DiagnosticsSnapshot, ShowSheet};
use cf_show::director::{HandoverPhase, ShowState};
use cf_show::drop::DropPlan;
use cf_show::model::SlotMode;

use super::commands::{CommandContext, LiveCommand, MACROS};
use super::text::{self, CueStatus};
use super::widgets::{
    ACTIVE_FILL, BIG_BUTTON_HEIGHT, BUTTON_HEIGHT, CURRENT_FILL, PLAYED_TEXT, WARN_TEXT, action_button, apply_style,
    command_button,
};
use super::{bottom, side};

/// Largeur initiale de la colonne de droite.
const SIDE_WIDTH: f32 = 470.0;

/// Tout ce que la vue lit, empruntée à l'image courante.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LiveData<'a> {
    pub state: &'a ShowState,
    pub sheet: &'a ShowSheet,
    pub features: Option<&'a AudioFeatures>,
    pub controls: Option<&'a ControlsStatus>,
    pub diagnostics: Option<&'a DiagnosticsSnapshot>,
    pub drop_plan: Option<&'a DropPlan>,
}

impl<'a> LiveData<'a> {
    /// Ce dont les commandes ont besoin pour devenir des actions.
    pub(crate) fn command_context(&self) -> CommandContext<'a> {
        let slot = self.sheet.slot(self.state.slot);
        let cue = slot.and_then(|s| s.cues.get(self.state.cue));
        CommandContext {
            slot: self.state.slot,
            slot_count: self.sheet.slots.len(),
            armed: self.state.armed_slot,
            cue_name: cue.map_or("", |c| c.name.as_str()),
            macro_count: cue.map_or(0, |c| c.macros.len().min(MACROS)),
        }
    }
}

/// Dessine la vue dans `root` (le `Ui` racine de la fenêtre opérateur).
pub(crate) fn draw(root: &mut egui::Ui, data: &LiveData<'_>, orders: &mut Vec<Action>, mode_request: &mut bool) {
    apply_style(root.style_mut());
    let ctx = data.command_context();
    egui::Panel::top("live_slots").show(root, |ui| slots_bar(ui, data, &ctx, orders, mode_request));
    egui::Panel::bottom("live_bottom").show(root, |ui| bottom::draw(ui, data, &ctx, orders));
    egui::Panel::right("live_side")
        .default_size(SIDE_WIDTH)
        .show(root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| side::draw(ui, data, &ctx, orders));
        });
    egui::CentralPanel::default().show(root, |ui| cues_panel(ui, data, &ctx, orders));
}

/// La frise : DJ précédent, un bouton par créneau (un clic arme), DJ suivant, et le bouton de mode.
fn slots_bar(
    ui: &mut egui::Ui,
    data: &LiveData<'_>,
    ctx: &CommandContext<'_>,
    orders: &mut Vec<Action>,
    mode_request: &mut bool,
) {
    let state = data.state;
    let size = egui::vec2(0.0, BUTTON_HEIGHT);
    ui.horizontal_wrapped(|ui| {
        action_button(ui, LiveCommand::PrevDj, ctx, orders, size, false);
        for (index, slot) in data.sheet.slots.iter().enumerate() {
            let name = text::dj_name(&slot.dj, index);
            let hours = slot
                .schedule
                .map_or_else(String::new, |(start, _)| format!("{} ", text::hour_minute(start)));
            let (label, fill) = if index == state.slot {
                let mut label = format!("{hours}{name} · en cours {}", text::clock(state.elapsed));
                if let Some(h) = &state.handover {
                    label.push_str(match h.phase {
                        HandoverPhase::Downbeat => " · passation : attend le temps fort",
                        HandoverPhase::Fade => " · passation en cours",
                    });
                }
                (label, Some(CURRENT_FILL))
            } else if state.armed_slot == Some(index) {
                (format!("{hours}{name} · ARMÉ"), Some(ACTIVE_FILL))
            } else {
                (format!("{hours}{name}"), None)
            };
            let mut button = egui::Button::new(RichText::new(label).strong());
            if let Some(fill) = fill {
                button = button.fill(fill);
            }
            // Un clic sur un autre créneau l'arme : le prochain « DJ suivant » y mène.
            let response = ui.add_enabled(index != state.slot, button.min_size(size));
            if response.clicked() {
                if let Ok(rank) = u16::try_from(index) {
                    orders.push(Action::ArmDj(cf_control::DjIndex(rank)));
                }
            }
        }
        action_button(ui, LiveCommand::NextDj, ctx, orders, size, false);
        action_button(ui, LiveCommand::ArmNext, ctx, orders, size, false);
        if command_button(ui, LiveCommand::SwitchMode, size, false, true) {
            *mode_request = true;
        }
    });
    if !data.sheet.event_name.is_empty() {
        ui.label(format!("{} · mode {}", data.sheet.event_name, mode_text(state.mode)));
    }
}

fn mode_text(mode: SlotMode) -> &'static str {
    match mode {
        SlotMode::Auto => "auto",
        SlotMode::Semi => "semi-auto",
        SlotMode::Manual => "manuel",
    }
}

/// La liste de cues du créneau en cours et les trois gros boutons de conduite.
fn cues_panel(ui: &mut egui::Ui, data: &LiveData<'_>, ctx: &CommandContext<'_>, orders: &mut Vec<Action>) {
    let state = data.state;
    let slot = data.sheet.slot(state.slot);
    let dj = slot.map_or_else(String::new, |s| text::dj_name(&s.dj, state.slot));
    ui.heading(format!("CUES de {dj}"));
    let called = text::called_cue(state);
    let list_height = (ui.available_height() - BIG_BUTTON_HEIGHT - 24.0).max(120.0);
    egui::ScrollArea::vertical().max_height(list_height).show(ui, |ui| {
        let Some(slot) = slot else {
            ui.colored_label(WARN_TEXT, "Créneau inconnu de la feuille de conduite.");
            return;
        };
        for (index, cue) in slot.cues.iter().enumerate() {
            let status = text::cue_status(state.cue, called, index);
            cue_row(ui, index, cue, status, state, called);
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 20.0) / 3.0;
        let size = egui::vec2(width, BIG_BUTTON_HEIGHT);
        action_button(ui, LiveCommand::Go, ctx, orders, size, false);
        action_button(ui, LiveCommand::Back, ctx, orders, size, false);
        action_button(ui, LiveCommand::Hold, ctx, orders, size, state.hold);
    });
}

/// Une ligne de la liste : marque, numéro, nom, état, règles d'entrée et de suite, compte à rebours.
fn cue_row(
    ui: &mut egui::Ui,
    index: usize,
    cue: &cf_render::SheetCue,
    status: CueStatus,
    state: &ShowState,
    called: Option<usize>,
) {
    let (mark, tag, text_color) = match status {
        CueStatus::Played => ("✔", "joué", Some(PLAYED_TEXT)),
        CueStatus::Current => ("▶", "en cours", None),
        CueStatus::Next if called == Some(index) => ("»", "appelé", None),
        CueStatus::Next => ("·", "suivant", None),
        CueStatus::Later => (" ", "", None),
    };
    let mut frame = egui::Frame::group(ui.style());
    if status == CueStatus::Current {
        frame = frame.fill(CURRENT_FILL);
    }
    frame.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            let mut title = RichText::new(format!("{mark} {:>2}  {}", index + 1, cue.name)).strong();
            if let Some(color) = text_color {
                title = title.color(color);
            }
            ui.label(title);
            if !tag.is_empty() {
                ui.label(RichText::new(tag).color(text_color.unwrap_or(Color32::WHITE)));
            }
        });
        // Le détail sert avant tout au cue courant et à ceux qui viennent ; un cue joué reste lisible sans s'étaler.
        if status != CueStatus::Played {
            let rules = format!("{} · {}", text::entry_text(cue.entry), text::advance_text(cue.advance));
            ui.label(RichText::new(rules).color(Color32::from_rgb(190, 196, 210)));
        }
        if status == CueStatus::Current {
            let pending = state.pending.as_ref().map(|p| (p.entry, p.waited));
            if let Some(line) = text::countdown_text(pending, state.hold, state.next_cue_in) {
                let color = if state.hold || pending.is_some() {
                    WARN_TEXT
                } else {
                    Color32::WHITE
                };
                ui.label(RichText::new(line).strong().color(color));
            }
            if let Some(progress) = state.cue_progress {
                ui.add(egui::ProgressBar::new(progress.clamp(0.0, 1.0)).desired_width(ui.available_width()));
            }
        }
    });
}
