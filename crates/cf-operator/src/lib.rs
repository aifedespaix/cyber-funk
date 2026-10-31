//! La fenêtre opérateur (egui) : préparation de la soirée et pilotage en live. Jamais affichée sur le projecteur.
//!
//! La fenêtre principale de l'application est le **projecteur** (réglée par le binaire). L'opérateur est une
//! seconde fenêtre, avec sa propre caméra 2D qui porte le contexte egui : egui ne dessine donc jamais dans le
//! projecteur.

mod audio_panel;
mod controls_panel;
mod diagnostics;
mod freecam;
mod live;
mod loading;
mod prepare;
mod preview;
mod quit;
mod ui;
mod window;

pub use live::OperatorMode;
pub use prepare::PrepConfig;
pub use quit::{QuitConfirmation, QuitDecision, QuitGuardPlugin, QuitPrompt, QuitSource};
pub use window::OperatorWindow;

use bevy::prelude::*;
use bevy_egui::input::EguiWantsInput;
use bevy_egui::{EguiGlobalSettings, EguiPlugin, EguiPrimaryContextPass};
use cf_render::{CfSet, TextInputFocus};

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
        app.world_mut()
            .resource_mut::<EguiGlobalSettings>()
            .auto_create_primary_context = false;
        // Le panneau audio écrit des commandes ; le plugin audio enregistre le même message (idempotent).
        app.add_message::<cf_render::audio::AudioCommand>();
        // Quand un champ texte d'egui a le focus, le clavier n'est plus une commande (voir `cf_render::controls`).
        app.init_resource::<TextInputFocus>()
            .add_systems(Update, sync_text_focus.before(CfSet::Input));
        // Où chercher les looks et quel événement ouvrir en Préparation (le binaire peut l'avoir posé avant).
        app.init_resource::<PrepConfig>()
            .add_systems(Startup, prepare::init_session);
        live::build(app);
        preview::build(app);
        freecam::build(app);
        app.add_systems(Startup, window::spawn_operator_window).add_systems(
            EguiPrimaryContextPass,
            (
                preview::sync_egui,
                live::live_input,
                ui::main_panel,
                diagnostics::diagnostics_window,
                live::mode_dialog,
                ui::quit_dialog,
                live::flush_orders,
            )
                .chain(),
        );
    }
}

/// Recopie « egui veut le clavier » (champ texte en cours d'édition, menu ouvert) dans `TextInputFocus`, que le pont
/// clavier lit en `CfSet::Input`. La valeur est celle de la passe egui de l'image précédente : un champ qui prend le
/// focus par un clic l'a donc avant la première touche tapée.
fn sync_text_focus(wants: Res<EguiWantsInput>, mut focus: ResMut<TextInputFocus>) {
    focus.set_if_neq(TextInputFocus(wants.wants_any_keyboard_input()));
}
