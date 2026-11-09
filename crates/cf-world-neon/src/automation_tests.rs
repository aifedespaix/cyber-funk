//! Le mode manuel du monde dans de vraies `App` Bevy minimales, sans rendu : chaque directeur qui lance des événements de
//! lui-même se tait quand `WorldAutomation::manual` est vrai, obéit toujours à un déclenchement explicite, et publie ses
//! événements dans `ActiveEvents` le temps qu'ils durent.
//!
//! # Inventaire des directeurs de `cf-world-neon`
//!
//! | directeur | lance de lui-même | catégorie | coupé en manuel ? |
//! | --- | --- | --- | --- |
//! | `traffic` (`traffic/director.rs`, `scheduler.rs`) | un événement du catalogue toutes les 15 à 34 s (poursuite, convoi, orage, méga-tour…) | `Car` ; `Building` pour la méga-tour, la panne et la pulsation de la ville ; `Sign` pour le vaisseau publicitaire et le titre holographique | **oui** (compte à rebours) ; `Action::Trigger` et l'ordre « événement au hasard » passent |
//! | `landmark` (`traffic/grand/landmark.rs`, `landmarks/catalog.rs`) | l'immeuble-vitrine, tiré par le trafic (`EventName::Landmark`) | `Building` | **oui** (même compte à rebours) ; `landmark:<nom>` passe |
//! | `stunts` (`stunts/director.rs`) | une cascade toutes les 16 à 34 s | `Car` | **oui** (tirage) ; l'ordre « Ascension » et la cascade du drop préparé par le spectacle passent |
//! | `djsign` (`djsign/director.rs`) | le retour périodique de l'enseigne du DJ (10 à 15 min) | `Sign` | **oui** (retour périodique) ; pré-intro, changement de DJ et ordre « afficher le DJ » passent |
//! | `districts` (`districts/systems.rs`, `state.rs`) | le changement de quartier décidé par le gouverneur | `District` | **oui** (décision du gouverneur) ; `district:<type>` (`DistrictOrder::Force`) passe |
//! | trafic de fond (`air_lanes`, `ground`) | véhicules des couloirs, voitures au sol | — | non : la vie continue de la ville, pas un événement |
//! | enseignes de la route (`signs/road.rs`, `ads`, `show_signs`, `ticker`) | un panneau par emplacement, tous les 105 m | — | non : du décor posé le long de la route, comme les immeubles ; rien au catalogue |
//! | façades (`facade/`, tempéraments) | allumage, vagues, étincelles | — | non : la respiration des immeubles |
//! | ciel (`sky/`), étoiles filantes | étoiles filantes toutes les quelques secondes | — | non : ambiance du ciel, rien au catalogue |
//! | météo (`weather/`) | averses | — | non : ambiance ; l'orage-événement est celui du trafic (`storm`) |
//! | astres (`meteors/`) | chutes à la cadence `meteor_rate` du look, pluies sur les drops | — | non : ambiance du ciel réglée par le look, ni catalogue ni déclencheur ; voir le rapport |
//! | projecteurs (`beams/`) | balayage des façades | — | non : décor |
//! | paquet du drop (`dropfx/`) | ondes et gerbe sur `drop_now` | — | non : joué par le spectacle |
//! | pré-intro (`intro/`) | construction de la ville au niveau 0 | — | non : séquence du spectacle |

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use bevy::input::keyboard::KeyboardInput;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::WindowFocused;
use cf_analysis::Section;
use cf_control::{Action, TimedAction, TriggerId};
use cf_render::{
    AbsolutePosition, ActiveEvents, ActiveEventsPlugin, AudioFeatures, ControlAction, ControllerLibrary,
    ControlsConfig, ControlsPlugin, CorePlugin, CullingPlugin, CurrentShow, DistrictState, EventCategory, EventOrigin,
    MidiChoice, PalettePlugin, SheetSlot, ShowPlugin, ShowSheet, StreamFocus, WorldAutomation, WorldPlugin,
};
use cf_show::director::ShowDirector;
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use rand_pcg::Pcg32;

use crate::automation::{AUTOMATIC_DIRECTORS, may_spawn};
use crate::djsign::{DjSignPlugin, DjSignRes};
use crate::stunts::{
    StuntAlign, StuntAppExt, StuntCtx, StuntEnv, StuntKind, StuntPose, StuntRegistry, StuntRun, StuntScript, StuntTier,
    StuntWorld, StuntsPlugin,
};
use crate::traffic::{EventName, EventRegistry, TrafficDirector, TrafficPlugin};
use crate::{
    DistrictsPlugin, FacadeLook, FacadePlugin, LandmarksPlugin, NeonWorld, PilotPlugin, PilotSettings, RoadLook,
};

/// Les directeurs couverts par ce fichier : chacun a ses trois tests (`<directeur>_spawns_nothing_spontaneously_in_manual_mode`,
/// `<directeur>_still_obeys_explicit_trigger_in_manual_mode`, `<directeur>_publishes_and_clears_active_event`).
const COVERED: &[&str] = &["traffic", "landmark", "stunts", "djsign", "districts"];

/// Une image de 0,1 s : les directeurs plafonnent leur pas à 0,1 s, rien n'est sauté.
const FRAME_S: f64 = 0.1;
/// Vitesse de la voiture, m/s.
const SPEED: f64 = 60.0;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn demo_show() -> LoadedShow {
    let looks = LookLibrary::load_dir(&assets().join("looks")).expect("looks livrés");
    LoadedShow::load(&assets().join("shows/demo.ron"), &looks).expect("show de démonstration")
}

/// Le socle : temps manuel, registre des événements actifs, bascule du mode manuel. `show` : le spectacle de
/// démonstration et les contrôles (le chemin de `Action::Trigger`).
fn base(show: bool, manual: bool) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, CorePlugin))
        .add_plugins(ActiveEventsPlugin)
        .init_asset::<Mesh>()
        .init_resource::<AudioFeatures>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<FacadeLook>()
        .add_message::<KeyboardInput>()
        .add_message::<WindowFocused>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(FRAME_S)))
        .insert_resource(WorldAutomation { manual });
    if show {
        app.add_plugins((
            ControlsPlugin::new(ControlsConfig {
                user_profile_path: None,
                midi: MidiChoice::Off,
                library: ControllerLibrary::new(),
            }),
            ShowPlugin::new(demo_show()),
        ));
    }
    app
}

/// Le trafic (et les immeubles-vitrines), la voiture à 400 km du départ.
fn traffic_app(show: bool, manual: bool) -> App {
    let mut app = base(show, manual);
    app.add_plugins((
        CullingPlugin::default(),
        TrafficPlugin {
            assets_dir: assets(),
            ..TrafficPlugin::default()
        },
        LandmarksPlugin::default(),
    ));
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    app.update();
    app
}

/// Une image : la voiture avance, l'audio est un morceau calme à 120 BPM (un kick toutes les 5 images).
fn frame(app: &mut App, i: usize) {
    {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut position in cars.iter_mut(world) {
            position.0.z -= SPEED * FRAME_S;
        }
    }
    {
        let mut audio = app.world_mut().resource_mut::<AudioFeatures>();
        audio.0.live = true;
        audio.0.time_s = i as f64 * FRAME_S;
        audio.0.section = Section::Calm;
        audio.0.section_changed = false;
        audio.0.intensity = 0.2;
        audio.0.loud = 0.5;
        audio.0.bass = 0.4;
        audio.0.mid = 0.3;
        audio.0.high = 0.3;
        audio.0.kick = i.is_multiple_of(5);
        audio.0.count = (i / 5) as u32;
        audio.0.beat = i as f64 / 5.0;
        audio.0.bpm = Some(120.0);
    }
    app.update();
}

fn frames(seconds: f64) -> usize {
    (seconds / FRAME_S) as usize
}

fn trigger(app: &mut App, slug: &str) {
    let at = app.world().resource::<Time>().elapsed_secs_f64();
    app.world_mut().write_message(ControlAction(TimedAction {
        time_s: at,
        action: Action::Trigger(TriggerId::new(slug)),
    }));
}

fn listed(app: &App) -> Vec<(&'static str, EventCategory, EventOrigin)> {
    app.world()
        .resource::<ActiveEvents>()
        .iter()
        .map(|e| (e.event_id, e.category, e.origin))
        .collect()
}

/// Joue jusqu'à ce que `done` soit vrai ; rend l'image, ou `None` passé `seconds`.
fn until(app: &mut App, from: usize, seconds: f64, mut done: impl FnMut(&mut App) -> bool) -> Option<usize> {
    (from..from + frames(seconds)).find(|&i| {
        frame(app, i);
        done(app)
    })
}

// ---------------------------------------------------------------------------------------------------------------------
// Garde et helper.

#[test]
fn every_known_director_is_covered() {
    let mut covered = COVERED.to_vec();
    let mut known = AUTOMATIC_DIRECTORS.to_vec();
    covered.sort_unstable();
    known.sort_unstable();
    assert_eq!(
        covered, known,
        "un directeur de AUTOMATIC_DIRECTORS n'a pas ses tests de mode manuel (ou l'inverse)"
    );
}

#[test]
fn may_spawn_follows_the_switch_and_defaults_to_auto() {
    assert!(may_spawn(None));
    assert!(may_spawn(Some(&WorldAutomation { manual: false })));
    assert!(!may_spawn(Some(&WorldAutomation { manual: true })));
}

// ---------------------------------------------------------------------------------------------------------------------
// Trafic.

#[test]
fn traffic_spawns_nothing_spontaneously_in_manual_mode() {
    // En automatique, le premier événement part en moins de 20 s (compte à rebours de 8 s, puis début de mesure) : 90 s
    // laissent largement le temps à trois tirages.
    let mut app = traffic_app(false, true);
    for i in 0..frames(90.0) {
        frame(&mut app, i);
        assert_eq!(app.world().resource::<TrafficDirector>().active_count(), 0, "image {i}");
        assert!(listed(&app).is_empty(), "image {i} : {:?}", listed(&app));
    }
}

#[test]
fn traffic_still_obeys_explicit_trigger_in_manual_mode() {
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "chase");
    let seen = until(&mut app, 5, 5.0, |app| !listed(app).is_empty()).expect("la poursuite ne part pas");
    assert_eq!(
        listed(&app),
        [("chase", EventCategory::Car, EventOrigin::Manual)],
        "image {seen}"
    );
    let names: Vec<_> = app.world().resource::<TrafficDirector>().active_names().collect();
    assert_eq!(names, [EventName::Chase]);
}

#[test]
fn traffic_publishes_and_clears_active_event() {
    // Automatique, sans spectacle : le registre suit exactement les événements du directeur, avec l'origine `Auto`.
    let mut app = traffic_app(false, false);
    let mut seen_auto = false;
    for i in 0..frames(240.0) {
        frame(&mut app, i);
        let director = app.world().resource::<TrafficDirector>();
        let mut running: Vec<&str> = director.active_names().map(EventName::as_str).collect();
        let mut published: Vec<&str> = listed(&app).iter().map(|e| e.0).collect();
        running.sort_unstable();
        published.sort_unstable();
        assert_eq!(published, running, "image {i}");
        seen_auto |= listed(&app).iter().any(|e| e.2 == EventOrigin::Auto);
    }
    assert!(seen_auto, "aucun événement spontané en 240 s");

    // Déclenché à la main : publié, puis retiré à sa fin.
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "chase");
    let seen = until(&mut app, 5, 5.0, |app| !listed(app).is_empty()).expect("la poursuite ne part pas");
    let ended = until(&mut app, seen + 1, 240.0, |app| {
        app.world().resource::<TrafficDirector>().active_count() == 0
    })
    .expect("la poursuite ne finit pas");
    assert!(listed(&app).is_empty(), "image {ended} : {:?}", listed(&app));
}

#[test]
fn refused_trigger_is_not_listed_as_active() {
    // Le registre du trafic est vidé : le déclencheur est pris, mais le monde n'a rien à lancer (refus). Rien n'est écrit.
    let mut app = traffic_app(true, true);
    app.insert_resource(EventRegistry::default());
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "chase");
    for i in 5..frames(10.0) {
        frame(&mut app, i);
        assert!(listed(&app).is_empty(), "image {i} : {:?}", listed(&app));
    }
    assert_eq!(app.world().resource::<TrafficDirector>().active_count(), 0);
    let pending = app
        .world()
        .resource::<cf_render::ShowRuntime>()
        .director()
        .state()
        .orders
        .trigger_pending()
        .is_some();
    assert!(!pending, "le déclencheur a bien été pris");
    assert_eq!(
        journal(&app),
        [(
            "chase".to_owned(),
            cf_render::JournalState::Refused(cf_render::RefusalReason::NotBuildable)
        )],
        "le refus doit être dit, pas avalé"
    );
}

/// Les lignes du journal : identifiant et état.
fn journal(app: &App) -> Vec<(String, cf_render::JournalState)> {
    app.world()
        .resource::<cf_render::EventJournal>()
        .iter()
        .map(|e| (e.event_id.clone(), e.state))
        .collect()
}

#[test]
fn two_compatible_triggers_in_the_same_frame_both_start() {
    // Avant : le directeur ne retenait qu'un nom forcé, le second écrasait le premier.
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "chase");
    trigger(&mut app, "storm");
    until(&mut app, 5, 6.0, |app| listed(app).len() == 2).expect("les deux événements ne partent pas");
    let mut ids: Vec<_> = listed(&app).iter().map(|e| e.0).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["chase", "storm"]);
}

#[test]
fn an_unknown_trigger_does_not_block_the_next_one() {
    // Avant : seul le plus ancien de la file était regardé ; un identifiant inconnu bloquait tout pendant 3 s.
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "n-importe-quoi");
    trigger(&mut app, "chase");
    until(&mut app, 5, 1.0, |app| !listed(app).is_empty()).expect("la poursuite est restée derrière l'inconnu");
}

#[test]
fn a_forced_structure_is_refused_with_its_reason_while_another_holds_the_road() {
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "landmark:arcade");
    let seen = until(&mut app, 5, 5.0, |app| !listed(app).is_empty()).expect("l'immeuble ne part pas");
    trigger(&mut app, "tower");
    for i in seen + 1..seen + 1 + frames(1.0) {
        frame(&mut app, i);
    }
    assert_eq!(
        listed(&app),
        [("landmark", EventCategory::Building, EventOrigin::Manual)],
        "la tour ne doit pas se superposer à l'immeuble"
    );
    let tower = journal(&app).into_iter().find(|(id, _)| id == "tower");
    assert_eq!(
        tower.map(|(_, state)| state),
        Some(cf_render::JournalState::Refused(
            cf_render::RefusalReason::StructureBusy
        )),
        "{:?}",
        journal(&app)
    );
}

#[test]
fn a_started_trigger_is_journaled_as_started() {
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "chase");
    until(&mut app, 5, 5.0, |app| !listed(app).is_empty()).expect("la poursuite ne part pas");
    assert_eq!(journal(&app), [("chase".to_owned(), cf_render::JournalState::Started)]);
}

#[test]
fn a_trigger_nobody_recognises_ends_up_unclaimed() {
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "n-importe-quoi");
    for i in 5..5 + frames(5.0) {
        frame(&mut app, i);
    }
    assert_eq!(
        journal(&app),
        [(
            "n-importe-quoi".to_owned(),
            cf_render::JournalState::Refused(cf_render::RefusalReason::Unclaimed)
        )]
    );
}

// ---------------------------------------------------------------------------------------------------------------------
// Immeubles-vitrines.

#[test]
fn landmark_spawns_nothing_spontaneously_in_manual_mode() {
    // Seul l'immeuble-vitrine est inscrit : en automatique il serait tiré au premier compte à rebours.
    let mut app = traffic_app(false, true);
    let mut registry = EventRegistry::default();
    for (name, build) in crate::traffic::grand::grand_factories(None) {
        if name == EventName::Landmark {
            registry.register(name, build);
        }
    }
    app.insert_resource(registry);
    for i in 0..frames(90.0) {
        frame(&mut app, i);
        assert_eq!(app.world().resource::<TrafficDirector>().active_count(), 0, "image {i}");
        assert!(listed(&app).is_empty(), "image {i} : {:?}", listed(&app));
    }
}

#[test]
fn landmark_still_obeys_explicit_trigger_in_manual_mode() {
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "landmark:arcade");
    until(&mut app, 5, 5.0, |app| !listed(app).is_empty()).expect("l'immeuble ne part pas");
    assert_eq!(
        listed(&app),
        [("landmark", EventCategory::Building, EventOrigin::Manual)]
    );
}

#[test]
fn landmark_publishes_and_clears_active_event() {
    let mut app = traffic_app(true, true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "landmark:arcade");
    let seen = until(&mut app, 5, 5.0, |app| !listed(app).is_empty()).expect("l'immeuble ne part pas");
    let ended = until(&mut app, seen + 1, 300.0, |app| {
        app.world().resource::<TrafficDirector>().active_count() == 0
    })
    .expect("l'immeuble ne finit pas");
    assert!(listed(&app).is_empty(), "image {ended} : {:?}", listed(&app));
}

// ---------------------------------------------------------------------------------------------------------------------
// Cascades.

/// Un saut de 5 s, toujours possible, à la place du slalom (le registre ne contient que lui).
struct Hop(StuntKind);

struct HopRun;

impl StuntRun for HopRun {
    fn duration(&self) -> f64 {
        5.0
    }
    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        out.x = ctx.env.car_x;
        out.altitude = 2.0;
        t < 5.0
    }
    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}

impl StuntScript for Hop {
    fn kind(&self) -> StuntKind {
        self.0
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Common
    }
    fn weight(&self) -> f64 {
        1.0
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, _env: &StuntEnv<'_>, _world: &dyn StuntWorld) -> bool {
        true
    }
    fn begin(&self, _ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        Box::new(HopRun)
    }
}

/// Le pilote, le trafic et les cascades (registre réduit au faux saut), sans spectacle : tout est permis sauf l'Ascension.
fn stunts_app(manual: bool) -> App {
    let mut app = base(false, manual);
    app.add_plugins((
        CullingPlugin::default(),
        PilotPlugin {
            settings: PilotSettings::default(),
        },
        TrafficPlugin {
            assets_dir: assets(),
            ..TrafficPlugin::default()
        },
        StuntsPlugin::default(),
    ))
    .insert_resource(StuntRegistry::default())
    .register_stunt(Hop(StuntKind::Slalom));
    // Le trafic seul n'a pas d'événement : la cascade est la seule chose qui se publie.
    app.insert_resource(EventRegistry::default());
    app.update();
    app
}

fn stunt_running(app: &App) -> bool {
    app.world().resource::<crate::StuntDirector>().active()
}

#[test]
fn stunts_spawns_nothing_spontaneously_in_manual_mode() {
    // En automatique, la première cascade part entre 5 et 10 s.
    let mut app = stunts_app(true);
    for i in 0..frames(90.0) {
        frame(&mut app, i);
        assert!(!stunt_running(&app), "image {i}");
        assert!(listed(&app).is_empty(), "image {i} : {:?}", listed(&app));
    }
}

#[test]
fn stunts_publishes_and_clears_active_event() {
    let mut app = stunts_app(false);
    let seen = until(&mut app, 0, 30.0, |app| !listed(app).is_empty()).expect("aucune cascade en 30 s");
    assert_eq!(listed(&app), [("stunt:slalom", EventCategory::Car, EventOrigin::Auto)]);
    let event = app
        .world()
        .resource::<ActiveEvents>()
        .iter()
        .next()
        .cloned()
        .expect("listée");
    assert_eq!(event.duration_s, Some(5.0));
    let ended = until(&mut app, seen + 1, 20.0, |app| !stunt_running(app)).expect("la cascade ne finit pas");
    frame(&mut app, ended + 1);
    assert!(listed(&app).is_empty(), "image {ended} : {:?}", listed(&app));
}

/// L'ordre « Ascension » de l'opérateur, dans une application minimale, demanderait un spectacle dont le niveau ouvre
/// les cascades à l'instant du test : on vérifie la décision sur le directeur lui-même (pur), mode manuel compris.
#[test]
fn stunts_still_obeys_explicit_trigger_in_manual_mode() {
    use cf_show::model::StuntTier as ShowStuntTier;

    use crate::stunts::{StuntFrame, StuntLink, StuntShow};
    use crate::traffic::{Craft, CraftId, TrafficBeat};

    struct Empty;
    impl StuntWorld for Empty {
        fn spawn(&mut self, _variant: u8, _at: DVec3, _scale: f32, _tint: i64) -> Option<CraftId> {
            None
        }
        fn release(&mut self, _id: CraftId) {}
        fn craft_mut(&mut self, _id: CraftId) -> Option<&mut Craft> {
            None
        }
        fn roof_at(&self, _x: f64, _z: f64) -> Option<f32> {
            None
        }
        fn facade(&self, _side: i8, _z: f64, _depth: f64) -> Option<f32> {
            None
        }
        fn overhead(&self, _out: &mut Vec<crate::pilot::flight::Obstruction>, _car_z: f64) {}
        fn bridge_ahead(&self, _car_z: f64, _distance: f64, _margin: f64) -> bool {
            false
        }
        fn busy(&self) -> bool {
            false
        }
        fn hold(&mut self, _on: bool) {}
    }
    struct Order(bool);
    impl StuntLink for Order {
        fn take_ascension(&mut self) -> bool {
            std::mem::take(&mut self.0)
        }
        fn freshness_weight(&self, _id: &str, _now: f64) -> f64 {
            1.0
        }
        fn freshness_mark(&mut self, _id: &str, _now: f64) {}
    }

    let mut gates = cf_show::gates::open_gates();
    gates.max_floor = 4;
    gates.stunt_tiers = ["common", "uncommon", "rare"]
        .into_iter()
        .map(|s| ShowStuntTier::new(s).unwrap())
        .collect();
    gates.ascension = false;
    let mut registry = StuntRegistry::default();
    registry.register(Box::new(Hop(StuntKind::Slalom)));
    registry.register(Box::new(Hop(StuntKind::Ascension)));
    let mut director = crate::StuntDirector::new(3);
    director.set_spontaneous(false);
    let mut link = Order(true);
    let mut started = Vec::new();
    for i in 0..600_u32 {
        let now = 100.0 + f64::from(i) / 60.0;
        let frame = StuntFrame {
            dt: 1.0 / 60.0,
            now,
            enabled: true,
            car_x: 0.0,
            car_z: -now * 60.0,
            altitude: 0.0,
            road_speed: 60.0,
            beat: TrafficBeat {
                kick: i % 30 == 0,
                count: i / 30,
                bpm: Some(120.0),
                ..TrafficBeat::SILENT
            },
            drop: false,
            intensity: 0.5,
            show: Some(StuntShow {
                gates: &gates,
                breathing: false,
                drop_now: false,
                planned: crate::stunts::Planned::None,
            }),
        };
        director.update(&frame, &registry, &mut Empty, &mut link);
        if let Some(view) = director.view()
            && started.last() != Some(&view.kind)
        {
            started.push(view.kind);
        }
    }
    assert_eq!(started, [StuntKind::Ascension], "seule l'Ascension demandée part");
    assert_eq!(director.last_origin(), EventOrigin::Manual);
}

// ---------------------------------------------------------------------------------------------------------------------
// Enseigne du DJ.

fn sheet() -> ShowSheet {
    ShowSheet {
        event_name: "Démesure".to_owned(),
        dj_sign: true,
        slots: ["KRMN", "Nova Kaito"]
            .iter()
            .map(|dj| SheetSlot {
                dj: (*dj).to_owned(),
                theme: None,
                logo: None,
                schedule: None,
                cues: Vec::new(),
            })
            .collect(),
    }
}

/// L'enseigne du DJ dans le monde, avec l'état du spectacle publié à la main (comme `djsign_app.rs`) au niveau 2.
fn djsign_app(manual: bool) -> App {
    let mut app = base(false, manual);
    let director = ShowDirector::<Pcg32>::new(&demo_show(), DropTitles::empty(), 1).expect("chef");
    let mut state = director.state().clone();
    state.level = 2;
    app.add_plugins((
        PalettePlugin,
        WorldPlugin::new(NeonWorld::default()),
        FacadePlugin,
        DjSignPlugin::always(),
    ))
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .init_resource::<RoadLook>()
    .insert_resource(sheet())
    .insert_resource(CurrentShow(state));
    app.world_mut().spawn((StreamFocus, AbsolutePosition::default()));
    app
}

fn dj_sign_listed(app: &App) -> Vec<EventOrigin> {
    listed(app).into_iter().map(|e| e.2).collect()
}

#[test]
fn djsign_spawns_nothing_spontaneously_in_manual_mode() {
    // Le premier DJ appelle l'enseigne à l'ouverture (spectacle : `Manual`) ; ensuite, en automatique, elle revient
    // seule entre 10 et 15 min. En 16 min de manuel, jamais d'enseigne `Auto`.
    let mut app = djsign_app(true);
    for i in 0..frames(16.0 * 60.0) {
        frame(&mut app, i);
        let spawns = app.world().resource::<DjSignRes>().0.spawns();
        assert!(spawns <= 1, "image {i} : {spawns} poses, l'enseigne est revenue seule");
        assert!(
            listed(&app).iter().all(|e| e.2 == EventOrigin::Manual),
            "image {i} : {:?}",
            listed(&app)
        );
    }
}

#[test]
fn djsign_still_obeys_explicit_trigger_in_manual_mode() {
    let mut app = djsign_app(true);
    // L'enseigne de l'ouverture passe et se range.
    let gone = until(&mut app, 0, 60.0, |app| {
        app.world().resource::<DjSignRes>().0.site().is_none() && listed(app).is_empty()
    });
    let mut i = gone.expect("l'enseigne d'ouverture ne se range pas") + 1;
    // L'ordre « afficher le DJ », tel que le chef du spectacle le publie.
    app.world_mut().resource_mut::<CurrentShow>().0.dj_show_seq += 1;
    frame(&mut app, i);
    i += 1;
    assert!(app.world().resource::<DjSignRes>().0.site().is_some(), "image {i}");
    assert_eq!(dj_sign_listed(&app), [EventOrigin::Manual]);
    assert_eq!(listed(&app)[0].0, "dj-sign");
    assert_eq!(listed(&app)[0].1, EventCategory::Sign);
}

#[test]
fn djsign_publishes_and_clears_active_event() {
    let mut app = djsign_app(false);
    // L'enseigne d'ouverture (changement de DJ : le spectacle) puis son rangement.
    let seen = until(&mut app, 0, 5.0, |app| !listed(app).is_empty()).expect("l'enseigne d'ouverture");
    assert_eq!(dj_sign_listed(&app), [EventOrigin::Manual]);
    let gone = until(&mut app, seen + 1, 60.0, |app| {
        app.world().resource::<DjSignRes>().0.site().is_none()
    })
    .expect("l'enseigne ne se range pas");
    assert!(listed(&app).is_empty(), "image {gone}");
    // Le retour périodique, de lui-même.
    until(&mut app, gone + 1, 16.0 * 60.0, |app| !listed(app).is_empty()).expect("pas de retour en 16 min");
    assert_eq!(dj_sign_listed(&app), [EventOrigin::Auto]);
}

// ---------------------------------------------------------------------------------------------------------------------
// Quartiers.

fn districts_app(manual: bool) -> App {
    let mut app = base(true, manual);
    app.insert_resource(NeonWorld::default())
        .add_plugins((CullingPlugin::default(), PalettePlugin, DistrictsPlugin));
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    app.update();
    app
}

fn district(app: &App) -> DistrictState {
    *app.world().resource::<DistrictState>()
}

#[test]
fn districts_spawns_nothing_spontaneously_in_manual_mode() {
    // En automatique, le calme ouvre un changement dès 150 s (au plus 480 s) : 600 s de manuel n'en voient aucun.
    let mut app = districts_app(true);
    let start = district(&app).current;
    for i in 0..frames(600.0) {
        frame(&mut app, i);
        let s = district(&app);
        assert_eq!(s.target, None, "image {i}");
        assert_eq!(s.current, start, "image {i}");
        assert!(listed(&app).is_empty(), "image {i} : {:?}", listed(&app));
    }
}

#[test]
fn districts_still_obeys_explicit_trigger_in_manual_mode() {
    let mut app = districts_app(true);
    for i in 0..5 {
        frame(&mut app, i);
    }
    trigger(&mut app, "district:suburb");
    until(&mut app, 5, 2.0, |app| district(app).target.is_some()).expect("le forçage ne part pas");
    assert_eq!(
        listed(&app),
        [("district:suburb", EventCategory::District, EventOrigin::Manual)]
    );
}

#[test]
fn districts_publishes_and_clears_active_event() {
    let mut app = districts_app(false);
    let at = until(&mut app, 0, 300.0, |app| district(app).target.is_some()).expect("aucun changement en 300 s");
    let events = listed(&app);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!((events[0].1, events[0].2), (EventCategory::District, EventOrigin::Auto));
    let done = until(&mut app, at + 1, 90.0, |app| district(app).target.is_none()).expect("transition sans fin");
    assert!(listed(&app).is_empty(), "image {done} : {:?}", listed(&app));
}
