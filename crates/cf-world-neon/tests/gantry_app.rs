//! Le portique dans une vraie `App` Bevy, sans rendu : créé une fois à `PostStartup`, jamais masqué (règle 8), posé devant
//! la voiture quand l'étape commence, enterré hors passage, une seule voiture a le focus de streaming.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, CorePlugin, PalettePlugin, SheetSlot, ShowSheet, StreamFocus, WorldPlugin};
use cf_world_neon::NeonWorld;
use cf_world_neon::djsign::{DjSignGlyphs, DjSignPlugin};
use cf_world_neon::gantry::{GantryMarker, GantryPlugin, GantryState, pose};
use cf_world_neon::run::{RunSequencer, RunStatus};

const FRAME: Duration = Duration::from_micros(16_667);

fn sheet(djs: &[&str]) -> ShowSheet {
    ShowSheet {
        event_name: "Démesure".to_owned(),
        dj_sign: true,
        slots: djs
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

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        PalettePlugin,
        WorldPlugin::new(NeonWorld::default()),
        DjSignPlugin::always(),
        GantryPlugin,
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
    .insert_resource(sheet(&["KRMN", "Nova Kaito"]))
    .insert_resource(RunStatus(RunSequencer::new().update(None, 0.0, false)));
    app.world_mut().spawn((StreamFocus, AbsolutePosition::default()));
    app
}

fn set_gantry(app: &mut App, stage: f32) {
    app.world_mut().resource_mut::<RunStatus>().0.stages.gantry = stage;
}

fn gantry_root(app: &mut App) -> (bevy::math::DVec3, Vec3) {
    let mut q = app
        .world_mut()
        .query_filtered::<(&AbsolutePosition, &Transform), With<GantryMarker>>();
    let (position, transform) = q.single(app.world()).expect("la racine du portique");
    (position.0, transform.scale)
}

#[test]
fn the_gantry_exists_from_startup_and_is_buried_and_tiny_while_absent_but_never_hidden() {
    let mut app = app();
    app.update();
    app.update();
    assert!(app.world().resource::<DjSignGlyphs>().cells.contains_key(&'K'));
    let (position, scale) = gantry_root(&mut app);
    assert!(position.y < -f64::from(pose::BEAM_H_M), "enterré : {}", position.y);
    assert!(scale.max_element() <= 1e-3 && scale.min_element() > 0.0, "{scale:?}");
    let mut q = app.world_mut().query_filtered::<&Visibility, With<GantryMarker>>();
    assert_ne!(*q.single(app.world()).expect("racine"), Visibility::Hidden);
    assert!(app.world().resource::<GantryState>().placed().is_none());
}

#[test]
fn the_gantry_is_placed_ahead_of_the_car_when_the_stage_starts() {
    let mut app = app();
    app.update();
    {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut p in cars.iter_mut(world) {
            p.0.z = -500.0;
            p.0.x = 3.0;
        }
    }
    set_gantry(&mut app, 0.5);
    app.update();
    let (position, scale) = gantry_root(&mut app);
    assert!((position.z - (-500.0 - pose::AHEAD_M)).abs() < 1e-6 && (position.x - 3.0).abs() < 1e-6);
    assert!(position.y.abs() < 1e-6, "sorti de terre : {}", position.y);
    assert_eq!(scale, Vec3::ONE);

    // Il reste où il est quand la voiture avance, puis redescend.
    {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut p in cars.iter_mut(world) {
            p.0.z = -600.0;
        }
    }
    app.update();
    assert!((gantry_root(&mut app).0.z - (-500.0 - pose::AHEAD_M)).abs() < 1e-6);
    set_gantry(&mut app, 1.0);
    app.update();
    let (position, scale) = gantry_root(&mut app);
    assert!(position.y < 0.0 && scale.max_element() <= 1e-3);
    assert!(app.world().resource::<GantryState>().placed().is_none());
}

#[test]
fn the_gantry_writes_the_name_of_the_dj_who_arrives_during_a_handover() {
    let mut app = app();
    app.update();
    app.world_mut().resource_mut::<RunStatus>().0.handover_slot = Some(1);
    set_gantry(&mut app, 0.5);
    app.world_mut().resource_mut::<RunStatus>().0.stages.sign = 1.0;
    for _ in 0..900 {
        app.update();
    }
    let lit = app.world().resource::<GantryState>().lit_letters();
    assert_eq!("Nova Kaito".chars().filter(|c| !c.is_whitespace()).count(), lit);
}

#[test]
fn only_one_entity_holds_the_streaming_focus() {
    let mut app = app();
    set_gantry(&mut app, 0.5);
    for _ in 0..5 {
        app.update();
    }
    let mut q = app.world_mut().query_filtered::<Entity, With<StreamFocus>>();
    assert_eq!(q.iter(app.world()).count(), 1);
}
