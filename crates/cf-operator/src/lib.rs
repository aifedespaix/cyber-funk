//! La fenêtre opérateur (egui) : préparation de la soirée et pilotage en live. Jamais affichée sur le projecteur.
//!
//! La fenêtre principale de l'application est le **projecteur** (réglée par le binaire). L'opérateur est une
//! seconde fenêtre, avec sa propre caméra 2D qui porte le contexte egui : egui ne dessine donc jamais dans le
//! projecteur.

mod quit;
mod ui;
mod window;

pub use quit::{QuitConfirmation, QuitDecision, QuitGuardPlugin, QuitPrompt, QuitSource};
pub use window::OperatorWindow;

use bevy::prelude::*;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};

/// Plugin de la fenêtre opérateur : fenêtre, contexte egui, panneau principal et dialogue de sortie.
///
/// À ne pas ajouter avec `--no-operator` ; le binaire ajoute alors seulement `QuitGuardPlugin::double_close()`.
#[derive(Debug, Default)]
pub struct OperatorPlugin;

impl Plugin for OperatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((EguiPlugin::default(), QuitGuardPlugin::dialog()));
        // Le contexte egui est posé à la main sur la caméra opérateur : par défaut bevy_egui le poserait sur la
        // première caméra créée, c'est-à-dire celle du projecteur.
        app.world_mut().resource_mut::<EguiGlobalSettings>().auto_create_primary_context = false;
        app.add_systems(Startup, window::spawn_operator_window)
            .add_systems(EguiPrimaryContextPass, (ui::main_panel, ui::quit_dialog).chain());
    }
}
