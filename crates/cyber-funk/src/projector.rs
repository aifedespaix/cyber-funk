//! La fenêtre projecteur : plein écran sans bordure, curseur caché, aucune interface.
//!
//! La fenêtre principale de Bevy *est* le projecteur. Elle naît sur l'écran principal, puis `place_projector` la
//! déplace sur l'écran demandé dès que Bevy connaît les écrans : au moment de créer la fenêtre initiale,
//! `bevy_winit` n'a pas encore énuméré les moniteurs et `MonitorSelection::Index` retomberait en silence sur
//! l'écran courant. Le déplacement a lieu à la première image.

use bevy::prelude::*;
use bevy::window::{
    CursorOptions, ExitCondition, Monitor, MonitorSelection, PresentMode, PrimaryWindow, WindowMode, WindowPlugin,
    WindowResolution,
};

use crate::cli::MonitorChoice;

/// Titre de la fenêtre projecteur.
const TITLE: &str = "cyber-funk : projecteur";
/// Taille de la fenêtre en mode `--windowed`, en pixels logiques (16:9, comme la sortie projecteur).
const WINDOWED_SIZE: (u32, u32) = (1280, 720);

/// Réglages du projecteur issus de la ligne de commande.
#[derive(Resource, Debug, Clone)]
pub struct ProjectorConfig {
    /// Écran demandé.
    pub monitor: MonitorChoice,
    /// Fenêtre ordinaire (développement) au lieu du plein écran.
    pub windowed: bool,
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

/// Place le projecteur sur l'écran demandé (voir la doc du module).
#[derive(Debug)]
pub struct ProjectorPlugin(pub ProjectorConfig);

impl Plugin for ProjectorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.0.clone())
            .add_systems(Update, place_projector.run_if(not_placed));
    }
}

/// Marque la fin du placement : le système ne tourne plus ensuite.
#[derive(Resource, Debug)]
struct ProjectorPlaced;

fn not_placed(placed: Option<Res<ProjectorPlaced>>) -> bool {
    placed.is_none()
}

/// Ce que le système sait d'un écran, de quoi choisir sans dépendre de Bevy dans la logique de choix.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MonitorInfo {
    name: Option<String>,
}

/// Écran retenu pour le projecteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// Écran principal du système.
    Primary,
    /// n-ième écran de l'énumération de Bevy.
    Index(usize),
    /// Écran à la position donnée dans la liste fournie à [`resolve`].
    Slot(usize),
}

/// Pourquoi un écran demandé n'a pas pu être retenu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ResolveError {
    /// L'index dépasse le nombre d'écrans.
    OutOfRange { index: usize, count: usize },
    /// Aucun écran ne porte ce nom.
    NoMatch,
    /// Plusieurs écrans contiennent ce fragment de nom.
    Ambiguous,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange { index, count } => write!(f, "index {index} hors limites, {count} écran(s) détecté(s)"),
            Self::NoMatch => f.write_str("aucun écran de ce nom"),
            Self::Ambiguous => f.write_str("plusieurs écrans correspondent, préciser le nom"),
        }
    }
}

/// Choisit l'écran du projecteur parmi `monitors`.
///
/// Un nom s'apparie d'abord exactement (sans la casse), puis par fragment s'il est sans ambiguïté.
fn resolve(choice: &MonitorChoice, monitors: &[MonitorInfo]) -> Result<Target, ResolveError> {
    match choice {
        MonitorChoice::Primary => Ok(Target::Primary),
        MonitorChoice::Index(index) if *index < monitors.len() => Ok(Target::Index(*index)),
        MonitorChoice::Index(index) => Err(ResolveError::OutOfRange {
            index: *index,
            count: monitors.len(),
        }),
        MonitorChoice::Name(wanted) => {
            let wanted = wanted.to_lowercase();
            let names: Vec<Option<String>> = monitors
                .iter()
                .map(|monitor| monitor.name.as_ref().map(|name| name.to_lowercase()))
                .collect();
            if let Some(slot) = names.iter().position(|name| name.as_deref() == Some(wanted.as_str())) {
                return Ok(Target::Slot(slot));
            }
            let mut partial = names
                .iter()
                .enumerate()
                .filter(|(_, name)| name.as_ref().is_some_and(|name| name.contains(&wanted)))
                .map(|(slot, _)| slot);
            match (partial.next(), partial.next()) {
                (Some(slot), None) => Ok(Target::Slot(slot)),
                (Some(_), Some(_)) => Err(ResolveError::Ambiguous),
                (None, _) => Err(ResolveError::NoMatch),
            }
        }
    }
}

fn place_projector(
    mut commands: Commands,
    config: Res<ProjectorConfig>,
    monitors: Query<(Entity, &Monitor)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    // Les moniteurs sont créés par bevy_winit juste avant la première image ; en attendant on ne fait rien.
    if monitors.is_empty() {
        return;
    }
    commands.insert_resource(ProjectorPlaced);

    // L'ordre des entités suit l'énumération du système au démarrage : on s'en sert pour numéroter la liste.
    let mut listed: Vec<(Entity, &Monitor)> = monitors.iter().collect();
    listed.sort_by_key(|(entity, _)| entity.index());
    for (index, (_, monitor)) in listed.iter().enumerate() {
        tracing::info!(
            index,
            name = monitor.name.as_deref().unwrap_or("?"),
            width_px = monitor.physical_width,
            height_px = monitor.physical_height,
            refresh_mhz = monitor.refresh_rate_millihertz,
            "écran détecté"
        );
    }

    let infos: Vec<MonitorInfo> = listed
        .iter()
        .map(|(_, monitor)| MonitorInfo {
            name: monitor.name.clone(),
        })
        .collect();
    let selection = match resolve(&config.monitor, &infos) {
        Ok(Target::Primary) => MonitorSelection::Primary,
        Ok(Target::Index(index)) => MonitorSelection::Index(index),
        Ok(Target::Slot(slot)) => MonitorSelection::Entity(listed[slot].0),
        Err(error) => {
            tracing::warn!(requested = ?config.monitor, "écran du projecteur introuvable ({error}) : écran principal");
            MonitorSelection::Primary
        }
    };

    if config.windowed {
        // Fenêtre de développement : on la centre sur l'écran choisi, sans la passer en plein écran.
        if selection != MonitorSelection::Primary {
            window.position.center(selection);
        }
    } else {
        window.mode = WindowMode::BorderlessFullscreen(selection);
    }
    tracing::info!(?selection, windowed = config.windowed, "projecteur placé");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitors(names: &[Option<&str>]) -> Vec<MonitorInfo> {
        names
            .iter()
            .map(|name| MonitorInfo {
                name: name.map(str::to_owned),
            })
            .collect()
    }

    #[test]
    fn l_ecran_principal_est_le_defaut() {
        assert_eq!(resolve(&MonitorChoice::Primary, &monitors(&[])), Ok(Target::Primary));
    }

    #[test]
    fn un_index_valide_est_conserve_et_un_index_hors_limites_est_refuse() {
        let two = monitors(&[Some("DP-1"), Some("HDMI-1")]);
        assert_eq!(resolve(&MonitorChoice::Index(1), &two), Ok(Target::Index(1)));
        assert_eq!(
            resolve(&MonitorChoice::Index(2), &two),
            Err(ResolveError::OutOfRange { index: 2, count: 2 })
        );
    }

    #[test]
    fn un_nom_s_apparie_sans_la_casse_puis_par_fragment() {
        let two = monitors(&[Some("DELL U2720Q"), Some("Projector XJ")]);
        assert_eq!(resolve(&MonitorChoice::Name("projector xj".into()), &two), Ok(Target::Slot(1)));
        assert_eq!(resolve(&MonitorChoice::Name("dell".into()), &two), Ok(Target::Slot(0)));
        assert_eq!(resolve(&MonitorChoice::Name("benq".into()), &two), Err(ResolveError::NoMatch));
    }

    #[test]
    fn un_fragment_ambigu_est_refuse_mais_le_nom_exact_gagne() {
        let two = monitors(&[Some("HDMI-1"), Some("HDMI-10"), None]);
        assert_eq!(resolve(&MonitorChoice::Name("hdmi".into()), &two), Err(ResolveError::Ambiguous));
        assert_eq!(resolve(&MonitorChoice::Name("hdmi-1".into()), &two), Ok(Target::Slot(0)));
    }

    #[test]
    fn le_projecteur_plein_ecran_n_a_ni_bordure_ni_curseur() {
        let plugin = window_plugin(&ProjectorConfig {
            monitor: MonitorChoice::Primary,
            windowed: false,
        });
        let window = plugin.primary_window.unwrap();
        assert!(!window.decorations);
        assert!(matches!(window.mode, WindowMode::BorderlessFullscreen(_)));
        assert!(!plugin.primary_cursor_options.unwrap().visible);
        assert!(!plugin.close_when_requested, "la fermeture passe par le garde-fou de sortie");
    }

    #[test]
    fn l_option_windowed_donne_une_fenetre_ordinaire_avec_curseur() {
        let plugin = window_plugin(&ProjectorConfig {
            monitor: MonitorChoice::Primary,
            windowed: true,
        });
        assert_eq!(plugin.primary_window.unwrap().mode, WindowMode::Windowed);
        assert!(plugin.primary_cursor_options.unwrap().visible);
    }
}
