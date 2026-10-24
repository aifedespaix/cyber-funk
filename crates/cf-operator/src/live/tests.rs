//! Tests de la vue Live sans écran : un set de trois DJs joué par les commandes de la vue, le Blackout tenu relâché à
//! la passation, la mise en page dessinée avec tous ses raccourcis et aucun texte sous 14 px.

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::{KeyCode, KeyboardInput};
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowFocused;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};
use cf_control::{Action, Switch};
use cf_render::audio::AudioFeatures;
use cf_render::{CfSet, ControlsConfig, ControlsPlugin, CorePlugin, CurrentShow, MidiChoice, ShowPlugin, ShowSheet};
use cf_show::director::ShowState;
use cf_show::load::{LoadedShow, LookLibrary};
use cf_show::model::{SlotId, SlotMode};

use super::commands::{ALL, CommandContext, LiveCommand};
use super::view::{self, LiveData};
use super::widgets::MIN_TEXT;
use super::*;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Le show de démonstration porté à trois créneaux, tous en conduite manuelle (rien n'avance sans l'opérateur).
fn three_dj_show() -> LoadedShow {
    let library = LookLibrary::load_dir(&assets().join("looks")).expect("looks livrés");
    let mut event = LoadedShow::load(&assets().join("shows/demo.ron"), &library)
        .expect("show de démonstration")
        .event()
        .clone();
    let mut third = event.slots[0].clone();
    third.id = SlotId::new("lune").expect("identifiant");
    third.dj.name = "Lune".to_owned();
    event.slots.push(third);
    for slot in &mut event.slots {
        slot.mode = SlotMode::Manual;
    }
    LoadedShow::new(event, &library, "test").expect("événement valide")
}

#[derive(Resource, Default)]
struct Seen(Vec<Action>);

fn collect(mut seen: ResMut<Seen>, mut actions: MessageReader<ControlAction>) {
    seen.0.extend(actions.read().map(|a| a.0.action.clone()));
}

/// Le spectacle, les contrôles, la vue Live (sans sa partie egui : seulement les systèmes d'envoi) et une fenêtre
/// opérateur factice qui porte le contexte egui.
fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, CorePlugin))
        .init_resource::<AudioFeatures>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_message::<KeyboardInput>()
        .add_message::<WindowFocused>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
        .add_plugins((
            ControlsPlugin::new(ControlsConfig {
                midi: MidiChoice::Off,
                ..ControlsConfig::default()
            }),
            ShowPlugin::new(three_dj_show()),
        ))
        .init_resource::<Seen>();
    build(&mut app);
    app.add_systems(Update, (live_input, flush_orders).chain().before(CfSet::Show))
        .add_systems(Update, collect.after(CfSet::Show));
    app.world_mut().spawn(PrimaryEguiContext);
    app.update();
    app.update();
    app
}

fn state(app: &App) -> &ShowState {
    &app.world().resource::<CurrentShow>().0
}

/// Fait ce que fait le clic sur le bouton de cette commande : son `Action`, par le même `LiveOrders`.
fn click(app: &mut App, command: LiveCommand) {
    let world = app.world_mut();
    let sheet = world.resource::<ShowSheet>().clone();
    let show = world.resource::<CurrentShow>().0.clone();
    let features = AudioFeatures::default();
    let data = LiveData {
        state: &show,
        sheet: &sheet,
        features: Some(&features),
        controls: None,
        diagnostics: None,
        drop_plan: None,
    };
    let action = command.action(&data.command_context());
    world.resource_mut::<LiveOrders>().0.extend(action);
}

/// Joue des images jusqu'à `done` ; échoue au bout de `max_s` secondes de show.
fn until(app: &mut App, max_s: f64, what: &str, done: impl Fn(&App) -> bool) {
    for _ in 0..(max_s * 60.0) as usize {
        app.update();
        if done(app) {
            return;
        }
    }
    assert!(done(app), "{what} : rien après {max_s} s de show");
}

#[test]
fn la_feuille_de_conduite_porte_les_trois_djs() {
    let app = app();
    let sheet = app.world().resource::<ShowSheet>();
    let names: Vec<&str> = sheet.slots.iter().map(|s| s.dj.as_str()).collect();
    assert_eq!(names, ["Aya", "KRMN", "Lune"]);
    assert!(sheet.slots.iter().all(|s| !s.cues.is_empty()));
    // Avec un show, la vue s'ouvre en Live.
    assert_eq!(*app.world().resource::<OperatorMode>(), OperatorMode::Live);
}

#[test]
fn un_set_de_trois_djs_se_joue_uniquement_par_les_commandes_de_la_vue() {
    let mut app = app();
    assert_eq!(state(&app).slot, 0);

    // DJ 1 : GO jusqu'au dernier cue de son créneau, puis on arme le suivant et on lance la passation.
    for expected_slot in 1..=2 {
        click(&mut app, LiveCommand::Go);
        let cue = state(&app).cue;
        // Un cue qui entre « au drop » attend au plus MAX_LATE = 90 s (règle de Sillage).
        until(&mut app, 200.0, "entrée du cue suivant", |a| state(a).cue != cue);

        click(&mut app, LiveCommand::ArmNext);
        app.update();
        assert_eq!(
            state(&app).armed_slot,
            Some(expected_slot),
            "« Armer le suivant » vise le bon créneau"
        );

        click(&mut app, LiveCommand::NextDj);
        until(&mut app, 120.0, "passation", |a| state(a).slot == expected_slot);
    }
    assert_eq!(state(&app).slot, 2, "le troisième DJ joue");

    // Retour au DJ précédent par la vue aussi.
    click(&mut app, LiveCommand::PrevDj);
    until(&mut app, 120.0, "retour au DJ précédent", |a| state(a).slot == 1);

    let seen = &app.world().resource::<Seen>().0;
    assert!(seen.contains(&Action::NextDj) && seen.contains(&Action::Go));
}

#[test]
fn les_boutons_a_bascule_passent_par_le_chef_du_spectacle() {
    let mut app = app();
    click(&mut app, LiveCommand::Hold);
    click(&mut app, LiveCommand::Blackout);
    click(&mut app, LiveCommand::FlashKill);
    app.update();
    app.update();
    let s = state(&app);
    assert!(s.hold && s.blackout_on && !s.flashes);
    // Un second clic éteint : la bascule est résolue par cf-show, pas par la vue.
    click(&mut app, LiveCommand::Blackout);
    app.update();
    app.update();
    assert!(!state(&app).blackout_on);
}

/// Une image avec la touche K dans cet état (`None` : aucun changement).
fn frame_with_k(app: &mut App, change: Option<bool>) {
    let events = change
        .map(|pressed| egui::Event::Key {
            key: egui::Key::K,
            physical_key: Some(egui::Key::K),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        })
        .into_iter()
        .collect();
    let raw = egui::RawInput {
        events,
        ..egui::RawInput::default()
    };
    let mut query = app.world_mut().query::<&mut EguiContext>();
    let ctx = query
        .single_mut(app.world_mut())
        .expect("contexte opérateur")
        .get_mut()
        .clone();
    ctx.begin_pass(raw);
    app.update();
    // La sortie de la passe ne nous intéresse pas : la vue n'est pas dessinée ici.
    ctx.end_pass().drop_without_applying_deltas();
}

#[test]
fn le_blackout_tenu_est_relache_a_la_passation_et_ne_se_rearme_pas_seul() {
    let mut app = app();
    app.world_mut().resource_mut::<Seen>().0.clear();

    // K enfoncée : noir.
    frame_with_k(&mut app, Some(true));
    frame_with_k(&mut app, None);
    frame_with_k(&mut app, None);
    assert!(state(&app).blackout_on, "le maintien allume le noir");

    // Passation vers le DJ suivant, K toujours enfoncée.
    click(&mut app, LiveCommand::NextDj);
    for _ in 0..(120 * 60) {
        frame_with_k(&mut app, None);
        if state(&app).slot == 1 {
            break;
        }
    }
    assert_eq!(state(&app).slot, 1, "la passation a eu lieu");
    for _ in 0..3 {
        frame_with_k(&mut app, None);
    }
    assert!(!state(&app).blackout_on, "le noir tenu est relâché au changement de DJ");
    let seen = app.world().resource::<Seen>().0.clone();
    assert!(seen.contains(&Action::Blackout(Switch::Off)));

    // La touche est restée enfoncée : rien ne se rallume tout seul.
    for _ in 0..30 {
        frame_with_k(&mut app, None);
    }
    assert!(!state(&app).blackout_on);

    // Relâchée puis repressée : le maintien repart.
    frame_with_k(&mut app, Some(false));
    frame_with_k(&mut app, Some(true));
    frame_with_k(&mut app, None);
    assert!(state(&app).blackout_on);
    frame_with_k(&mut app, Some(false));
    frame_with_k(&mut app, None);
    frame_with_k(&mut app, None);
    assert!(!state(&app).blackout_on, "relâcher la touche éteint le noir");
}

/// Tous les textes dessinés (galets de texte) et la plus petite taille de police utilisée.
fn drawn_texts(output: &egui::FullOutput) -> (String, f32) {
    let mut all = String::new();
    let mut smallest = f32::INFINITY;
    for clipped in &output.shapes {
        collect_text(&clipped.shape, &mut all, &mut smallest);
    }
    (all, smallest)
}

fn collect_text(shape: &egui::epaint::Shape, all: &mut String, smallest: &mut f32) {
    match shape {
        egui::epaint::Shape::Text(text) => {
            all.push_str(text.galley.text());
            all.push('\n');
            for section in &text.galley.job.sections {
                *smallest = smallest.min(section.format.font_id.size);
            }
        }
        egui::epaint::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect_text(s, all, smallest)),
        _ => {}
    }
}

#[test]
fn la_vue_se_dessine_avec_tous_ses_raccourcis_et_sans_texte_sous_14_px() {
    let app = app();
    let sheet = app.world().resource::<ShowSheet>().clone();
    let show = app.world().resource::<CurrentShow>().0.clone();
    let features = AudioFeatures::default();
    let data = LiveData {
        state: &show,
        sheet: &sheet,
        features: Some(&features),
        controls: None,
        diagnostics: None,
        drop_plan: None,
    };
    let ctx = egui::Context::default();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
        ..egui::RawInput::default()
    };
    let mut orders = Vec::new();
    let mut mode_request = false;
    // Deux images : les panneaux se dimensionnent à la première, le contenu hors cadre n'est dessiné qu'à la suivante.
    ctx.run_ui(raw.clone(), |ui| view::draw(ui, &data, &mut orders, &mut mode_request))
        .drop_without_applying_deltas();
    let output = ctx.run_ui(raw, |ui| view::draw(ui, &data, &mut orders, &mut mode_request));
    let (text, smallest) = drawn_texts(&output);
    output.drop_without_applying_deltas();

    // Chaque bouton montre son raccourci. Les potards de macro n'existent que si le look en a.
    let context: CommandContext<'_> = data.command_context();
    for command in ALL {
        let is_macro = matches!(command, LiveCommand::MacroUp(_) | LiveCommand::MacroDown(_));
        // Les boutons de pas (Master ±, Macro ±) et le raccourci de macro sont des touches, pas des boutons.
        let is_key_only = matches!(command, LiveCommand::MasterUp | LiveCommand::MasterDown) || is_macro;
        if is_key_only {
            continue;
        }
        let shown = format!("{}  [{}]", command.label(), command.shortcut_text());
        assert!(text.contains(&shown), "bouton « {shown} » absent de la vue :\n{text}");
    }
    // Master : son raccourci est écrit sous le curseur.
    assert!(text.contains("PgUp / PgDn"));
    assert!(
        smallest >= MIN_TEXT,
        "texte de {smallest} points : la vue Live ne descend pas sous {MIN_TEXT}"
    );
    assert!(text.contains("CUES de Aya"), "{text}");
    assert!(orders.is_empty() && !mode_request, "dessiner n'envoie rien");
    let _ = context;
}

#[test]
fn le_changement_de_mode_demande_une_confirmation() {
    let mut app = app();
    // Ctrl+M demande le passage en Préparation, sans le faire.
    let raw = egui::RawInput {
        events: vec![egui::Event::Key {
            key: egui::Key::M,
            physical_key: Some(egui::Key::M),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL,
        }],
        ..egui::RawInput::default()
    };
    let mut query = app.world_mut().query::<&mut EguiContext>();
    let ctx = query.single_mut(app.world_mut()).expect("contexte").get_mut().clone();
    ctx.begin_pass(raw);
    app.update();
    ctx.end_pass().drop_without_applying_deltas();
    assert_eq!(
        app.world().resource::<ModePrompt>().target,
        Some(OperatorMode::Preparation)
    );
    assert_eq!(
        *app.world().resource::<OperatorMode>(),
        OperatorMode::Live,
        "pas encore déverrouillé"
    );
}
