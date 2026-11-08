//! La fenêtre opérateur (egui) : préparation de la soirée et pilotage en live. Jamais affichée sur le projecteur.
//!
//! La fenêtre principale de l'application est le **projecteur** (réglée par le binaire). L'opérateur est une
//! seconde fenêtre, avec sa propre caméra 2D qui porte le contexte egui : egui ne dessine donc jamais dans le
//! projecteur.

mod cockpit;
mod format;
mod freecam;
mod loading;
mod mode;
mod orders;
mod prepare;
mod preview;
mod quit;
mod run_state;
mod services;
mod shell;
mod tabs;
mod ui;
mod window;

pub use mode::OperatorMode;
pub use prepare::PrepConfig;
pub use quit::{QuitConfirmation, QuitDecision, QuitGuardPlugin, QuitPrompt, QuitSource};
pub use run_state::{ShowRun, ShowRunState};
pub use services::logbuf::{OperatorLog, mark_moment};
pub use shell::{InstallUpdate, UpdateStatus};
pub use tabs::settings::restore::startup_event_path;
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
            .add_systems(Startup, tabs::settings::restore::init_session);
        // Le binaire écrit l'état de la mise à jour (J9-06) ; l'opérateur ne fait que l'afficher.
        app.init_resource::<UpdateStatus>().add_message::<InstallUpdate>();
        // L'onglet Debug écrit des ordres de quartier ; le monde enregistre le même message (idempotent), et sans monde
        // ils restent simplement sans lecteur.
        app.init_resource::<tabs::debug::DebugState>()
            .add_message::<cf_render::DistrictOrder>()
            .add_message::<cf_cinema::ShotOrder>();
        // La carte « Écran de sortie » lit la liste des écrans et écrit des ordres ; le projecteur (binaire) enregistre
        // les mêmes (idempotent). Sans lui la liste reste vide et les ordres sans lecteur.
        app.init_resource::<cf_render::ScreenList>()
            .add_message::<cf_render::ScreenOrder>();
        // Les ordres de l'interface arrivent au spectacle par ces messages (idempotent avec le plugin des contrôles).
        app.add_message::<cf_render::ControlAction>()
            .init_resource::<OperatorMode>()
            .init_resource::<ShowRunState>()
            .init_resource::<orders::LiveOrders>()
            .init_resource::<ui::CockpitState>()
            .init_resource::<cockpit::logos::LogoCache>()
            .init_resource::<cockpit::files::FilePicker>();
        shell::build(app);
        tabs::stats::build(app);
        preview::build(app);
        freecam::build(app);
        app.add_systems(Startup, window::spawn_operator_window).add_systems(
            EguiPrimaryContextPass,
            (
                preview::sync_egui,
                mode::sync_mode,
                shell::shell_input,
                ui::main_panel,
                shell::persist_tab,
                services::screens::remember_screen_orders,
                tabs::settings::restore::remember_evening,
                ui::quit_dialog,
                orders::flush_orders,
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
