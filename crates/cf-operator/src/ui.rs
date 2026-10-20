//! Interface egui de la fenêtre opérateur.

use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};

use cf_render::audio::{AudioCommand, AudioFeatures, AudioStatus, VisualLatency};

use crate::audio_panel;
use crate::quit::{QuitDecision, QuitPrompt, QuitSource};

/// Panneau central : l'audio pour l'instant (J1-06) ; les autres panneaux arrivent avec les tâches suivantes.
///
/// Sans `CfAudioPlugin`, les ressources audio manquent : le panneau le dit au lieu de s'afficher à vide.
pub(crate) fn main_panel(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    status: Option<Res<AudioStatus>>,
    features: Option<Res<AudioFeatures>>,
    latency: Option<ResMut<VisualLatency>>,
    mut commands: MessageWriter<AudioCommand>,
) {
    // Depuis egui 0.36 les panneaux se posent dans un `Ui` racine, pas directement sur le contexte.
    let ctx = context.get_mut();
    let mut root = egui::Ui::new(
        ctx.clone(),
        "operator_root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    egui::CentralPanel::default().show(&mut root, |ui| {
        ui.heading("cyber-funk");
        egui::ScrollArea::vertical().show(ui, |ui| match (status, features, latency) {
            (Some(status), Some(features), Some(mut latency)) => {
                let mut orders = Vec::new();
                audio_panel::draw(ui, &status, &features, &mut latency, &mut orders);
                commands.write_batch(orders);
            }
            _ => {
                ui.weak("Audio non branché (CfAudioPlugin absent).");
            }
        });
    });
}

/// Dialogue de confirmation de sortie, affiché tant qu'une fermeture est en attente.
///
/// `Échap` ferme le dialogue (annule) : c'est une sortie du dialogue, jamais de l'application.
pub(crate) fn quit_dialog(
    mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>,
    prompt: Res<QuitPrompt>,
    mut decisions: MessageWriter<QuitDecision>,
) {
    let Some(source) = prompt.pending else {
        return;
    };
    let ctx = context.get_mut();
    let consequence = match source {
        QuitSource::Operator => "Fermer l'opérateur arrête aussi le projecteur.",
        QuitSource::Projector => "Fermer le projecteur arrête aussi l'opérateur.",
    };
    egui::Window::new("Quitter cyber-funk ?")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .show(ctx, |ui| {
            ui.label(consequence);
            ui.horizontal(|ui| {
                if ui.button("Quitter").clicked() {
                    decisions.write(QuitDecision::Confirm);
                }
                if ui.button("Annuler").clicked() {
                    decisions.write(QuitDecision::Cancel);
                }
            });
        });
    if ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        decisions.write(QuitDecision::Cancel);
    }
}
