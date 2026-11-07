#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "tests : un échec d'aide doit faire échouer le test bruyamment"
)]

use std::collections::BTreeSet;

use bevy::prelude::*;
use bevy_egui::egui;
use cf_control::{Action, TriggerId};
use cf_render::{
    ActiveEvent, ActiveEvents, ActiveId, EventCategory, EventOrigin, Settings, WorldAutomation, WorldEvent,
    WorldEventAppExt, WorldEventCatalog,
};
use cf_show::district::TransitionKind;
use cf_show::persist::AppSettings;

use super::chip::{MAX_ORDERS_PER_FRAME, chip_run, chip_text, fire, push_order};
use super::columns::{PlanCache, column_for_family, column_of, plan_columns, runs_as};
use super::districts::{TRANSITION_FALLBACK_TIP, TRANSITIONS, transition_tooltip};
use super::grid::{split_column, split_columns};
use super::lane::{lane_rows, row_time_text};
use super::pad::PadMove;
use super::top_bar::{load_ratio, wants_search_focus};
use super::{DebugState, DebugView, Gesture, apply_gestures, camera_actions, draw};

fn event(id: &'static str, label: &'static str, family: &'static str) -> WorldEvent {
    WorldEvent {
        id,
        label,
        family,
        note: "",
        category: None,
    }
}

fn catalog() -> WorldEventCatalog {
    let mut catalog = WorldEventCatalog::default();
    catalog.register(event("storm", "Orage", "Trafic et ciel"));
    catalog.register(event("fish", "Banc de poissons", "Trafic et ciel"));
    catalog.register(WorldEvent {
        category: Some(EventCategory::Sign),
        ..event("adship", "Vaisseau publicitaire", "Trafic et ciel")
    });
    catalog.register(event("landmark:arcade", "Borne d'arcade", "Immeubles-vitrines"));
    catalog
}

fn running(event_id: &'static str, category: EventCategory, started_s: f64, duration_s: Option<f64>) -> ActiveEvent {
    ActiveEvent {
        id: ActiveId::UNSET,
        event_id,
        label: event_id.to_owned(),
        category,
        origin: EventOrigin::Manual,
        started_s,
        duration_s,
    }
}

fn ids(events: &[&WorldEvent]) -> Vec<&'static str> {
    events.iter().map(|e| e.id).collect()
}

// --- Table familles → colonnes ---

/// Le catalogue réel : les trois inscriptions du monde (trafic, immeubles-vitrines, quartiers), comme leurs plugins.
fn real_catalog() -> WorldEventCatalog {
    let mut app = App::new();
    app.register_world_events(cf_world_neon::traffic::catalog::world_events())
        .register_world_events(cf_world_neon::landmarks::catalog::world_events())
        .register_world_events(cf_world_neon::districts::world_events());
    app.world_mut().remove_resource::<WorldEventCatalog>().unwrap()
}

/// La catégorie que le monde écrira dans `ActiveEvents` en lançant cet événement.
fn published_category(id: &str) -> EventCategory {
    use cf_world_neon::traffic::{EventName, catalog::category};
    if let Some(name) = EventName::from_slug(id) {
        category(name)
    } else if cf_world_neon::landmarks::catalog::kind_of(id).is_some() {
        // `landmark:<nom>` force l'immeuble puis lance l'événement `landmark` du trafic.
        category(EventName::Landmark)
    } else if cf_world_neon::districts::kind_of(id).is_some() {
        EventCategory::District
    } else {
        panic!("« {id} » : déclencheur inconnu du test, dire quelle catégorie le monde publie")
    }
}

#[test]
fn every_catalog_family_has_a_column() {
    let catalog = real_catalog();
    assert!(catalog.all().len() > 40, "le catalogue réel est assemblé");
    for family in catalog.families() {
        assert!(
            column_for_family(family).is_some(),
            "la famille « {family} » n'a pas de colonne : l'ajouter à la table de `columns.rs`"
        );
    }
    // Chaque puce tombe dans la colonne de son entrée « En cours ».
    for event in catalog.all() {
        assert_eq!(
            column_of(event),
            Some(published_category(event.id)),
            "« {} » ({}) : la puce et son entrée « En cours » seraient dans deux colonnes",
            event.id,
            event.family
        );
    }
    for (id, column) in [
        ("adship", EventCategory::Sign),
        ("holo-title", EventCategory::Sign),
        ("tower", EventCategory::Building),
        ("landmark", EventCategory::Building),
        ("blackout", EventCategory::Building),
        ("city-pulse", EventCategory::Building),
        ("storm", EventCategory::Car),
        ("district:suburb", EventCategory::District),
    ] {
        let event = catalog.all().iter().find(|e| e.id == id);
        let event = event.unwrap_or_else(|| panic!("« {id} » absent du catalogue réel"));
        assert_eq!(column_of(event), Some(column), "{id}");
    }
    let plan = plan_columns(&catalog, "", &BTreeSet::new());
    assert!(plan.others.is_empty(), "aucun événement dans « Autres »");
    assert!(plan.columns.iter().all(|c| !c.is_empty()), "aucune colonne vide");
}

#[test]
fn plan_columns_puts_pinned_first() {
    let catalog = catalog();
    let pinned = BTreeSet::from(["fish".to_owned()]);
    let plan = plan_columns(&catalog, "", &pinned);
    assert_eq!(
        ids(&plan.columns[2]),
        ["fish", "storm"],
        "épinglé en tête, ordre du catalogue ensuite"
    );
    assert_eq!(ids(&plan.columns[0]), ["adship"], "catégorie explicite");
    assert_eq!(ids(&plan.columns[1]), ["landmark:arcade"]);
    let none = plan_columns(&catalog, "", &BTreeSet::new());
    assert_eq!(ids(&none.columns[2]), ["storm", "fish"]);
}

#[test]
fn plan_columns_filter_is_case_insensitive() {
    let catalog = catalog();
    let plan = plan_columns(&catalog, "  ARCADE ", &BTreeSet::new());
    assert_eq!(ids(&plan.columns[1]), ["landmark:arcade"]);
    assert!(plan.columns[2].is_empty() && plan.columns[0].is_empty());
    let orage = plan_columns(&catalog, "oRaGe", &BTreeSet::new());
    assert_eq!(ids(&orage.columns[2]), ["storm"]);
}

#[test]
fn unknown_family_goes_to_others() {
    let mut catalog = catalog();
    catalog.register(event("ufo", "Soucoupe", "Extraterrestres"));
    assert_eq!(column_for_family("Extraterrestres"), None);
    let plan = plan_columns(&catalog, "", &BTreeSet::new());
    assert_eq!(ids(&plan.others), ["ufo"]);
    assert!(plan.columns.iter().all(|c| !ids(c).contains(&"ufo")));
}

#[test]
fn the_plan_cache_follows_filter_pins_and_catalog() {
    let mut catalog = catalog();
    let mut cache = PlanCache::default();
    let pinned = BTreeSet::new();
    assert!(cache.refresh(&catalog, "", &pinned), "premier calcul");
    assert!(
        !cache.refresh(&catalog, "", &pinned),
        "rien n'a changé : rien recalculé"
    );
    assert_eq!(
        cache
            .column(&catalog, EventCategory::Car)
            .map(|e| e.id)
            .collect::<Vec<_>>(),
        ["storm", "fish"]
    );
    assert!(cache.refresh(&catalog, "fish", &pinned));
    assert_eq!(cache.column(&catalog, EventCategory::Car).count(), 1);
    let pinned = BTreeSet::from(["storm".to_owned()]);
    assert!(cache.refresh(&catalog, "fish", &pinned));
    catalog.register(event("ufo", "Soucoupe", "Extraterrestres"));
    assert!(cache.refresh(&catalog, "", &pinned));
    assert_eq!(cache.others(&catalog).map(|e| e.id).collect::<Vec<_>>(), ["ufo"]);
}

#[test]
fn a_chip_matches_its_running_entry_by_id_or_by_prefix() {
    assert!(runs_as("storm", "storm"));
    assert!(runs_as("district:neon", "district:neon"));
    // Les immeubles-vitrines sont publiés sous l'identifiant de l'événement du trafic qui les montre.
    assert!(runs_as("landmark:arcade", "landmark"));
    assert!(!runs_as("storm", "storms"));
    assert!(!runs_as("district:neon", "district:suburb"));
    assert!(!runs_as("landmark", "landmark:arcade"));
}

// --- Puces ---

#[test]
fn fire_respects_per_frame_cap() {
    let mut orders = Vec::new();
    let storm = event("storm", "Orage", "Trafic et ciel");
    assert!(fire(&mut orders, &storm));
    assert_eq!(orders, [Action::Trigger(TriggerId::new("storm"))]);
    let accepted = (0..100).filter(|_| fire(&mut orders, &storm)).count();
    assert_eq!(accepted, MAX_ORDERS_PER_FRAME - 1);
    assert_eq!(orders.len(), MAX_ORDERS_PER_FRAME);
    assert!(
        !push_order(&mut orders, Action::SkipShot),
        "le plafond vaut pour tous les ordres du Debug"
    );
}

#[test]
fn chip_progress_uses_max_of_running_instances() {
    let mut active = ActiveEvents::default();
    assert_eq!(chip_run("storm", &active, 0.0).count, 0);
    active.begin(running("storm", EventCategory::Car, 0.0, Some(10.0)));
    active.begin(running("storm", EventCategory::Car, 6.0, Some(10.0)));
    active.begin(running("fish", EventCategory::Car, 0.0, Some(1.0)));
    let run = chip_run("storm", &active, 8.0);
    assert_eq!(run.count, 2);
    assert!((run.progress.unwrap() - 0.8).abs() < 1e-6, "{run:?}");
    // Une instance sans durée : la puce est active, sans remplissage inventé.
    let mut open = ActiveEvents::default();
    open.begin(running("dj-sign", EventCategory::Sign, 0.0, None));
    let run = chip_run("dj-sign", &open, 3.0);
    assert_eq!((run.count, run.progress), (1, None));
    assert_eq!(chip_text("Orage", false, run), "Orage");
    assert_eq!(chip_text("Orage", true, chip_run("storm", &active, 8.0)), "★ Orage ×2");
}

// --- Couloirs « En cours » ---

#[test]
fn lanes_show_running_events_of_their_column_with_time_left() {
    let mut active = ActiveEvents::default();
    active.begin(ActiveEvent {
        origin: EventOrigin::Auto,
        ..running("stunt:loop", EventCategory::Car, 0.0, Some(8.0))
    });
    active.begin(running("dj-sign", EventCategory::Sign, 0.0, None));
    let cars: Vec<_> = lane_rows(&active, EventCategory::Car, 2.0).collect();
    assert_eq!(
        cars.len(),
        1,
        "les cascades, absentes du catalogue, ont quand même leur ligne"
    );
    assert!(cars[0].auto);
    assert_eq!(row_time_text(&cars[0]), "25% · 6 s");
    let signs: Vec<_> = lane_rows(&active, EventCategory::Sign, 2.0).collect();
    assert_eq!(signs.len(), 1, "l'enseigne du DJ va avec les panneaux");
    assert_eq!(row_time_text(&signs[0]), "en cours");
    assert_eq!(lane_rows(&active, EventCategory::District, 2.0).count(), 0);
}

// --- Quartiers ---

#[test]
fn district_fallback_transition_is_marked_unimplemented() {
    for transition in TRANSITIONS {
        if transition == TransitionKind::Gradient {
            assert!(transition.implemented());
            assert_eq!(transition_tooltip(transition), None);
        } else {
            assert!(!transition.implemented(), "{transition:?}");
            assert_eq!(
                transition_tooltip(transition),
                Some(TRANSITION_FALLBACK_TIP),
                "{transition:?}"
            );
        }
    }
    assert!(TRANSITION_FALLBACK_TIP.contains("Dégradé"));
}

// --- Gestes : mode manuel, arrêts, épingles ---

#[derive(Resource, Default)]
struct Pending(Vec<Gesture>);

#[derive(Resource, Default)]
struct Changes(Vec<(bool, bool)>);

fn apply_system(
    mut pending: ResMut<Pending>,
    automation: Option<ResMut<WorldAutomation>>,
    active: Option<ResMut<ActiveEvents>>,
    settings: Option<ResMut<Settings>>,
) {
    let (mut automation, mut active, mut settings) = (automation, active, settings);
    apply_gestures(&mut pending.0, &mut automation, &mut active, &mut settings);
}

fn watch(automation: Res<WorldAutomation>, settings: Res<Settings>, mut seen: ResMut<Changes>) {
    seen.0.push((automation.is_changed(), settings.is_changed()));
}

fn gesture_app() -> App {
    let mut app = App::new();
    app.init_resource::<Pending>()
        .init_resource::<Changes>()
        .init_resource::<WorldAutomation>()
        .init_resource::<ActiveEvents>()
        .insert_resource(Settings::new(AppSettings::default(), "inutilise.ron".into()))
        .add_systems(Update, (apply_system, watch).chain());
    app.update();
    app
}

fn send(app: &mut App, gestures: impl IntoIterator<Item = Gesture>) {
    app.world_mut().resource_mut::<Pending>().0.extend(gestures);
    app.update();
}

#[test]
fn manual_toggle_writes_world_automation() {
    let mut app = gesture_app();
    send(&mut app, [Gesture::SetManual(true)]);
    assert!(app.world().resource::<WorldAutomation>().manual);
    assert!(
        app.world().resource::<Pending>().0.is_empty(),
        "les gestes sont consommés"
    );
    // Redemander la même valeur, ou ne rien faire, ne réveille personne.
    send(&mut app, [Gesture::SetManual(true)]);
    send(&mut app, []);
    let seen = &app.world().resource::<Changes>().0;
    assert_eq!(seen[1..], [(true, false), (false, false), (false, false)]);
    send(&mut app, [Gesture::SetManual(false)]);
    assert!(!app.world().resource::<WorldAutomation>().manual);
}

#[test]
fn stop_all_clears_active_events() {
    let mut app = gesture_app();
    let (keep, gone) = {
        let mut active = app.world_mut().resource_mut::<ActiveEvents>();
        active.begin(running("storm", EventCategory::Car, 0.0, None));
        let keep = active.begin(running("tower", EventCategory::Building, 0.0, None));
        let gone = active.begin(running("adship", EventCategory::Sign, 0.0, None));
        (keep, gone)
    };
    send(&mut app, [Gesture::Stop(gone), Gesture::StopColumn(EventCategory::Car)]);
    let left: Vec<_> = app.world().resource::<ActiveEvents>().iter().map(|e| e.id).collect();
    assert_eq!(left, [keep]);
    send(&mut app, [Gesture::StopAll]);
    assert!(app.world().resource::<ActiveEvents>().is_empty());
}

#[test]
fn pinning_writes_the_settings_only_on_a_gesture() {
    let mut app = gesture_app();
    send(&mut app, [Gesture::TogglePin("storm")]);
    send(&mut app, []);
    let pinned = &app.world().resource::<Settings>().value.operator.pinned_events;
    assert_eq!(pinned, &BTreeSet::from(["storm".to_owned()]));
    let seen = &app.world().resource::<Changes>().0;
    assert_eq!(seen[1..], [(false, true), (false, false)]);
    send(&mut app, [Gesture::TogglePin("storm")]);
    assert!(
        app.world()
            .resource::<Settings>()
            .value
            .operator
            .pinned_events
            .is_empty()
    );
}

// --- Barre du haut ---

#[test]
fn the_load_gauge_is_full_at_eight_events() {
    assert!(load_ratio(0).abs() < f32::EPSILON);
    assert!((load_ratio(4) - 0.5).abs() < 1e-6);
    assert!((load_ratio(50) - 1.0).abs() < 1e-6);
}

#[test]
fn slash_focuses_the_search_unless_typing() {
    let slash = || vec![egui::Event::Text("/".to_owned())];
    with_events(slash(), |input| assert!(wants_search_focus(input, false)));
    with_events(slash(), |input| assert!(!wants_search_focus(input, true)));
    with_events(vec![egui::Event::Text("?".to_owned())], |input| {
        assert!(!wants_search_focus(input, false));
    });
}

#[test]
fn slash_is_free_in_the_default_keyboard() {
    let profile = cf_control::ControlProfile::sillage_keyboard();
    let text = format!("{profile:?}");
    assert!(
        !text.contains("'/'") && !text.contains("Slash"),
        "« / » est pris : {text}"
    );
}

// --- Mise en page ---

#[test]
fn the_grid_has_four_equal_columns_split_two_thirds_one_third() {
    let rect = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(830.0, 500.0));
    let columns = split_columns(rect, 10.0);
    for column in &columns {
        assert!((column.width() - 200.0).abs() < 1e-3, "{column:?}");
        assert!((column.height() - 500.0).abs() < 1e-3);
    }
    assert!((columns[3].max.x - 830.0).abs() < 1e-3);
    let parts = split_column(columns[0], 20.0, 20.0);
    assert!((parts.header.height() - 20.0).abs() < 1e-3);
    assert!((parts.lane_header.height() - 20.0).abs() < 1e-3);
    assert!(
        (parts.chips.height() - 2.0 * parts.lane.height()).abs() < 1e-3,
        "{parts:?}"
    );
    assert!((parts.lane.max.y - 500.0).abs() < 1e-3);
    // Une colonne trop basse ne donne jamais de hauteur négative.
    let tiny = split_column(
        egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 10.0)),
        20.0,
        20.0,
    );
    assert!(tiny.chips.height() >= 0.0 && tiny.lane.height() >= 0.0);
}

// --- Dock ---

#[test]
fn only_existing_entry_points_are_wired_in_the_dock() {
    assert_eq!(camera_actions(), [Action::SkipShot, Action::ShowDj]);
    let wired: Vec<_> = PadMove::ALL.into_iter().filter_map(PadMove::action).collect();
    assert_eq!(wired, [Action::Ascension], "le pilote n'a aucune autre entrée manuelle");
    assert_eq!(PadMove::ALL.len(), 10);
}

// --- Dessin (sans fenêtre) ---

#[test]
fn the_tab_draws_headless_and_a_frame_without_click_does_nothing() {
    let catalog = real_catalog();
    let mut active = ActiveEvents::default();
    active.begin(running("storm", EventCategory::Car, 0.0, Some(10.0)));
    let mut state = DebugState::default();
    let mut orders = Vec::new();
    let ctx = egui::Context::default();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0))),
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| {
            let mut view = DebugView {
                state: &mut state,
                catalog: Some(&catalog),
                active: Some(&active),
                manual: Some(false),
                now_s: 4.0,
                settings: None,
                district: None,
                library: None,
                cars: None,
                tuning: None,
                orders: &mut orders,
            };
            draw(ui, &mut view);
        })
        .drop_without_applying_deltas();
    }
    assert!(orders.is_empty(), "aucun clic, aucun ordre");
    assert!(state.gestures.is_empty(), "aucun clic, aucun geste");
}

/// Une image egui avec ces événements ; `check` lit son `InputState`.
fn with_events(events: Vec<egui::Event>, check: impl FnOnce(&egui::InputState)) {
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
