//! L'écran de sortie, côté opérateur : quel geste devient quel ordre, et ce qui est retenu pour le prochain lancement.
//!
//! ```text
//! clic sur un écran (carte « Écran de sortie ») ─ pick ─▶ ScreenOrder ─▶ binaire (projector) : déplace la fenêtre
//!                                                                    └─▶ remember_screen_orders ─▶ OperatorPrefs
//! ```
//!
//! L'opérateur ne connaît pas le projecteur : il lit [`ScreenList`] (écrite par le binaire) et écrit des
//! [`ScreenOrder`]. Les réglages ne sont écrits que sur un vrai changement (pas d'écriture disque pour rien).

use bevy::prelude::*;
use cf_render::{ScreenList, ScreenOrder, Settings};
use cf_show::persist::OperatorPrefs;

use crate::shell::confirm::Confirm;

/// État de la carte (jamais sauvegardé) : la confirmation d'un changement d'écran en Live.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub(crate) struct ScreenCardState {
    pub confirm: Confirm,
}

/// Clé de confirmation d'un écran (0 est réservé à « sans contexte »).
fn confirm_key(slot: usize) -> u32 {
    u32::try_from(slot).map_or(u32::MAX, |slot| slot.saturating_add(1))
}

/// Un clic sur l'écran `slot` : l'ordre à envoyer, s'il y en a un.
///
/// Rien pour un écran absent, sans nom (le binaire ne le retrouverait pas) ou qui porte déjà le projecteur. En Live,
/// un premier clic arme seulement la confirmation (un clic de travers noircirait la projection devant le public) ;
/// le second, après le délai anti double-clic, envoie l'ordre.
pub(crate) fn pick(
    list: &ScreenList,
    slot: usize,
    live: bool,
    now_s: f64,
    state: &mut ScreenCardState,
) -> Option<ScreenOrder> {
    let name = list
        .screens
        .get(slot)
        .filter(|screen| !screen.projector)?
        .name
        .clone()?;
    if live && !state.confirm.click(now_s, confirm_key(slot)) {
        return None;
    }
    state.confirm.disarm();
    Some(ScreenOrder::MoveProjector(name))
}

/// L'écran dont la confirmation est armée, s'il y en a une (après expiration éventuelle à `now_s`).
pub(crate) fn armed_slot(list: &ScreenList, now_s: f64, state: &mut ScreenCardState) -> Option<usize> {
    let slot = (0..list.screens.len()).find(|slot| {
        let mut probe = state.confirm;
        probe.refresh(now_s, confirm_key(*slot));
        probe.is_armed()
    });
    if slot.is_none() {
        state.confirm.disarm();
    }
    slot
}

/// Recopie un ordre dans les préférences ; `true` si elles ont changé. `fullscreen_controllable` : le projecteur
/// accepte un changement de mode (`ScreenList::fullscreen` connu) ; sinon (`--windowed`) le plein écran retenu ne change
/// pas, l'opérateur n'ayant rien vu changer.
pub(crate) fn remember(prefs: &mut OperatorPrefs, order: &ScreenOrder, fullscreen_controllable: bool) -> bool {
    match order {
        ScreenOrder::MoveProjector(name) if prefs.output_monitor.as_deref() != Some(name.as_str()) => {
            prefs.output_monitor = Some(name.clone());
            true
        }
        ScreenOrder::SetFullscreen(fullscreen) if fullscreen_controllable && prefs.fullscreen != *fullscreen => {
            prefs.fullscreen = *fullscreen;
            true
        }
        _ => false,
    }
}

/// Retient les choix d'écran de l'opérateur (seulement sur un vrai changement : `Settings` n'est touchée qu'alors).
pub(crate) fn remember_screen_orders(
    mut orders: MessageReader<ScreenOrder>,
    list: Option<Res<ScreenList>>,
    settings: Option<ResMut<Settings>>,
) {
    let Some(mut settings) = settings else {
        orders.clear();
        return;
    };
    let controllable = list.is_some_and(|list| list.fullscreen.is_some());
    for order in orders.read() {
        let mut prefs = settings.value.operator.clone();
        if remember(&mut prefs, order, controllable) {
            settings.value.operator = prefs;
        }
    }
}

#[cfg(test)]
mod tests {
    use cf_render::ScreenInfo;
    use cf_show::persist::AppSettings;

    use super::*;
    use crate::shell::confirm::CONFIRM_DELAY_S;

    fn screen(name: Option<&str>, projector: bool) -> ScreenInfo {
        ScreenInfo {
            name: name.map(str::to_owned),
            width: 1920,
            height: 1080,
            refresh_mhz: Some(60_000),
            primary: projector,
            projector,
        }
    }

    fn three() -> ScreenList {
        ScreenList {
            screens: vec![
                screen(Some("DP-1"), true),
                screen(Some("HDMI-1"), false),
                screen(None, false),
            ],
            fullscreen: Some(true),
        }
    }

    #[test]
    fn picking_another_screen_orders_a_move_and_the_current_one_nothing() {
        let list = three();
        let mut state = ScreenCardState::default();
        assert_eq!(
            pick(&list, 1, false, 0.0, &mut state),
            Some(ScreenOrder::MoveProjector("HDMI-1".into()))
        );
        assert_eq!(pick(&list, 0, false, 0.0, &mut state), None, "déjà le projecteur");
        assert_eq!(pick(&list, 2, false, 0.0, &mut state), None, "écran sans nom");
        assert_eq!(pick(&list, 9, false, 0.0, &mut state), None, "écran absent");
    }

    #[test]
    fn in_live_a_move_needs_two_clicks() {
        let list = three();
        let mut state = ScreenCardState::default();
        assert_eq!(pick(&list, 1, true, 10.0, &mut state), None, "le premier clic arme");
        assert_eq!(armed_slot(&list, 10.1, &mut state), Some(1));
        assert_eq!(
            pick(&list, 1, true, 10.1, &mut state),
            None,
            "un double-clic ne confirme pas"
        );
        assert_eq!(
            pick(&list, 1, true, 10.0 + CONFIRM_DELAY_S + 0.1, &mut state),
            Some(ScreenOrder::MoveProjector("HDMI-1".into()))
        );
        assert_eq!(armed_slot(&list, 11.0, &mut state), None, "désarmée après confirmation");
    }

    #[test]
    fn selecting_a_screen_writes_prefs_only_on_change() {
        let mut prefs = OperatorPrefs::default();
        let order = ScreenOrder::MoveProjector("HDMI-1".into());
        assert!(remember(&mut prefs, &order, true));
        assert_eq!(prefs.output_monitor.as_deref(), Some("HDMI-1"));
        assert!(!remember(&mut prefs, &order, true), "même écran : rien à écrire");
        assert!(
            !remember(&mut prefs, &ScreenOrder::SetFullscreen(true), true),
            "déjà plein écran"
        );
        assert!(remember(&mut prefs, &ScreenOrder::SetFullscreen(false), true));
        assert!(!remember(&mut prefs, &ScreenOrder::Identify, true));
    }

    #[test]
    fn fullscreen_is_not_remembered_when_the_projector_mode_is_locked() {
        let mut prefs = OperatorPrefs::default();
        assert!(!remember(&mut prefs, &ScreenOrder::SetFullscreen(false), false));
        assert!(prefs.fullscreen, "--windowed : plein écran retenu intact");
    }

    #[test]
    fn remember_screen_orders_touches_settings_only_on_change() {
        let mut app = App::new();
        let mut value = AppSettings::default();
        value.operator.output_monitor = Some("HDMI-1".into());
        app.add_plugins(MinimalPlugins)
            .add_message::<ScreenOrder>()
            .insert_resource(Settings::new(value, "settings.ron".into()))
            .add_systems(Update, remember_screen_orders);
        app.update();
        let changed = |app: &mut App| {
            let tick = app.world().resource_ref::<Settings>().last_changed();
            app.update();
            app.world().resource_ref::<Settings>().last_changed() != tick
        };
        app.world_mut()
            .write_message(ScreenOrder::MoveProjector("HDMI-1".into()));
        assert!(!changed(&mut app), "même écran : réglages intacts");
        app.world_mut().write_message(ScreenOrder::MoveProjector("DP-1".into()));
        assert!(changed(&mut app));
        assert_eq!(
            app.world()
                .resource::<Settings>()
                .value
                .operator
                .output_monitor
                .as_deref(),
            Some("DP-1")
        );
    }
}
