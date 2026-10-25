//! Le trafic dans une vraie `App` Bevy, sans rendu : ses entités sont préallouées au démarrage et ne changent plus, la
//! cadence de simulation de chaque véhicule suit le culling de simulation (compteurs), le pilote reçoit ce que le trafic
//! a décidé, et les sujets sortent bien de la flotte.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, CorePlugin, CullingPlugin, CullingView, StreamFocus};
use cf_world_neon::PilotTraffic;
use cf_world_neon::traffic::{
    TrafficBody, TrafficDirector, TrafficEntities, TrafficPlugin, TrafficStats, TrafficTrail,
};

const FRAME: Duration = Duration::from_micros(16_667);

fn app(culling_view: bool) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        CullingPlugin::default(),
        TrafficPlugin {
            assets_dir: Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            ..TrafficPlugin::default()
        },
    ))
    .init_asset::<Mesh>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    // La voiture roule à 60 m/s vers −z, à 400 km du départ.
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    if culling_view {
        app.world_mut().spawn((CullingView, GlobalTransform::default()));
    }
    app
}

fn drive(app: &mut App, frames: usize) {
    for _ in 0..frames {
        let world = app.world_mut();
        let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
        for mut position in cars.iter_mut(world) {
            position.0.z -= 60.0 * FRAME.as_secs_f64();
        }
        app.update();
    }
}

fn entity_count(app: &mut App) -> usize {
    app.world_mut().query::<Entity>().iter(app.world()).count()
}

#[test]
fn les_entites_sont_preallouees_au_demarrage_et_ne_changent_plus() {
    let mut app = app(false);
    drive(&mut app, 30);
    let entities = app.world().resource::<TrafficEntities>().entity_count();
    // 212 véhicules (34×5 + 18 + 6×4), 220 traînées, les points lumineux et les murs, les deux couches de panneaux des
    // structures (plein et lumière).
    assert_eq!(entities, 212 + 220 + 2 + 2);
    let bodies = app.world_mut().query::<&TrafficBody>().iter(app.world()).count();
    let trails = app.world_mut().query::<&TrafficTrail>().iter(app.world()).count();
    assert_eq!((bodies, trails), (212, 220));

    let before = entity_count(&mut app);
    drive(&mut app, 60 * 120);
    assert_eq!(
        entity_count(&mut app),
        before,
        "aucun spawn ni despawn en régime établi"
    );
    let stats = *app.world().resource::<TrafficStats>();
    assert!(stats.fleet.active > 0, "le trafic de fond vit : {stats:?}");
    assert_eq!(stats.entities, entities);
}

#[test]
fn hors_champ_les_vehicules_ne_font_qu_un_pas_sur_huit() {
    // Sans caméra de culling : tout reste à pleine cadence. Avec : les entités ne sont vues par personne (pas de rendu
    // dans ce test), donc au huitième de cadence.
    let ratio = |culling_view: bool| {
        let mut app = app(culling_view);
        drive(&mut app, 60 * 60);
        let fleet = app.world().resource::<TrafficStats>().fleet;
        let (stepped, skipped) = (fleet.stepped_total as f64, fleet.skipped_total as f64);
        assert!(stepped + skipped > 1_000.0, "assez de véhicules-images : {fleet:?}");
        stepped / (stepped + skipped)
    };
    let full = ratio(false);
    let reduced = ratio(true);
    assert!((full - 1.0).abs() < 1e-9, "pleine cadence sans caméra : {full}");
    assert!(
        (reduced - 0.125).abs() < 0.03,
        "un tour sur huit hors champ : {reduced}"
    );
}

#[test]
fn le_pilote_recoit_l_espace_aerien_occupe_et_les_sujets_sortent_de_la_flotte() {
    let mut app = app(false);
    let mut bands_seen = 0;
    let mut subjects_seen = 0;
    for _ in 0..(60 * 90) {
        drive(&mut app, 1);
        bands_seen = bands_seen.max(app.world().resource::<PilotTraffic>().bands.len());
        let director = app.world().resource::<TrafficDirector>();
        let mut kinds = Vec::new();
        director.collect_subjects(|s| kinds.push((s.kind, s.id)));
        subjects_seen = subjects_seen.max(kinds.len());
        let mut ids: Vec<_> = kinds.iter().map(|k| k.1).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), kinds.len(), "ids uniques");
    }
    assert!(bands_seen > 0, "un véhicule est passé près de la voiture en altitude");
    assert!(subjects_seen > 2, "des sujets filmables : {subjects_seen}");
}

#[test]
fn les_corps_des_vehicules_vivants_sont_visibles_et_les_libres_caches() {
    let mut app = app(false);
    drive(&mut app, 60 * 40);
    let world = app.world_mut();
    let mut visible = 0;
    let mut hidden = 0;
    let mut query = world.query_filtered::<&Visibility, With<TrafficBody>>();
    for v in query.iter(world) {
        if *v == Visibility::Hidden {
            hidden += 1;
        } else {
            visible += 1;
        }
    }
    let active = world.resource::<TrafficStats>().fleet.active as usize;
    assert_eq!(visible, active, "autant de corps visibles que de véhicules vivants");
    assert_eq!(visible + hidden, 212);
}
