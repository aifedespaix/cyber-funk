//! Bas de la vue Live : macros et master, pads, sécurité.

use bevy_egui::egui::{self, RichText};
use cf_control::{Action, Level, MacroIndex};

use super::commands::{CommandContext, LiveCommand};
use super::view::LiveData;
use super::widgets::{WARN_TEXT, action_button};

/// Hauteur des gros boutons de pad et de sécurité.
const PAD_HEIGHT: f32 = 52.0;
/// Largeur d'un potard, en points.
const KNOB_WIDTH: f32 = 130.0;

pub(crate) fn draw(ui: &mut egui::Ui, data: &LiveData<'_>, ctx: &CommandContext<'_>, orders: &mut Vec<Action>) {
    macros_row(ui, data, orders);
    ui.separator();
    pads_row(ui, ctx, orders);
    safety_row(ui, data, ctx, orders);
}

/// Les macros du look en cours et le master : un curseur à la souris, deux touches au clavier.
fn macros_row(ui: &mut egui::Ui, data: &LiveData<'_>, orders: &mut Vec<Action>) {
    let state = data.state;
    let names = data
        .sheet
        .slot(state.slot)
        .and_then(|s| s.cues.get(state.cue))
        .map(|c| c.macros.as_slice())
        .unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.strong("MACROS");
        if names.is_empty() {
            ui.label("aucune dans ce look");
        }
        for (index, name) in names.iter().enumerate().take(super::commands::MACROS) {
            let Some(knob) = state.macros.get(index) else {
                break;
            };
            let Ok(rank) = u8::try_from(index) else {
                break;
            };
            ui.vertical(|ui| {
                ui.set_width(KNOB_WIDTH);
                // « touché » : le potard pilote le look ; sinon il montre la position qui reproduit ses valeurs.
                let label = if knob.engaged {
                    name.clone()
                } else {
                    format!("{name} (look)")
                };
                ui.label(RichText::new(label).strong());
                let mut value = knob.value;
                if ui
                    .add(egui::Slider::new(&mut value, 0.0..=1.0).show_value(false))
                    .changed()
                {
                    orders.push(Action::Macro {
                        index: MacroIndex(rank),
                        value: Level::Set(value),
                    });
                }
                ui.label(format!(
                    "{} / {}",
                    LiveCommand::MacroUp(rank).shortcut_text(),
                    LiveCommand::MacroDown(rank).shortcut_text()
                ));
            });
        }
        ui.separator();
        ui.vertical(|ui| {
            ui.set_width(KNOB_WIDTH + 20.0);
            ui.label(RichText::new(format!("MASTER {:.0} %", state.master * 100.0)).strong());
            let mut value = state.master;
            if ui
                .add(egui::Slider::new(&mut value, 0.0..=1.0).show_value(false))
                .changed()
            {
                orders.push(Action::Master(Level::Set(value)));
            }
            ui.label(format!(
                "{} / {}",
                LiveCommand::MasterUp.shortcut_text(),
                LiveCommand::MasterDown.shortcut_text()
            ));
        });
    });
}

fn pads_row(ui: &mut egui::Ui, ctx: &CommandContext<'_>, orders: &mut Vec<Action>) {
    ui.horizontal_wrapped(|ui| {
        ui.strong("PADS");
        let size = egui::vec2(0.0, PAD_HEIGHT);
        for command in [
            LiveCommand::ForceDrop,
            LiveCommand::Title,
            LiveCommand::ShowDj,
            LiveCommand::RandomEvent,
            LiveCommand::SkipShot,
            LiveCommand::Ascension,
        ] {
            action_button(ui, command, ctx, orders, size, false);
        }
    });
}

fn safety_row(ui: &mut egui::Ui, data: &LiveData<'_>, ctx: &CommandContext<'_>, orders: &mut Vec<Action>) {
    let state = data.state;
    ui.horizontal_wrapped(|ui| {
        ui.strong("SÉCURITÉ");
        let size = egui::vec2(0.0, PAD_HEIGHT);
        action_button(ui, LiveCommand::Blackout, ctx, orders, size, state.blackout_on);
        action_button(ui, LiveCommand::SafeLook, ctx, orders, size, state.safe_look);
        // Bascule « flashs coupés » : allumée quand les flashs sont interdits.
        action_button(ui, LiveCommand::FlashKill, ctx, orders, size, !state.flashes);
        action_button(ui, LiveCommand::Breathing, ctx, orders, size, state.breathing);
    });
    ui.horizontal_wrapped(|ui| {
        let flashes = if state.flashes {
            "Flashs : limités (limiteur photosensible actif)".to_owned()
        } else {
            "Flashs : COUPÉS".to_owned()
        };
        ui.label(flashes);
        if state.blackout_on {
            ui.label(RichText::new("NOIR demandé").strong().color(WARN_TEXT));
        }
        if state.safe_look {
            ui.label(RichText::new("LOOK CALME en place").strong().color(WARN_TEXT));
        }
        if state.breathing {
            ui.label("Respiration : effets réduits");
        }
    });
}
