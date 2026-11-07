//! La fenêtre projecteur : plein écran sans bordure, curseur caché, aucune interface.
//!
//! La fenêtre principale de Bevy *est* le projecteur. Elle naît sur l'écran principal, puis `place_projector` la
//! déplace sur l'écran demandé dès que Bevy connaît les écrans : au moment de créer la fenêtre initiale,
//! `bevy_winit` n'a pas encore énuméré les moniteurs et `MonitorSelection::Index` retomberait en silence sur
//! l'écran courant. Le déplacement a lieu à la première image.
//!
//! Écran demandé : `--projector-monitor`, sinon l'écran mémorisé par l'opérateur, sinon l'écran principal (voir
//! [`resolve`]). Ensuite l'onglet Réglages peut déplacer le projecteur en cours de show (voir [`screens`]).

mod resolve;
mod screens;

use bevy::prelude::*;
use bevy::window::{
    CursorOptions, ExitCondition, MonitorSelection, PresentMode, WindowMode, WindowPlugin, WindowResolution,
};
use cf_render::{ScreenList, ScreenOrder};

use crate::cli::MonitorChoice;

/// Titre de la fenêtre projecteur.
const TITLE: &str = "cyber-funk : projecteur";
/// Taille de la fenêtre en mode `--windowed`, en pixels logiques (16:9, comme la sortie projecteur).
pub(crate) const WINDOWED_SIZE: (u32, u32) = (1280, 720);

/// Réglages du projecteur au démarrage : ligne de commande et réglages mémorisés.
#[derive(Resource, Debug, Clone)]
pub struct ProjectorConfig {
    /// Écran demandé par `--projector-monitor` (prioritaire).
    pub cli_monitor: Option<MonitorChoice>,
    /// Écran mémorisé par l'opérateur (`OperatorPrefs::output_monitor`), par son nom.
    pub saved_monitor: Option<String>,
    /// Fenêtre ordinaire au lieu du plein écran (`--windowed`, ou plein écran décoché par l'opérateur).
    pub windowed: bool,
    /// `--windowed` : le mode est imposé, l'interface ne peut pas le changer.
    pub fullscreen_locked: bool,
}

impl ProjectorConfig {
    /// La configuration du démarrage : `--windowed` l'emporte sur le plein écran mémorisé.
    #[must_use]
    pub fn new(cli: &crate::cli::Cli, prefs: Option<&cf_show::persist::OperatorPrefs>) -> Self {
        Self {
            cli_monitor: cli.projector_monitor.clone(),
            saved_monitor: prefs.and_then(|prefs| prefs.output_monitor.clone()),
            windowed: cli.windowed || prefs.is_some_and(|prefs| !prefs.fullscreen),
            fullscreen_locked: cli.windowed,
        }
    }
}

/// Le `WindowPlugin` de l'application : fenêtre principale = projecteur, et sortie confirmée par l'opérateur.
///
/// `close_when_requested` et `ExitCondition::DontExit` laissent la main à `QuitGuardPlugin` (cf-operator) : une
/// fermeture de fenêtre ne coupe jamais le show sans confirmation.
#[must_use]
pub fn window_plugin(config: &ProjectorConfig) -> WindowPlugin {
    let (window, cursor) = if config.windowed {
        (
            Window {
                title: TITLE.into(),
                resolution: WindowResolution::new(WINDOWED_SIZE.0, WINDOWED_SIZE.1),
                present_mode: PresentMode::AutoVsync,
                ..default()
            },
            // En développement on garde le curseur : il sert à déplacer et redimensionner la fenêtre.
            CursorOptions::default(),
        )
    } else {
        (
            Window {
                title: TITLE.into(),
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                decorations: false,
                present_mode: PresentMode::AutoVsync,
                ..default()
            },
            CursorOptions {
                visible: false,
                ..default()
            },
        )
    };
    WindowPlugin {
        primary_window: Some(window),
        primary_cursor_options: Some(cursor),
        exit_condition: ExitCondition::DontExit,
        close_when_requested: false,
    }
}

/// Place le projecteur sur l'écran demandé, tient la liste des écrans et suit les ordres de l'opérateur.
#[derive(Debug)]
pub struct ProjectorPlugin(pub ProjectorConfig);

impl Plugin for ProjectorPlugin {
    fn build(&self, app: &mut App) {
        // L'opérateur enregistre les mêmes (idempotent) : sans lui, les ordres restent simplement sans écrivain.
        app.insert_resource(self.0.clone())
            .init_resource::<ScreenList>()
            .init_resource::<screens::EarlyOrders>()
            .add_message::<ScreenOrder>()
            .add_systems(
                Update,
                (
                    screens::place_projector.run_if(not(resource_exists::<screens::ProjectorState>)),
                    screens::apply_screen_orders,
                    screens::refresh_screen_list,
                )
                    .chain(),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(windowed: bool) -> ProjectorConfig {
        ProjectorConfig {
            cli_monitor: None,
            saved_monitor: None,
            windowed,
            fullscreen_locked: windowed,
        }
    }

    #[test]
    fn le_projecteur_plein_ecran_n_a_ni_bordure_ni_curseur() {
        let plugin = window_plugin(&config(false));
        let window = plugin.primary_window.unwrap();
        assert!(!window.decorations);
        assert!(matches!(window.mode, WindowMode::BorderlessFullscreen(_)));
        assert!(!plugin.primary_cursor_options.unwrap().visible);
        assert!(
            !plugin.close_when_requested,
            "la fermeture passe par le garde-fou de sortie"
        );
    }

    #[test]
    fn l_option_windowed_donne_une_fenetre_ordinaire_avec_curseur() {
        let plugin = window_plugin(&config(true));
        assert_eq!(plugin.primary_window.unwrap().mode, WindowMode::Windowed);
        assert!(plugin.primary_cursor_options.unwrap().visible);
    }

    #[test]
    fn la_ligne_de_commande_et_les_reglages_donnent_la_configuration() {
        use clap::Parser as _;
        let mut prefs = cf_show::persist::OperatorPrefs {
            output_monitor: Some("HDMI-1".into()),
            ..Default::default()
        };
        let plain = crate::cli::Cli::parse_from(["cyber-funk"]);
        let config = ProjectorConfig::new(&plain, Some(&prefs));
        assert_eq!(
            (config.cli_monitor, config.saved_monitor.as_deref(), config.windowed),
            (None, Some("HDMI-1"), false)
        );
        prefs.fullscreen = false;
        assert!(
            ProjectorConfig::new(&plain, Some(&prefs)).windowed,
            "plein écran décoché"
        );
        let cli = crate::cli::Cli::parse_from(["cyber-funk", "--projector-monitor", "1", "--windowed"]);
        let config = ProjectorConfig::new(&cli, None);
        assert_eq!(config.cli_monitor, Some(MonitorChoice::Index(1)));
        assert!(config.windowed && config.fullscreen_locked);
        assert!(
            !ProjectorConfig::new(&plain, Some(&prefs)).fullscreen_locked,
            "plein écran décoché : toujours pilotable"
        );
    }
}
