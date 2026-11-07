//! Le projecteur et les écrans en cours de show : placement au démarrage, liste des écrans pour l'opérateur, ordres
//! de l'opérateur (`ScreenOrder`).
//!
//! ```text
//! première image où Bevy connaît les écrans : place_projector  ─▶ Window (mode/position), ProjectorState
//! écran branché/débranché, projecteur déplacé : refresh_screen_list ─▶ ScreenList (seulement si elle change)
//! ScreenOrder (onglet Réglages)              : apply_screen_orders  ─▶ Window, ProjectorState
//! ```
//!
//! Seule la fenêtre principale (le projecteur) est touchée, jamais la fenêtre opérateur. La fenêtre est déplacée par
//! son mode (`BorderlessFullscreen(écran)`) ou sa position, jamais recréée : aucun asset ni shader n'est rechargé
//! (règle 8). Aucun réglage n'est écrit ici : un écran mémorisé absent reste mémorisé.

use bevy::prelude::*;
use bevy::window::{CursorOptions, Monitor, MonitorSelection, PrimaryMonitor, PrimaryWindow, WindowMode};
use cf_render::{ScreenList, ScreenOrder, describe_screens};

use super::resolve::{MonitorInfo, Origin, Target, resolve, startup_target};
use super::{ProjectorConfig, WINDOWED_SIZE};
use crate::cli::MonitorChoice;

/// Où est le projecteur. Posée par [`place_projector`] : son absence veut dire « pas encore placé ».
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ProjectorState {
    /// Écran qui porte le projecteur, `None` : inconnu (écran principal introuvable comme entité).
    pub on: Option<Entity>,
    /// Plein écran sans bordure, ou fenêtre ordinaire.
    pub fullscreen: bool,
}

/// Les écrans dans l'ordre de l'énumération du système au démarrage (celui des numéros de `--projector-monitor`).
fn listed<'a>(monitors: impl Iterator<Item = (Entity, &'a Monitor, bool)>) -> Vec<(Entity, &'a Monitor, bool)> {
    let mut listed: Vec<_> = monitors.collect();
    listed.sort_by_key(|(entity, _, _)| entity.index());
    listed
}

fn infos(listed: &[(Entity, &Monitor, bool)]) -> Vec<MonitorInfo> {
    listed
        .iter()
        .map(|(_, monitor, _)| MonitorInfo {
            name: monitor.name.clone(),
        })
        .collect()
}

/// Place le projecteur sur l'écran retenu au démarrage (voir la doc du module parent).
pub(super) fn place_projector(
    mut commands: Commands,
    config: Res<ProjectorConfig>,
    monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
) {
    // Les moniteurs sont créés par bevy_winit juste avant la première image ; en attendant on ne fait rien.
    if monitors.is_empty() {
        return;
    }
    let listed = listed(monitors.iter());
    for (index, (_, monitor, primary)) in listed.iter().enumerate() {
        tracing::info!(
            index,
            name = monitor.name.as_deref().unwrap_or("?"),
            width_px = monitor.physical_width,
            height_px = monitor.physical_height,
            refresh_mhz = monitor.refresh_rate_millihertz,
            primary,
            "écran détecté"
        );
    }

    let (target, fallback) = startup_target(
        config.cli_monitor.as_ref(),
        config.saved_monitor.as_deref(),
        &infos(&listed),
    );
    // Une seule fois, au placement : un repli n'inonde pas le journal.
    match fallback {
        Some((Origin::Saved, MonitorChoice::Name(name), error)) => tracing::warn!(
            saved = %name,
            "écran mémorisé absent ({error}) : écran principal ; le réglage est conservé"
        ),
        Some((_, requested, error)) => {
            tracing::warn!(
                ?requested,
                "écran du projecteur introuvable ({error}) : écran principal"
            );
        }
        None => {}
    }
    let primary = listed
        .iter()
        .find(|(_, _, primary)| *primary)
        .map(|(entity, _, _)| *entity);
    let (selection, on) = match target {
        Target::Primary => (MonitorSelection::Primary, primary),
        Target::Index(index) => (MonitorSelection::Index(index), listed.get(index).map(|(e, _, _)| *e)),
        Target::Slot(slot) => match listed.get(slot) {
            Some((entity, _, _)) => (MonitorSelection::Entity(*entity), Some(*entity)),
            None => (MonitorSelection::Primary, primary),
        },
    };

    if config.windowed {
        // Fenêtre ordinaire : centrée sur l'écran choisi, sans passer en plein écran.
        if selection != MonitorSelection::Primary {
            window.position.center(selection);
        }
    } else {
        window.mode = WindowMode::BorderlessFullscreen(selection);
    }
    commands.insert_resource(ProjectorState {
        on,
        fullscreen: !config.windowed,
    });
    tracing::info!(?selection, windowed = config.windowed, "projecteur placé");
}

/// Recopie les écrans dans [`ScreenList`] quand un écran apparaît, disparaît ou change, ou que le projecteur change
/// d'écran. Rien n'est alloué les autres images.
pub(super) fn refresh_screen_list(
    state: Option<Res<ProjectorState>>,
    monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>,
    changed: Query<(), Changed<Monitor>>,
    mut removed: RemovedComponents<Monitor>,
    mut list: ResMut<ScreenList>,
) {
    let removed_any = removed.read().count() > 0;
    let moved = state.as_ref().is_some_and(Res::is_changed);
    if changed.is_empty() && !removed_any && !moved {
        return;
    }
    let listed = listed(monitors.iter());
    let on = state.and_then(|state| state.on);
    let projector = on.and_then(|on| listed.iter().position(|(entity, _, _)| *entity == on));
    let pairs: Vec<(&Monitor, bool)> = listed
        .iter()
        .map(|(_, monitor, primary)| (*monitor, *primary))
        .collect();
    list.set_if_neq(ScreenList(describe_screens(&pairs, projector)));
}

/// Applique les ordres de l'opérateur au projecteur. Un ordre sans effet (écran déjà occupé, mode déjà en place) ne
/// touche pas à la fenêtre.
pub(super) fn apply_screen_orders(
    mut orders: MessageReader<ScreenOrder>,
    mut state: ResMut<ProjectorState>,
    monitors: Query<(Entity, &Monitor, Has<PrimaryMonitor>)>,
    mut window: Single<(&mut Window, &mut CursorOptions), With<PrimaryWindow>>,
) {
    for order in orders.read() {
        let (window, cursor) = &mut *window;
        match order {
            ScreenOrder::MoveProjector(name) => {
                let listed = listed(monitors.iter());
                let entity = match resolve(&MonitorChoice::Name(name.clone()), &infos(&listed)) {
                    Ok(Target::Slot(slot)) => listed.get(slot).map(|(entity, _, _)| *entity),
                    Ok(_) => None,
                    Err(error) => {
                        tracing::warn!(screen = %name, "projecteur non déplacé : {error}");
                        None
                    }
                };
                let Some(entity) = entity.filter(|entity| state.on != Some(*entity)) else {
                    continue;
                };
                let selection = MonitorSelection::Entity(entity);
                if state.fullscreen {
                    window.mode = WindowMode::BorderlessFullscreen(selection);
                } else {
                    window.position.center(selection);
                }
                state.on = Some(entity);
                tracing::info!(screen = %name, fullscreen = state.fullscreen, "projecteur déplacé");
            }
            ScreenOrder::SetFullscreen(fullscreen) => {
                if *fullscreen == state.fullscreen {
                    continue;
                }
                let selection = state.on.map_or(MonitorSelection::Current, MonitorSelection::Entity);
                if *fullscreen {
                    window.mode = WindowMode::BorderlessFullscreen(selection);
                    window.decorations = false;
                    cursor.visible = false;
                } else {
                    // Fenêtre ordinaire : bordure et curseur pour pouvoir la déplacer à la main.
                    window.mode = WindowMode::Windowed;
                    #[allow(clippy::cast_precision_loss)]
                    // Constantes de quelques milliers de pixels : exactes en f32.
                    window.resolution.set(WINDOWED_SIZE.0 as f32, WINDOWED_SIZE.1 as f32);
                    window.position.center(selection);
                    window.decorations = true;
                    cursor.visible = true;
                }
                state.fullscreen = *fullscreen;
                tracing::info!(fullscreen, "projecteur : mode d'affichage changé");
            }
            ScreenOrder::Identify => {
                // Afficher un numéro sur chaque écran demanderait une fenêtre par écran : pas encore disponible.
                tracing::info!("identification des écrans pas encore disponible");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::window::WindowPosition;
    use cf_render::Settings;
    use cf_show::persist::AppSettings;

    use super::super::ProjectorPlugin;
    use super::*;

    fn monitor(name: &str) -> Monitor {
        Monitor {
            name: Some(name.to_owned()),
            physical_height: 1080,
            physical_width: 1920,
            physical_position: IVec2::ZERO,
            refresh_rate_millihertz: Some(60_000),
            scale_factor: 1.0,
            video_modes: Vec::new(),
        }
    }

    /// Une application sans fenêtre réelle : un projecteur (fenêtre principale), une fenêtre opérateur, deux écrans
    /// (le premier principal) et des réglages qui mémorisent `saved`.
    fn app(saved: Option<&str>, windowed: bool) -> (App, [Entity; 2], Entity) {
        let mut app = App::new();
        let mut value = AppSettings::default();
        value.operator.output_monitor = saved.map(str::to_owned);
        app.add_plugins(MinimalPlugins)
            .insert_resource(Settings::new(value, "settings.ron".into()))
            .add_plugins(ProjectorPlugin(ProjectorConfig {
                cli_monitor: None,
                saved_monitor: saved.map(str::to_owned),
                windowed,
            }));
        app.world_mut().spawn((Window::default(), PrimaryWindow));
        let operator = app.world_mut().spawn(Window::default()).id();
        let dp = app.world_mut().spawn((monitor("DP-1"), PrimaryMonitor)).id();
        let hdmi = app.world_mut().spawn(monitor("HDMI-1")).id();
        app.update();
        app.update();
        (app, [dp, hdmi], operator)
    }

    fn projector(app: &mut App) -> Window {
        let mut query = app.world_mut().query_filtered::<&Window, With<PrimaryWindow>>();
        query.single(app.world()).cloned().expect("un projecteur")
    }

    fn on_screen(app: &App) -> Vec<bool> {
        app.world()
            .resource::<ScreenList>()
            .0
            .iter()
            .map(|s| s.projector)
            .collect()
    }

    #[test]
    fn saved_missing_monitor_falls_back_to_primary_and_keeps_setting_in_app() {
        let (mut app, _, _) = app(Some("BenQ W1070"), false);
        assert_eq!(
            projector(&mut app).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        );
        assert_eq!(on_screen(&app), [true, false], "l'écran principal porte le projecteur");
        assert_eq!(
            app.world()
                .resource::<Settings>()
                .value
                .operator
                .output_monitor
                .as_deref(),
            Some("BenQ W1070"),
            "un écran débranché aujourd'hui n'efface pas le réglage"
        );
    }

    #[test]
    fn a_saved_monitor_is_used_at_startup() {
        let (mut app, [_, hdmi], _) = app(Some("hdmi-1"), false);
        assert_eq!(
            projector(&mut app).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Entity(hdmi))
        );
        assert_eq!(on_screen(&app), [false, true]);
    }

    #[test]
    fn a_move_order_moves_the_projector_only_and_keeps_fullscreen() {
        let (mut app, [_, hdmi], operator) = app(None, false);
        let before = app.world().get::<Window>(operator).cloned().expect("opérateur");
        app.world_mut()
            .write_message(ScreenOrder::MoveProjector("HDMI-1".into()));
        app.update();
        app.update();
        assert_eq!(
            projector(&mut app).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Entity(hdmi))
        );
        assert_eq!(on_screen(&app), [false, true]);
        let after = app.world().get::<Window>(operator).cloned().expect("opérateur");
        assert_eq!(
            (after.mode, after.position),
            (before.mode, before.position),
            "l'opérateur ne bouge pas"
        );
    }

    #[test]
    fn leaving_fullscreen_then_moving_centres_a_window() {
        let (mut app, [_, hdmi], _) = app(None, false);
        app.world_mut().write_message(ScreenOrder::SetFullscreen(false));
        app.update();
        app.world_mut()
            .write_message(ScreenOrder::MoveProjector("HDMI-1".into()));
        app.update();
        let window = projector(&mut app);
        assert_eq!(window.mode, WindowMode::Windowed);
        assert_eq!(
            window.position,
            WindowPosition::Centered(MonitorSelection::Entity(hdmi))
        );
        assert!(window.decorations);
        app.world_mut().write_message(ScreenOrder::SetFullscreen(true));
        app.update();
        assert_eq!(
            projector(&mut app).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Entity(hdmi)),
            "le plein écran revient sur l'écran choisi"
        );
    }

    #[test]
    fn an_unknown_screen_leaves_the_projector_in_place() {
        let (mut app, _, _) = app(None, false);
        app.world_mut().write_message(ScreenOrder::MoveProjector("BenQ".into()));
        app.world_mut().write_message(ScreenOrder::Identify);
        app.update();
        assert_eq!(
            projector(&mut app).mode,
            WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
        );
        assert_eq!(on_screen(&app), [true, false]);
    }
}
