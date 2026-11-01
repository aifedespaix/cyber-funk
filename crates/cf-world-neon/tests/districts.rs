//! Le moteur de quartiers dans une vraie `App` Bevy, sans rendu : le show de démonstration, les contrôles (chemin de
//! `Action::Trigger`), le monde (`NeonWorld` en ressource, sans streaming) et l'étalonnage. Le temps est manuel (10 i/s)
//! et l'horloge audio suit le temps simulé.

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
use cf_render::palette::MAX_WAIT;
use cf_render::{
    AbsolutePosition, AudioFeatures, ControlAction, ControllerLibrary, ControlsConfig, ControlsPlugin, CorePlugin,
    CullingPlugin, DistrictGradeState, DistrictOrder, DistrictState, MidiChoice, PalettePlugin, ShowPlugin,
    StreamFocus, World as _,
};
use cf_show::district::{
    DistrictId, DistrictKind, MAX_DWELL_S, MIN_DWELL_S, StayReason, TRANSITION_MAX_S, TransitionKind,
};
use cf_show::load::{LoadedShow, LookLibrary};
use cf_world_neon::scenery::LEAD_ROWS;
use cf_world_neon::traffic::{TrafficDirector, TrafficPlugin};
use cf_world_neon::{DistrictsPlugin, FacadeLook, NeonWorld};

const FRAME_S: f64 = 0.1;
/// Vitesse de la voiture, m/s.
const SPEED: f64 = 60.0;
const START_Z: f64 = -400_000.0;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn app(traffic: bool) -> App {
    let looks = LookLibrary::load_dir(&assets().join("looks")).expect("looks livrés");
    let show = LoadedShow::load(&assets().join("shows/demo.ron"), &looks).expect("show de démonstration");
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin, CorePlugin))
        .init_asset::<Mesh>()
        .init_resource::<AudioFeatures>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<FacadeLook>()
        .insert_resource(NeonWorld::default())
        .add_message::<KeyboardInput>()
        .add_message::<WindowFocused>()
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(FRAME_S)))
        .add_plugins((
            CullingPlugin::default(),
            ControlsPlugin::new(ControlsConfig {
                user_profile_path: None,
                midi: MidiChoice::Off,
                library: ControllerLibrary::new(),
            }),
            ShowPlugin::new(show),
            // L'étalonnage réel : `drive_palette` libère la cible sur un temps fort et avance le fondu.
            PalettePlugin,
            DistrictsPlugin,
        ));
    if traffic {
        app.add_plugins(TrafficPlugin {
            assets_dir: assets(),
            ..TrafficPlugin::default()
        });
    }
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, START_Z))));
    app.update();
    app
}

/// Une image de musique : la voiture avance, l'horloge audio suit, un kick toutes les 5 images.
fn frame(app: &mut App, i: usize, section: Section, intensity: f32) {
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
        audio.0.section = section;
        audio.0.section_changed = false;
        audio.0.intensity = intensity;
        audio.0.bass = 0.4;
        audio.0.high = 0.3;
        audio.0.kick = i.is_multiple_of(5);
        audio.0.count = (i / 5) as u32;
        audio.0.bpm = Some(120.0);
    }
    app.update();
}

fn state(app: &App) -> DistrictState {
    *app.world().resource::<DistrictState>()
}

fn neon() -> DistrictId {
    DistrictId {
        kind: DistrictKind::Neon,
        variant: 0,
    }
}

fn car_z(app: &mut App) -> f64 {
    let world = app.world_mut();
    let mut cars = world.query_filtered::<&AbsolutePosition, With<StreamFocus>>();
    cars.single(world).unwrap().0.z
}

/// Dernière rangée générée par le streaming pour la position actuelle de la voiture.
fn last_generated(app: &mut App) -> i64 {
    let z = car_z(app);
    let layout = app.world().resource::<NeonWorld>().layout();
    layout.chunk_at(-z) + i64::from(layout.ahead)
}

/// Rangée de la voiture.
fn car_row(app: &mut App) -> i64 {
    let z = car_z(app);
    app.world().resource::<NeonWorld>().layout().chunk_at(-z)
}

/// Début et fin (exclue) de la zone de dégradé vers `target` : le premier index du plan qui lui appartient, et la fin
/// du dernier dégradé planifié.
fn blend_zone(app: &App, target: DistrictId, from_row: i64) -> (i64, i64) {
    let world = app.world().resource::<NeonWorld>();
    let plan = world.scenery().districts();
    let start = (from_row..from_row + 200)
        .find(|&row| plan.district_at(row) == target)
        .expect("segment absent du plan");
    (start, world.scenery().last_planned_index())
}

fn grade_moving(app: &App) -> bool {
    let grade = app.world().resource::<DistrictGradeState>();
    grade.is_pending() || grade.is_fading()
}

fn trigger(app: &mut App, slug: &str) {
    let at = app.world().resource::<Time>().elapsed_secs_f64();
    app.world_mut().write_message(ControlAction(TimedAction {
        time_s: at,
        action: Action::Trigger(TriggerId::new(slug)),
    }));
}

/// Joue `seconds` de Calm à partir de l'image `from` et rend l'image où une cible apparaît (s'il y en a une).
fn until_target(app: &mut App, from: usize, seconds: f64) -> Option<usize> {
    let frames = (seconds / FRAME_S) as usize;
    (from..from + frames).find(|&i| {
        frame(app, i, Section::Calm, 0.2);
        state(app).target.is_some()
    })
}

#[test]
fn peak_ne_declenche_pas() {
    let mut app = app(false);
    for i in 0..(20.0 * 60.0 / FRAME_S) as usize {
        frame(&mut app, i, Section::Peak, 0.9);
        let s = state(&app);
        assert_eq!(s.current, neon(), "image {i}");
        assert_eq!(s.target, None, "image {i}");
    }
}

#[test]
fn calme_declenche_apres_le_temps_minimum() {
    let mut app = app(false);
    let at = until_target(&mut app, 0, 300.0).expect("aucun changement en 300 s de Calm");
    assert!(at as f64 * FRAME_S >= MIN_DWELL_S - FRAME_S, "changement à {at}");
    let target = state(&app).target.unwrap();
    assert_ne!(target.kind, DistrictKind::Neon);

    // Le segment commence au moins LEAD_ROWS rangées après la dernière rangée générée : celles-ci restent néon.
    let last = last_generated(&mut app);
    let world = app.world().resource::<NeonWorld>();
    assert!(world.scenery().districts_active());
    let plan = world.scenery().districts();
    for row in (last - 60)..(last + LEAD_ROWS) {
        assert_eq!(plan.district_at(row), neon(), "rangée {row} déjà générée");
    }
    assert_eq!(plan.district_at(world.scenery().last_planned_index()), target);

    // Approche (~20 s à 60 m/s), traversée de la zone, fondu : moins de 60 s en tout.
    for i in (at + 1)..(at + 600) {
        frame(&mut app, i, Section::Calm, 0.2);
        if state(&app).target.is_none() {
            break;
        }
    }
    let s = state(&app);
    assert_eq!(s.target, None);
    assert_eq!(s.current, target);
}

/// La couleur suit la voiture : rien ne bouge pendant l'approche (gouverneur maintenu en transition), l'étalonnage part
/// à l'entrée dans la zone de dégradé, et tout est fini au plus 10 s (plus l'attente du temps fort) après cette entrée,
/// une fois la zone passée.
#[test]
fn fin_de_transition_au_plus_10s() {
    let mut app = app(false);
    let at = until_target(&mut app, 0, 300.0).expect("aucun changement");
    let target = state(&app).target.unwrap();
    let from_row = car_row(&mut app);
    let (start, end) = blend_zone(&app, target, from_row);
    assert!(
        start > from_row + 40,
        "le segment doit être loin devant : {start} / {from_row}"
    );

    let mut progress = state(&app).progress;
    let mut entered_at = None;
    let mut done_at = None;
    for i in (at + 1)..(at + 900) {
        frame(&mut app, i, Section::Calm, 0.2);
        let s = state(&app);
        let row = car_row(&mut app);
        if row < start {
            // Approche : ni couleur, ni progrès, gouverneur bloqué en transition.
            assert!(!grade_moving(&app), "l'étalonnage part avant la zone (image {i})");
            assert!(s.progress.abs() < f32::EPSILON, "progrès en approche (image {i})");
            assert_eq!(s.last_reason, StayReason::Transitioning, "image {i}");
            assert_eq!(s.target, Some(target));
        } else if entered_at.is_none() {
            entered_at = Some(i);
            assert!(grade_moving(&app), "l'étalonnage ne part pas à l'entrée de la zone");
        }
        assert!(s.progress >= progress, "le progrès recule à l'image {i}");
        progress = s.progress;
        if s.target.is_none() {
            done_at = Some(i);
            assert!(row >= end, "fin avant que la voiture passe la zone : {row} < {end}");
            assert_eq!(s.current, target);
            break;
        }
        assert_eq!(s.current, neon(), "le quartier affiché change avant la fin (image {i})");
    }
    let entered_at = entered_at.expect("la voiture n'est jamais entrée dans la zone");
    let done_at = done_at.expect("transition jamais finie");
    assert!((progress - 1.0).abs() < f32::EPSILON);
    let duration = (done_at - entered_at) as f64 * FRAME_S;
    assert!(
        duration <= TRANSITION_MAX_S + f64::from(MAX_WAIT) + FRAME_S,
        "transition de {duration} s après l'entrée"
    );
    assert!(!grade_moving(&app), "l'étalonnage est posé à la fin");
}

#[test]
fn duree_de_transition_reglable() {
    let mut app = app(false);
    for i in 0..30 {
        frame(&mut app, i, Section::Groove, 0.5);
    }
    for (asked, kept) in [(50.0, 10.0), (0.2, 1.0), (f32::NAN, 1.0), (3.0, 3.0)] {
        app.world_mut().write_message(DistrictOrder::SetFadeSeconds(asked));
        frame(&mut app, 30, Section::Groove, 0.5);
        assert!(
            (state(&app).fade_s - kept).abs() < f32::EPSILON,
            "{asked} -> {}",
            state(&app).fade_s
        );
    }
    let to = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 3,
    };
    app.world_mut().write_message(DistrictOrder::Force {
        to,
        transition: TransitionKind::Gradient,
    });
    frame(&mut app, 31, Section::Groove, 0.5);
    let row = car_row(&mut app);
    let (start, end) = blend_zone(&app, to, row);
    // 60 m/s × 3 s = 180 m = 6,9 rangées : 7.
    assert_eq!(end - start, 7);
}

/// Groove sans changement de section : seule la fin d'un événement ouvre la fenêtre (avant le temps maximum).
#[test]
fn evenement_termine_ouvre_une_fenetre() {
    {
        let mut groove = app(true);
        let app = &mut groove;
        let mut counts = Vec::new();
        let mut changed_at = None;
        let frames = ((MAX_DWELL_S - 10.0) / FRAME_S) as usize;
        for i in 0..frames {
            if i == (MIN_DWELL_S / FRAME_S) as usize + 20 {
                trigger(app, "fish");
            }
            frame(app, i, Section::Groove, 0.5);
            counts.push(app.world().resource::<TrafficDirector>().active_count());
            if state(app).target.is_some() {
                changed_at = Some(i);
                break;
            }
        }
        let at = changed_at.expect("aucun changement sur fin d'événement");
        assert!(at as f64 * FRAME_S >= MIN_DWELL_S - FRAME_S);
        // Le tick de l'image `at` compare le trafic des deux images précédentes.
        assert!(
            counts[at - 1] < counts[at - 2],
            "changement sans fin d'événement à l'image {at}"
        );
    }

    // Sous les mêmes interdits : en Peak, une fin d'événement n'ouvre rien.
    let mut peak = app(true);
    let app = &mut peak;
    let mut ended = 0;
    let mut last = 0;
    for i in 0..((MIN_DWELL_S + 120.0) / FRAME_S) as usize {
        if i == (MIN_DWELL_S / FRAME_S) as usize + 20 {
            trigger(app, "fish");
        }
        frame(app, i, Section::Peak, 0.9);
        let count = app.world().resource::<TrafficDirector>().active_count();
        if i as f64 * FRAME_S > MIN_DWELL_S && count < last {
            ended += 1;
        }
        last = count;
        assert_eq!(state(app).target, None, "changement en Peak à l'image {i}");
    }
    assert!(ended > 0, "aucun événement n'a fini après le temps minimum");
}

#[test]
fn trigger_district_force() {
    let mut app = app(false);
    for i in 0..5 {
        frame(&mut app, i, Section::Peak, 0.9);
    }
    trigger(&mut app, "district:suburb");
    let mut forced = None;
    for i in 5..12 {
        frame(&mut app, i, Section::Peak, 0.9);
        if let Some(target) = state(&app).target {
            forced = Some(target);
            break;
        }
    }
    let target = forced.expect("le déclencheur n'a rien forcé");
    assert_eq!(target.kind, DistrictKind::Suburb);
    let s = state(&app);
    assert_eq!(s.transition, TransitionKind::Gradient);
    assert!(!s.fallback);
    let world = app.world().resource::<NeonWorld>();
    assert_eq!(
        world
            .scenery()
            .districts()
            .district_at(world.scenery().last_planned_index()),
        target
    );
}

#[test]
fn transition_non_implementee_retombe_sur_le_degrade() {
    let mut app = app(false);
    for i in 0..5 {
        frame(&mut app, i, Section::Groove, 0.5);
    }
    let to = DistrictId {
        kind: DistrictKind::Residential,
        variant: 1,
    };
    app.world_mut().write_message(DistrictOrder::Force {
        to,
        transition: TransitionKind::Bridge,
    });
    frame(&mut app, 5, Section::Groove, 0.5);
    let s = state(&app);
    assert_eq!(s.target, Some(to));
    assert_eq!(s.transition, TransitionKind::Bridge);
    assert!(s.fallback);
    // Le dégradé est posé dans le plan, au-delà des rangées générées.
    let last = last_generated(&mut app);
    let world = app.world().resource::<NeonWorld>();
    let plan = world.scenery().districts();
    assert_eq!(plan.district_at(last + LEAD_ROWS - 1), neon());
    assert_eq!(plan.district_at(world.scenery().last_planned_index()), to);
    assert!(world.scenery().last_planned_index() - (last + LEAD_ROWS) >= 4);
}

#[test]
fn gel_bloque_l_automatique() {
    let mut app = app(false);
    app.world_mut().write_message(DistrictOrder::SetFrozen(true));
    assert_eq!(until_target(&mut app, 0, 300.0), None);
    assert!(state(&app).frozen);
}

#[test]
fn reproductible() {
    let run = || {
        let mut app = app(false);
        for i in 0..(400.0 / FRAME_S) as usize {
            frame(&mut app, i, Section::Calm, 0.2);
        }
        state(&app).history
    };
    let first = run();
    assert!(first[1].is_some(), "au moins un changement : {first:?}");
    assert_eq!(first, run());
}
