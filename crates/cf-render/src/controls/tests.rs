//! Tests du pont des contrôles dans une `App` sans rendu : clavier synthétique, MIDI factice, temps manuel.

use std::time::Duration;

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode, KeyboardInput};
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowFocused;
use cf_control::{Action, Binding, ControlProfile, InputSource, Switch};

use super::*;
use crate::CorePlugin;
use crate::audio::AudioFeatures;

/// Ce que les tests observent.
#[derive(Resource, Default)]
pub(crate) struct Seen {
    pub actions: Vec<Action>,
}

fn collect(mut seen: ResMut<Seen>, mut actions: MessageReader<ControlAction>) {
    seen.actions.extend(actions.read().map(|a| a.0.action.clone()));
}

/// Une image à 60 i/s, à chaque `update`, quelle que soit la vitesse de la machine.
pub(crate) fn frame_time() -> TimeUpdateStrategy {
    TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0))
}

/// Une application avec les contrôles seuls (pas de spectacle).
pub(crate) fn controls_app(config: ControlsConfig) -> App {
    controls_app_with(config, |_| {})
}

/// Comme [`controls_app`], avec `extra` pour ajouter d'autres plugins (le spectacle) après les contrôles.
pub(crate) fn controls_app_with(config: ControlsConfig, extra: impl FnOnce(&mut App)) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin))
        .init_resource::<AudioFeatures>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<KeyboardInput>()
        .add_message::<WindowFocused>()
        .insert_resource(frame_time())
        .add_plugins(ControlsPlugin::new(config))
        .init_resource::<Seen>()
        .add_systems(Update, collect.after(CfSet::Show));
    extra(&mut app);
    app
}

pub(crate) fn key_message(code: KeyCode, key: Key, state: ButtonState) -> KeyboardInput {
    KeyboardInput {
        key_code: code,
        logical_key: key,
        state,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    }
}

pub(crate) fn press(app: &mut App, code: KeyCode, key: Key) {
    app.world_mut()
        .write_message(key_message(code, key, ButtonState::Pressed));
}

pub(crate) fn release(app: &mut App, code: KeyCode, key: Key) {
    app.world_mut()
        .write_message(key_message(code, key, ButtonState::Released));
}

pub(crate) fn seen(app: &App) -> &[Action] {
    &app.world().resource::<Seen>().actions
}

#[test]
fn fleche_droite_donne_go_dans_les_deux_fenetres() {
    let mut app = controls_app(ControlsConfig {
        user_profile_path: None,
        midi: MidiChoice::Off,
        ..ControlsConfig::default()
    });
    // Le pont ne filtre pas sur la fenêtre : projecteur et opérateur sont lus de la même façon (deux fenêtres =
    // deux entités dans `KeyboardInput::window`, que le pont ignore).
    for _ in 0..2 {
        press(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
        app.update();
        release(&mut app, KeyCode::ArrowRight, Key::ArrowRight);
        app.update();
    }
    assert_eq!(seen(&app), [Action::Go, Action::Go]);
}

#[test]
fn les_lettres_suivent_la_disposition_du_clavier() {
    let mut app = controls_app(ControlsConfig {
        user_profile_path: None,
        midi: MidiChoice::Off,
        ..ControlsConfig::default()
    });
    // AZERTY : la touche à l'emplacement du « D » QWERTY produit « d » ; ici l'emplacement est sans importance.
    press(&mut app, KeyCode::KeyD, Key::Character("d".into()));
    app.update();
    assert_eq!(seen(&app), [Action::ForceDrop]);
}

#[test]
fn taper_dans_un_champ_texte_ne_declenche_rien() {
    let mut app = controls_app(ControlsConfig {
        user_profile_path: None,
        midi: MidiChoice::Off,
        ..ControlsConfig::default()
    });
    app.world_mut().resource_mut::<TextInputFocus>().0 = true;
    for (code, key) in [
        (KeyCode::KeyD, Key::Character("d".into())),
        (KeyCode::ArrowRight, Key::ArrowRight),
        (KeyCode::Digit2, Key::Character("é".into())),
        (KeyCode::KeyG, Key::Character("g".into())),
    ] {
        press(&mut app, code, key.clone());
        app.update();
        release(&mut app, code, key);
        app.update();
    }
    assert!(
        seen(&app).is_empty(),
        "aucune action tapée dans egui : {:?}",
        seen(&app)
    );
    // Le champ perd le focus : le clavier redevient une commande.
    app.world_mut().resource_mut::<TextInputFocus>().0 = false;
    press(&mut app, KeyCode::KeyD, Key::Character("d".into()));
    app.update();
    assert_eq!(seen(&app), [Action::ForceDrop]);
}

#[test]
fn perdre_le_focus_de_la_fenetre_relache_les_boutons_tenus() {
    // Un binding à maintien sur une touche : tant qu'elle est enfoncée, le noir est allumé.
    let mut profile = ControlProfile::new("test");
    profile.bindings.push(Binding {
        mode: cf_control::Mode::Momentary,
        ..Binding::new(
            InputSource::key(cf_control::Key::Char('b')),
            cf_control::ActionTemplate::Blackout,
        )
    });
    let mut app = controls_app(ControlsConfig {
        user_profile_path: None,
        midi: MidiChoice::Off,
        ..ControlsConfig::default()
    });
    // Pas de spectacle ici : on charge le profil à la main, comme le ferait `SlotLoaded`.
    {
        let mut bridge = app.world_mut().resource_mut::<ControlBridge>();
        let bridge = &mut *bridge;
        assert!(bridge.mapper.load(&[&bridge.keyboard, &profile]).is_ok());
    }
    press(&mut app, KeyCode::KeyB, Key::Character("b".into()));
    app.update();
    assert_eq!(seen(&app), [Action::Blackout(Switch::On)]);
    let window = Entity::PLACEHOLDER;
    app.world_mut().write_message(WindowFocused { window, focused: false });
    app.update();
    assert_eq!(
        seen(&app),
        [Action::Blackout(Switch::On), Action::Blackout(Switch::Off)],
        "la fenêtre perd le clavier : le noir ne reste pas allumé"
    );
}

/// Fait tourner l'application jusqu'à ce que `done` soit vrai (3 s au plus : le MIDI passe par un autre fil).
fn until_true(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..180 {
        app.update();
        if done(app) {
            return;
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    unreachable!("délai dépassé : {what}");
}

#[test]
fn un_pad_appris_a_go_survit_a_un_redemarrage() {
    use cf_control::ActionTemplate;

    let dir = tempfile::tempdir().expect("dossier temporaire");
    let path = dir.path().join("user-controls.ron");
    let fake = FakeMidi::new();
    fake.plug("Pads");
    let config = |fake: &FakeMidi| ControlsConfig {
        midi: MidiChoice::Fake(fake.clone()),
        user_profile_path: Some(path.clone()),
        ..ControlsConfig::default()
    };

    // Session 1 : l'opérateur choisit GO, appuie sur un pad.
    let mut app = controls_app(config(&fake));
    until_true(&mut app, "ouverture du port", |_| !fake.open_ports().is_empty());
    app.world_mut().write_message(LearnCommand::Start(ActionTemplate::Go));
    app.update();
    assert_eq!(
        app.world().resource::<ControlsStatus>().learning,
        Some(ActionTemplate::Go)
    );
    assert!(fake.send("Pads", &[0x90, 40, 100]));
    until_true(&mut app, "binding appris", |a| {
        a.world().resource::<ControlsStatus>().learning.is_none()
    });
    app.update();
    assert!(path.is_file(), "le profil est enregistré");
    assert_eq!(app.world().resource::<ControlsStatus>().user_profile.bindings.len(), 1);
    // Pendant l'écoute le pad n'a rien déclenché ; maintenant il joue GO.
    assert_eq!(seen(&app), []);
    assert!(fake.send("Pads", &[0x80, 40, 0]));
    assert!(fake.send("Pads", &[0x90, 40, 100]));
    until_true(&mut app, "GO par le pad", |a| seen(a).contains(&Action::Go));
    drop(app);

    // Session 2 : redémarrage, le profil est relu et le pad joue encore GO.
    let fake = FakeMidi::new();
    fake.plug("Pads");
    let mut app = controls_app(config(&fake));
    assert_eq!(app.world().resource::<ControlsStatus>().user_profile.bindings.len(), 1);
    until_true(&mut app, "ouverture du port", |_| !fake.open_ports().is_empty());
    assert!(fake.send("Pads", &[0x90, 40, 100]));
    until_true(&mut app, "GO après redémarrage", |a| seen(a).contains(&Action::Go));
}

#[test]
fn retirer_une_ligne_du_profil_la_supprime_du_mapper_et_du_fichier() {
    use cf_control::ActionTemplate;

    let dir = tempfile::tempdir().expect("dossier temporaire");
    let path = dir.path().join("user.ron");
    let fake = FakeMidi::new();
    fake.plug("Pads");
    let mut app = controls_app(ControlsConfig {
        midi: MidiChoice::Fake(fake.clone()),
        user_profile_path: Some(path.clone()),
        ..ControlsConfig::default()
    });
    until_true(&mut app, "ouverture du port", |_| !fake.open_ports().is_empty());
    app.world_mut().write_message(LearnCommand::Start(ActionTemplate::Go));
    app.update();
    assert!(fake.send("Pads", &[0x90, 41, 100]));
    until_true(&mut app, "binding appris", |a| {
        a.world().resource::<ControlsStatus>().learning.is_none()
    });
    app.update();
    app.world_mut().write_message(LearnCommand::Remove(0));
    app.update();
    app.update();
    assert_eq!(app.world().resource::<ControlsStatus>().user_profile.bindings, []);
    let saved = ControlProfile::from_ron(&std::fs::read_to_string(&path).expect("fichier")).expect("profil");
    assert_eq!(saved.bindings, []);
}

#[test]
fn importer_un_profil_invalide_est_refuse() {
    use cf_control::ActionTemplate;

    let mut app = controls_app(ControlsConfig {
        midi: MidiChoice::Off,
        user_profile_path: None,
        ..ControlsConfig::default()
    });
    let source = InputSource::MidiNote {
        port: None,
        channel: 0,
        note: 1,
    };
    let mut bad = ControlProfile::new("import");
    bad.bindings.push(Binding::new(source.clone(), ActionTemplate::Go));
    bad.bindings.push(Binding::new(source, ActionTemplate::Back));
    app.world_mut().write_message(LearnCommand::Import(bad));
    app.update();
    let status = app.world().resource::<ControlsStatus>();
    assert!(status.learn_notice.as_deref().is_some_and(|n| n.contains("refusé")));
    assert_eq!(status.user_profile.bindings, []);
}
