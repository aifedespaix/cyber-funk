//! Tests de la coque sans écran : onglets et raccourcis, visibilité selon le mode, restauration de l'onglet, gestes en
//! deux temps, Noir toujours disponible, aide complète.

use bevy::prelude::*;
use bevy_egui::egui::{self, Event, Key, Modifiers};
use cf_render::Settings;
use cf_show::persist::AppSettings;

use super::help;
use super::shortcuts::{self, KeyGuard, SHORTCUTS, ShellKey, poll, tab_for_shortcut};
use super::status_bar::{self, StatusAction, StatusData, clock_text, enabled, remaining_s};
use super::{ShellState, Tab, persist_tab, restore_tab};
use crate::OperatorMode;

const PREP: OperatorMode = OperatorMode::Preparation;
const LIVE: OperatorMode = OperatorMode::Live;

#[test]
fn tab_shortcuts_map_ctrl_1_2_3() {
    let ctrl = Modifiers::CTRL;
    assert_eq!(tab_for_shortcut(Key::Num1, ctrl, PREP), Some(Tab::Settings));
    assert_eq!(tab_for_shortcut(Key::Num2, ctrl, PREP), Some(Tab::Stats));
    assert_eq!(tab_for_shortcut(Key::Num3, ctrl, PREP), Some(Tab::Debug));
    assert_eq!(tab_for_shortcut(Key::Num4, ctrl, PREP), None);
    // Le chiffre seul appartient au clavier par défaut (GotoCue) : jamais un onglet.
    assert_eq!(tab_for_shortcut(Key::Num1, Modifiers::NONE, PREP), None);
    // AltGr (Ctrl+Alt sous Windows) n'est pas Ctrl.
    assert_eq!(
        tab_for_shortcut(Key::Num2, Modifiers::CTRL | Modifiers::ALT, PREP),
        None
    );
}

#[test]
fn debug_tab_hidden_in_live() {
    assert!(Tab::Debug.visible_in(PREP));
    assert!(!Tab::Debug.visible_in(LIVE));
    assert!(Tab::Settings.visible_in(LIVE) && Tab::Stats.visible_in(LIVE));
    // Choisi en Préparation, il n'est pas affiché en Live, et revient en Préparation.
    let mut state = ShellState::default();
    assert!(state.select(Tab::Debug, PREP));
    assert_eq!(state.shown(LIVE), Tab::Settings);
    assert_eq!(state.shown(PREP), Tab::Debug);
    assert!(!state.select(Tab::Debug, LIVE));
    assert_eq!(Tab::Settings.label(LIVE), "Live");
    assert_eq!(Tab::Settings.label(PREP), "Réglages");
}

#[test]
fn debug_shortcut_ignored_in_live() {
    assert_eq!(tab_for_shortcut(Key::Num3, Modifiers::CTRL, LIVE), None);
    assert_eq!(tab_for_shortcut(Key::Num2, Modifiers::CTRL, LIVE), Some(Tab::Stats));
    with_events(vec![key(Key::Num3, Modifiers::CTRL)], |input| {
        let mut out = Vec::new();
        poll(input, LIVE, KeyGuard::default(), &mut out);
        assert_eq!(out, Vec::<ShellKey>::new());
    });
}

#[test]
fn tab_key_roundtrips() {
    for tab in Tab::ALL {
        assert_eq!(Tab::from_key(tab.key()), Some(tab));
    }
    // Les valeurs écrites dans `OperatorPrefs::last_tab` (défaut "settings").
    assert_eq!(Tab::Settings.key(), cf_show::persist::OperatorPrefs::default().last_tab);
    assert_eq!(Tab::Stats.key(), "stats");
    assert_eq!(Tab::Debug.key(), "debug");
}

#[test]
fn restores_last_tab_and_falls_back_to_settings_on_unknown_value() {
    assert_eq!(Tab::restore("stats"), Tab::Stats);
    assert_eq!(Tab::restore("debug"), Tab::Debug);
    assert_eq!(Tab::restore("onglet-de-demain"), Tab::Settings);
    assert_eq!(Tab::restore(""), Tab::Settings);

    // Dans une application : restauré au démarrage, réécrit seulement quand il change.
    let mut value = AppSettings::default();
    value.operator.last_tab = "stats".to_owned();
    let mut app = App::new();
    app.insert_resource(Settings::new(value, "inutilise.ron".into()))
        .init_resource::<ShellState>()
        .add_systems(Startup, restore_tab)
        .add_systems(Update, persist_tab);
    app.update();
    assert_eq!(app.world().resource::<ShellState>().tab, Tab::Stats);
    app.world_mut().resource_mut::<ShellState>().tab = Tab::Debug;
    app.update();
    assert_eq!(app.world().resource::<Settings>().value.operator.last_tab, "debug");
}

/// Ce que voit un lecteur des réglages à chaque image : ont-ils changé ?
#[derive(Resource, Default)]
struct SettingsChanges(Vec<bool>);

fn watch_settings(settings: Res<Settings>, mut seen: ResMut<SettingsChanges>) {
    seen.0.push(settings.is_changed());
}

#[test]
fn persisting_an_unchanged_tab_does_not_touch_the_settings() {
    let mut app = App::new();
    app.insert_resource(Settings::new(AppSettings::default(), "inutilise.ron".into()))
        .init_resource::<ShellState>()
        .init_resource::<SettingsChanges>()
        .add_systems(Startup, restore_tab)
        .add_systems(Update, (persist_tab, watch_settings).chain());
    for _ in 0..3 {
        app.update();
    }
    // Toucher les réglages réveille leur écriture sur disque : seulement sur un vrai changement. La première image
    // voit l'insertion de la ressource.
    assert_eq!(app.world().resource::<SettingsChanges>().0[1..], [false, false]);
    app.world_mut().resource_mut::<ShellState>().tab = Tab::Stats;
    app.update();
    app.update();
    assert_eq!(app.world().resource::<SettingsChanges>().0[3..], [true, false]);
}

#[test]
fn next_dj_needs_two_clicks() {
    let mut state = ShellState::default();
    assert!(!state.click_next_dj(), "le premier clic arme seulement");
    assert!(state.confirm_next_dj);
    assert!(state.click_next_dj(), "le second confirme");
    assert!(!state.confirm_next_dj, "et désarme");
    assert!(!state.click_next_dj(), "un nouveau changement redemande deux clics");
    // Le reset garde le même geste.
    assert!(!state.click_reset());
    assert!(state.click_reset());
}

#[test]
fn blackout_is_always_available() {
    let everything = StatusData {
        audio: Some(("Démo", true)),
        fps: Some(60.0),
        bpm: Some(124.0),
        dj: Some(("Aya", 0)),
        remaining_s: Some(600.0),
        active_events: Some(2),
        manual: Some(true),
        paused: Some(false),
        clock: Some((1000, 60)),
        has_show: true,
        blackout_on: true,
    };
    for data in [StatusData::default(), everything] {
        assert!(enabled(&data).blackout);
    }
    assert!(!enabled(&StatusData::default()).next_dj, "pas de show : rien à passer");
    // Échap déclenche le Noir dans les deux modes…
    for mode in [PREP, LIVE] {
        with_events(vec![key(Key::Escape, Modifiers::NONE)], |input| {
            let mut out = Vec::new();
            poll(input, mode, KeyGuard::default(), &mut out);
            assert_eq!(out, [ShellKey::Blackout]);
        });
    }
    // …sauf quand un dialogue l'utilise pour annuler, ou pendant une saisie de texte.
    for guard in [
        KeyGuard {
            dialog_open: true,
            typing: false,
        },
        KeyGuard {
            dialog_open: false,
            typing: true,
        },
    ] {
        with_events(vec![key(Key::Escape, Modifiers::NONE)], |input| {
            let mut out = Vec::new();
            poll(input, PREP, guard, &mut out);
            assert_eq!(out, Vec::<ShellKey>::new(), "{guard:?}");
        });
    }
}

#[test]
fn help_lists_every_registered_shortcut() {
    let rows = help::rows();
    for key in ShellKey::ALL {
        assert!(
            rows.iter().any(|(k, what)| *k == key.text() && !what.is_empty()),
            "{key:?} absent de l'aide"
        );
        assert!(!shortcuts::describe(key.text()).is_empty(), "{key:?} sans infobulle");
    }
    assert_eq!(rows.len(), SHORTCUTS.len());
    // L'aide se dessine.
    let ctx = egui::Context::default();
    ctx.run_ui(egui::RawInput::default(), help::draw)
        .drop_without_applying_deltas();
}

#[test]
fn tab_shortcut_uses_the_physical_key_on_azerty_and_never_while_typing() {
    // AZERTY : la rangée du haut donne un symbole sans Maj (« & », que egui ne nomme pas ; un autre symbole le
    // remplace ici), la touche physique reste « 1 ».
    let azerty = Event::Key {
        key: Key::Exclamationmark,
        physical_key: Some(Key::Num1),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL,
    };
    with_events(vec![azerty.clone(), Event::Text("?".to_owned())], |input| {
        let mut out = Vec::new();
        poll(input, PREP, KeyGuard::default(), &mut out);
        assert_eq!(out, [ShellKey::Tab(Tab::Settings), ShellKey::Help]);
    });
    with_events(vec![azerty, Event::Text("?".to_owned())], |input| {
        let mut out = Vec::new();
        let typing = KeyGuard {
            typing: true,
            dialog_open: false,
        };
        poll(input, PREP, typing, &mut out);
        assert_eq!(out, Vec::<ShellKey>::new());
    });
}

#[test]
fn the_status_bar_draws_with_and_without_values_and_says_when_missing() {
    let ctx = egui::Context::default();
    for confirm in [false, true] {
        let mut state = ShellState {
            confirm_next_dj: confirm,
            confirm_reset: confirm,
            ..ShellState::default()
        };
        let data = StatusData {
            has_show: true,
            paused: Some(true),
            ..StatusData::default()
        };
        let mut actions: Vec<StatusAction> = vec![StatusAction::Blackout];
        ctx.run_ui(egui::RawInput::default(), |ui| {
            actions = status_bar::draw(ui, &data, &mut state);
        })
        .drop_without_applying_deltas();
        assert_eq!(actions, Vec::<StatusAction>::new(), "aucun clic, aucun geste");
        assert_eq!(state.confirm_next_dj, confirm, "dessiner ne change pas l'étape");
    }
}

#[test]
fn remaining_time_comes_from_the_planned_schedule() {
    assert_eq!(remaining_s(None, 10.0), None);
    assert_eq!(remaining_s(Some((22 * 60, 23 * 60)), 600.0), Some(3000.0));
    // Un créneau qui passe minuit.
    assert_eq!(remaining_s(Some((23 * 60 + 30, 30)), 0.0), Some(3600.0));
    assert_eq!(remaining_s(Some((0, 1)), 90.0), Some(-30.0), "débordement : négatif");
}

#[test]
fn l_horloge_s_ecrit_a_la_milliseconde() {
    assert_eq!(clock_text(0), "00:00:00.000");
    assert_eq!(clock_text(3_723_045), "01:02:03.045");
    assert_eq!(clock_text(100 * 3_600_000), "100:00:00.000");
}

/// Une image egui avec ces événements ; `check` lit son `InputState`.
fn with_events(events: Vec<Event>, check: impl FnOnce(&egui::InputState)) {
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        events,
        ..egui::RawInput::default()
    };
    let mut check = Some(check);
    ctx.run_ui(raw, |ui| {
        if let Some(check) = check.take() {
            ui.ctx().input(check);
        }
    })
    .drop_without_applying_deltas();
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers,
    }
}
