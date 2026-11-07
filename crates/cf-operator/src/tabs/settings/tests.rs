//! Tests de l'onglet Réglages : profils, reprise au relancement, liste des DJ, confirmations, mise en page.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy_egui::egui;
use cf_render::Settings;
use cf_show::load::event_to_ron;
use cf_show::persist::{AppSettings, OperatorPrefs};

use super::profiles::{list_profiles, profile_path, save_evening};
use super::restore::{init_session, remember_evening, startup_event_path};
use super::*;
use crate::prepare::edit;
use crate::prepare::{PrepConfig, PrepSession};

fn assets() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets"))
}

fn config(profiles: Option<&Path>) -> PrepConfig {
    PrepConfig {
        assets_dir: assets(),
        show_path: None,
        profiles_dir: profiles.map(Path::to_path_buf),
    }
}

fn blank_session() -> PrepSession {
    PrepSession::start(&config(None)).expect("session neuve")
}

/// Un fichier d'événement valide (événement neuf) à `path`.
fn write_valid_event(path: &Path) {
    let event = blank_session().event().clone();
    std::fs::write(path, event_to_ron(&event).expect("RON")).expect("écriture");
}

/// La session de démarrage telle que la crée l'application, avec ces préférences.
fn started_app(config: PrepConfig, prefs: OperatorPrefs, settings_path: &Path) -> App {
    let mut app = App::new();
    let value = AppSettings {
        operator: prefs,
        ..AppSettings::default()
    };
    app.add_plugins(MinimalPlugins)
        .insert_resource(config)
        .insert_resource(Settings::new(value, settings_path.to_path_buf()))
        .add_systems(Startup, init_session)
        .add_systems(Update, remember_evening);
    app.update();
    app
}

/// Ajoute deux DJ nommés à la session (une étape d'annulation).
fn with_three_djs(session: &mut PrepSession) {
    let mut draft = session.event().clone();
    for name in ["Bravo", "Charlie"] {
        let i = edit::add_slot(&mut draft).expect("place pour un DJ");
        draft.slots[i].dj.name = name.to_owned();
    }
    draft.slots[0].dj.name = "Alpha".to_owned();
    session.commit(draft, None);
}

fn names(session: &PrepSession) -> Vec<String> {
    session.event().slots.iter().map(|s| s.dj.name.clone()).collect()
}

// --- Profils ---

#[test]
fn profile_path_rejects_traversal_and_empty_names() {
    let dir = Path::new("profils");
    for bad in [
        "", "   ", "..", "../x", "a/../b", "a/b", "a\\b", "a:b", "a?b", "a*b", "a<b", "a>b", "a|b", "a\"b", "a\tb",
    ] {
        assert_eq!(profile_path(dir, bad), None, "{bad:?} doit être refusé");
    }
    assert_eq!(profile_path(dir, "  Soirée  "), Some(dir.join("Soirée.ron")));
    assert_eq!(profile_path(dir, "Club 2"), Some(dir.join("Club 2.ron")));
}

#[test]
fn list_profiles_ignores_unreadable_files() {
    let dir = tempfile::tempdir().expect("dossier temporaire");
    write_valid_event(&dir.path().join("Zèbre.ron"));
    write_valid_event(&dir.path().join("alpha.ron"));
    write_valid_event(&dir.path().join("alpha.ron.bak"));
    std::fs::write(dir.path().join("cassé.ron"), "ceci n'est pas du RON").expect("écriture");
    std::fs::write(dir.path().join("notes.txt"), "rien").expect("écriture");
    std::fs::create_dir(dir.path().join("dossier.ron")).expect("dossier");
    assert_eq!(list_profiles(dir.path()), ["alpha", "Zèbre"]);
    assert_eq!(list_profiles(&dir.path().join("absent")), Vec::<String>::new());
}

// --- Reprise au relancement ---

#[test]
fn startup_prefers_cli_then_active_profile_then_last_path() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let profiles = root.path().join("profiles");
    std::fs::create_dir(&profiles).expect("dossier");
    let profile = profiles.join("Soirée.ron");
    write_valid_event(&profile);
    let last = root.path().join("dernier.ron");
    write_valid_event(&last);
    let cli = root.path().join("ligne-de-commande.ron");
    let prefs = OperatorPrefs {
        active_profile: Some("Soirée".to_owned()),
        last_event_path: Some(last.display().to_string()),
        ..OperatorPrefs::default()
    };

    assert_eq!(startup_event_path(&prefs, &profiles, Some(&cli)), Some(cli.clone()));
    assert_eq!(startup_event_path(&prefs, &profiles, None), Some(profile.clone()));
    std::fs::remove_file(&profile).expect("suppression");
    assert_eq!(startup_event_path(&prefs, &profiles, None), Some(last));
}

#[test]
fn startup_with_nothing_returns_none() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    assert_eq!(startup_event_path(&OperatorPrefs::default(), root.path(), None), None);
    let gone = OperatorPrefs {
        active_profile: Some("Disparu".to_owned()),
        last_event_path: Some(root.path().join("disparu.ron").display().to_string()),
        ..OperatorPrefs::default()
    };
    assert_eq!(startup_event_path(&gone, root.path(), None), None);
}

#[test]
fn corrupt_profile_falls_back_to_new_event() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let profiles = root.path().join("profiles");
    std::fs::create_dir(&profiles).expect("dossier");
    std::fs::write(profiles.join("Soirée.ron"), "(version: 1, cassé").expect("écriture");
    let prefs = OperatorPrefs {
        active_profile: Some("Soirée".to_owned()),
        ..OperatorPrefs::default()
    };
    assert_eq!(startup_event_path(&prefs, &profiles, None), None);

    let app = started_app(config(Some(&profiles)), prefs, &root.path().join("settings.ron"));
    let session = app.world().resource::<PrepSession>();
    assert_eq!(session.event().settings.name, "Nouvelle soirée", "événement neuf");
    assert_eq!(session.active_profile(), None);
    assert!(app.world().get_resource::<SettingsTab>().is_some());
}

#[test]
fn reorder_persists_after_save_and_reopen() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let profiles = root.path().join("profiles");
    let mut session = PrepSession::start(&config(Some(&profiles))).expect("session");
    let mut tab = SettingsTab::new(Some(profiles.clone()));
    with_three_djs(&mut session);

    // Charlie passe en tête, comme un glisser-déposer de la dernière ligne vers la première.
    let mut draft = session.event().clone();
    let mut selection = session.selection;
    apply_ops(&mut draft, &mut selection, &[Op::MoveSlot(2, 0)], &mut Touch::default());
    session.commit(draft, None);
    assert_eq!(names(&session), ["Charlie", "Alpha", "Bravo"]);

    assert!(save_evening(&mut session, &mut tab), "{:?}", session.status);
    assert_eq!(
        session.active_profile(),
        Some("Soirée"),
        "premier enregistrement : profil « Soirée »"
    );
    assert_eq!(tab.profiles, ["Soirée"]);
    assert!(!session.is_dirty());

    let mut prefs = OperatorPrefs::default();
    tab.remember.take().expect("geste retenu").apply(&mut prefs);
    assert_eq!(prefs.active_profile.as_deref(), Some("Soirée"));

    let app = started_app(config(Some(&profiles)), prefs, &root.path().join("settings.ron"));
    let reopened = app.world().resource::<PrepSession>();
    assert_eq!(names(reopened), ["Charlie", "Alpha", "Bravo"]);
    let ids = |s: &PrepSession| s.event().slots.iter().map(|x| x.id.clone()).collect::<Vec<_>>();
    assert_eq!(ids(reopened), ids(&session));
    assert_eq!(reopened.active_profile(), Some("Soirée"));
}

#[test]
fn delete_last_dj_is_refused_or_leaves_valid_event() {
    let mut session = blank_session();
    assert_eq!(session.event().slots.len(), 1);
    let mut draft = session.event().clone();
    let mut selection = session.selection;
    apply_ops(&mut draft, &mut selection, &[Op::RemoveSlot(0)], &mut Touch::default());
    assert_eq!(draft.slots.len(), 1, "le dernier DJ reste");
    session.commit(draft, None);
    assert!(session.issues().is_empty(), "{:?}", session.issues());

    with_three_djs(&mut session);
    let mut draft = session.event().clone();
    let mut selection = session.selection;
    apply_ops(&mut draft, &mut selection, &[Op::RemoveSlot(1)], &mut Touch::default());
    session.commit(draft, None);
    assert_eq!(names(&session), ["Alpha", "Charlie"]);
    assert!(session.issues().is_empty(), "{:?}", session.issues());
}

// --- Confirmations ---

#[test]
fn switching_dj_with_unsaved_changes_requires_confirmation() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let mut session = PrepSession::start(&config(Some(root.path()))).expect("session");
    let mut tab = SettingsTab::new(Some(root.path().to_path_buf()));
    with_three_djs(&mut session);
    assert!(save_evening(&mut session, &mut tab), "{:?}", session.status);

    // Fiche intacte : on change de DJ tout de suite.
    assert!(dj_list::select_dj(&mut session, &mut tab, 1));
    assert_eq!(session.selection.slot, 1);
    assert_eq!(tab.pending, None);

    let mut draft = session.event().clone();
    draft.slots[1].dj.name = "Bravo (modifié)".to_owned();
    session.commit(draft, None);
    assert!(
        !dj_list::select_dj(&mut session, &mut tab, 2),
        "fiche modifiée : confirmation"
    );
    assert_eq!(tab.pending, Some(Pending::SwitchDj(2)));
    assert_eq!(
        session.selection.slot, 1,
        "on reste sur le DJ tant que ce n'est pas confirmé"
    );

    resolve(&mut session, &mut tab, Choice::Stay);
    assert_eq!((session.selection.slot, tab.pending.clone()), (1, None));

    dj_list::select_dj(&mut session, &mut tab, 2);
    resolve(&mut session, &mut tab, Choice::Discard);
    assert_eq!(session.selection.slot, 2);
    assert_eq!(
        names(&session),
        ["Alpha", "Bravo", "Charlie"],
        "modification abandonnée"
    );
    assert!(!session.is_dirty());
}

#[test]
fn quit_with_dirty_session_asks_to_save() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let mut session = PrepSession::start(&config(Some(root.path()))).expect("session");
    let mut tab = SettingsTab::new(Some(root.path().to_path_buf()));
    let offered = |dirty| quit_choices(dirty).iter().map(|(c, _)| *c).collect::<Vec<_>>();
    assert_eq!(offered(session.is_dirty()), [QuitChoice::Quit, QuitChoice::Cancel]);

    with_three_djs(&mut session);
    assert_eq!(
        offered(session.is_dirty()),
        [
            QuitChoice::SaveAndQuit,
            QuitChoice::QuitWithoutSaving,
            QuitChoice::Cancel
        ]
    );
    // « Enregistrer et quitter » passe par le même enregistrement que Ctrl+S.
    assert!(save_evening(&mut session, &mut tab));
    assert_eq!(offered(session.is_dirty()), [QuitChoice::Quit, QuitChoice::Cancel]);
}

#[test]
fn an_invalid_event_is_refused_at_save() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let mut session = PrepSession::start(&config(Some(root.path()))).expect("session");
    let mut tab = SettingsTab::new(Some(root.path().to_path_buf()));
    let mut draft = session.event().clone();
    draft.settings.latency_offset_ms = 9_999.0;
    session.commit(draft, None);
    assert_ne!(session.issues(), []);
    assert!(!save_evening(&mut session, &mut tab));
    assert!(matches!(session.status, Status::Error(_)));

    assert!(list_profiles(root.path()).is_empty(), "rien n'est écrit");
}

#[test]
fn profile_path_rejects_windows_reserved_names() {
    let dir = Path::new("profils");
    for bad in [
        "CON", "con", "Prn", "aux", "NUL", "nul.txt", "COM1", "com9", "LPT1", "lpt9.ron", "CON .x", "Soirée.", "a. ",
    ] {
        assert_eq!(profile_path(dir, bad), None, "{bad:?} doit être refusé");
    }
    for good in ["CONcert", "COM0", "COM10", "Nulle part", "Aux platines", "LPT"] {
        assert!(profile_path(dir, good).is_some(), "{good:?} doit être accepté");
    }
}

// --- Ce qui est retenu pour le prochain lancement ---

/// Les préférences retenues après quelques images.
fn prefs_after_frames(app: &mut App) -> OperatorPrefs {
    for _ in 0..3 {
        app.update();
    }
    app.world().resource::<Settings>().value.operator.clone()
}

#[test]
fn show_launch_does_not_touch_remembered_profile() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let profiles = root.path().join("profiles");
    std::fs::create_dir(&profiles).expect("dossier");
    let profile = profiles.join("Soirée.ron");
    write_valid_event(&profile);
    let prefs = OperatorPrefs {
        active_profile: Some("Soirée".to_owned()),
        last_event_path: Some(profile.display().to_string()),
        ..OperatorPrefs::default()
    };
    let mut config = config(Some(&profiles));
    config.show_path = Some(assets().join("shows/demo.ron"));
    let mut app = started_app(config, prefs.clone(), &root.path().join("settings.ron"));
    assert_eq!(app.world().resource::<PrepSession>().active_profile(), None);
    assert_eq!(
        prefs_after_frames(&mut app),
        prefs,
        "un --show ne réécrit pas la soirée retenue"
    );
}

#[test]
fn fallback_does_not_clear_active_profile() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let profiles = root.path().join("profiles");
    std::fs::create_dir(&profiles).expect("dossier");
    std::fs::write(profiles.join("Soirée.ron"), "pas du RON").expect("écriture");
    let prefs = OperatorPrefs {
        active_profile: Some("Soirée".to_owned()),
        ..OperatorPrefs::default()
    };
    let mut app = started_app(
        config(Some(&profiles)),
        prefs.clone(),
        &root.path().join("settings.ron"),
    );
    assert_eq!(
        app.world().resource::<PrepSession>().active_profile(),
        None,
        "repli : soirée neuve"
    );
    assert_eq!(prefs_after_frames(&mut app), prefs, "le profil retenu n'est pas oublié");
}

#[test]
fn ctrl_s_in_show_session_rewrites_that_file_and_creates_no_profile() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let profiles = root.path().join("profiles");
    let show = root.path().join("show.ron");
    std::fs::copy(assets().join("shows/demo.ron"), &show).expect("copie");
    let mut config = config(Some(&profiles));
    config.show_path = Some(show.clone());
    let mut app = started_app(config, OperatorPrefs::default(), &root.path().join("settings.ron"));
    let mut session = app.world_mut().remove_resource::<PrepSession>().expect("session");
    let mut tab = app.world_mut().remove_resource::<SettingsTab>().expect("onglet");

    let mut draft = session.event().clone();
    draft.settings.name = "Renommée".to_owned();
    session.commit(draft, None);
    assert!(save_evening(&mut session, &mut tab), "{:?}", session.status);
    assert_eq!(session.active_profile(), None);
    assert_eq!(tab.remember, None, "aucun profil retenu");
    assert_eq!(list_profiles(&profiles), Vec::<String>::new(), "aucun profil créé");
    let text = std::fs::read_to_string(&show).expect("lecture");
    assert!(text.contains("Renommée"), "le fichier ouvert est réécrit");
}

#[test]
fn ctrl_s_new_session_creates_soiree_then_soiree_2() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    for expected in ["Soirée", "Soirée 2"] {
        let mut session = PrepSession::start(&config(Some(root.path()))).expect("session neuve");
        let mut tab = SettingsTab::new(Some(root.path().to_path_buf()));
        assert!(save_evening(&mut session, &mut tab), "{:?}", session.status);
        assert_eq!(session.active_profile(), Some(expected));
        let remembered = tab.remember.clone().expect("geste retenu");
        assert!(matches!(remembered, Remembered::Profile { ref name, .. } if name == expected));
        // Un deuxième Ctrl+S réécrit le même profil.
        assert!(save_evening(&mut session, &mut tab));
        assert_eq!(session.active_profile(), Some(expected));
    }
    assert_eq!(list_profiles(root.path()), ["Soirée", "Soirée 2"]);
}

#[test]
fn deleting_the_active_profile_detaches_the_evening() {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let mut session = PrepSession::start(&config(Some(root.path()))).expect("session neuve");
    let mut tab = SettingsTab::new(Some(root.path().to_path_buf()));
    assert!(save_evening(&mut session, &mut tab));
    profiles::delete_profile(&mut session, &mut tab, "Soirée");
    assert_eq!((session.active_profile(), session.doc_path()), (None, None));
    assert_eq!(tab.remember, Some(Remembered::NoProfile));
    assert!(matches!(&session.status, Status::Info(text) if text.contains("plus enregistrée")));
    assert!(save_evening(&mut session, &mut tab));
    assert_eq!(
        session.active_profile(),
        Some("Soirée"),
        "un nouveau profil, pas l'ancien fichier en douce"
    );
}

// --- Mise en page (sans fenêtre) ---

fn draw_at(width: f32, height: f32) -> egui::Rect {
    let root = tempfile::tempdir().expect("dossier temporaire");
    let mut session = PrepSession::start(&config(Some(root.path()))).expect("session");
    let mut tab = SettingsTab::new(Some(root.path().to_path_buf()));
    with_three_djs(&mut session);
    let ctx = egui::Context::default();
    let mut list = egui::Rect::NOTHING;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height))),
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| {
            let screens = cf_render::ScreenList::default();
            let card = ScreenCard {
                list: &screens,
                fullscreen: true,
                live: false,
                now_s: 0.0,
            };
            let mut orders = Vec::new();
            list = draw(ui, &mut session, &mut tab, &[], card, &mut orders, |ui| {
                ui.label("mode");
            });
            assert!(orders.is_empty(), "aucun clic, aucun ordre");
        })
        .drop_without_applying_deltas();
    }
    assert_eq!(tab.pending, None, "aucun clic, aucune confirmation");
    list
}

#[test]
fn the_tab_draws_at_640_by_400() {
    let list = draw_at(640.0, 400.0);
    assert!(list.width() > 0.0 && list.height() > 0.0, "{list:?}");
}

#[test]
fn the_tab_draws_at_1280_by_720() {
    let list = draw_at(1280.0, 720.0);
    assert!(list.width() > 0.0 && list.height() > 0.0, "{list:?}");
}
