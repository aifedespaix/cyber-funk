//! Chargement et préchauffe sans GPU : machine à états `Loading` → `Ready`, copies de préchauffe, couches forcées.
//!
//! Ce que la carte graphique en fait (compilation réelle, journal vide après `Ready`) est vérifié par le lancement du
//! binaire sous Xvfb + lavapipe (voir la PR).

#![allow(clippy::expect_used, clippy::panic)]

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, Projection};
use bevy::prelude::*;
use cf_render::warmup::WarmEntity;
use cf_render::{
    AppState, CorePlugin, FloatingOrigin, Layer, LayerCamera, LayerOpacity, LayerWarmup, LayersPlugin, LoadTracker,
    WarmupAppExt, WarmupPlugin, WarmupProgress, show_may_run,
};

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), CorePlugin))
        .init_asset::<Image>()
        .init_asset::<Mesh>()
        .init_asset::<StandardMaterial>();
    app
}

fn state(app: &App) -> AppState {
    *app.world().resource::<State<AppState>>().get()
}

/// Avance jusqu'à `Ready` (au plus `max` images) ; rend le nombre d'images.
fn run_until_ready(app: &mut App, max: u32) -> u32 {
    for frame in 1..=max {
        app.update();
        if state(app) == AppState::Ready {
            return frame;
        }
    }
    panic!("toujours en Loading après {max} images");
}

#[test]
fn sans_etape_le_chargement_se_termine() {
    let mut app = app();
    app.add_plugins(WarmupPlugin);
    run_until_ready(&mut app, 200);
    assert!(app.world().resource::<WarmupProgress>().ready_after_s.is_some());
    assert!(!app.world().resource::<LayerWarmup>().0, "les couches reprennent l'opacité du look");
}

#[test]
fn une_etape_en_cours_retient_le_chargement_puis_le_libere() {
    let mut app = app();
    app.add_plugins(WarmupPlugin);
    let step = app.world_mut().resource_mut::<LoadTracker>().register("voiture");
    for _ in 0..100 {
        app.update();
    }
    assert_eq!(state(&app), AppState::Loading, "tant que la voiture n'est pas chargée");
    app.world_mut().resource_mut::<LoadTracker>().finish(step);
    run_until_ready(&mut app, 200);
}

#[test]
fn un_asset_manquant_est_signale_mais_n_empeche_pas_l_ouverture() {
    let mut app = app();
    app.add_plugins(WarmupPlugin);
    let step = app.world_mut().resource_mut::<LoadTracker>().register("musiques");
    app.world_mut()
        .resource_mut::<LoadTracker>()
        .fail(step, "assets/music/x.mp3 : introuvable");
    run_until_ready(&mut app, 200);
    assert_eq!(app.world().resource::<LoadTracker>().failures().count(), 1);
}

#[derive(Resource, Default)]
struct Ticks(u32);

#[test]
fn le_show_ne_court_qu_en_ready() {
    let mut app = app();
    app.add_plugins(WarmupPlugin)
        .init_resource::<Ticks>()
        .add_systems(Update, (|mut ticks: ResMut<Ticks>| ticks.0 += 1).run_if(show_may_run));
    let frames = run_until_ready(&mut app, 200);
    // La transition s'applique avant `Update` : seule la dernière image (déjà en `Ready`) a fait tourner le show.
    assert_eq!(app.world().resource::<Ticks>().0, 1, "rien pendant les {} images de chargement", frames - 1);
    app.update();
    assert_eq!(app.world().resource::<Ticks>().0, 2);
}

#[test]
fn sans_le_plugin_le_show_court() {
    let mut app = app();
    app.init_resource::<Ticks>()
        .add_systems(Update, (|mut ticks: ResMut<Ticks>| ticks.0 += 1).run_if(show_may_run));
    app.update();
    assert_eq!(app.world().resource::<Ticks>().0, 1);
}

#[test]
fn les_copies_de_prechauffe_sont_dedupliquees_posees_devant_la_camera_et_retirees() {
    let mut app = app();
    app.add_plugins(WarmupPlugin);
    app.warm_material::<StandardMaterial>("matériaux standard");
    let (cuboid, blended, opaque) = {
        let world = app.world_mut();
        let cuboid = world.resource_mut::<Assets<Mesh>>().add(Cuboid::default());
        let mut materials = world.resource_mut::<Assets<StandardMaterial>>();
        let opaque = materials.add(StandardMaterial::default());
        let blended = materials.add(StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        (cuboid, blended, opaque)
    };
    // Trois entités du monde (une cachée, comme dans un pool) : deux combinaisons seulement.
    let world_layers = RenderLayers::layer(1);
    for (material, visibility) in [
        (opaque.clone(), Visibility::Visible),
        (opaque, Visibility::Hidden),
        (blended, Visibility::Hidden),
    ] {
        app.world_mut().spawn((
            Mesh3d(cuboid.clone()),
            MeshMaterial3d(material),
            world_layers.clone(),
            visibility,
        ));
    }
    let camera = app
        .world_mut()
        .spawn((
            Camera::default(),
            Projection::Perspective(PerspectiveProjection {
                near: 0.1,
                far: 2_600.0,
                ..default()
            }),
            world_layers,
            Transform::from_xyz(10.0, 2.0, 5.0),
            GlobalTransform::from(Transform::from_xyz(10.0, 2.0, 5.0)),
        ))
        .id();

    // Jusqu'à la phase de compilation : les copies existent.
    for _ in 0..40 {
        app.update();
        if app.world_mut().query::<&WarmEntity>().iter(app.world()).count() > 0 {
            break;
        }
    }
    app.update();
    let copies: Vec<Vec3> = app
        .world_mut()
        .query::<(&WarmEntity, &Transform)>()
        .iter(app.world())
        .map(|(_, transform)| transform.translation)
        .collect();
    assert_eq!(copies.len(), 2, "une copie par (maillage, mode alpha, couches)");
    let camera_position = app.world().get::<GlobalTransform>(camera).expect("caméra").translation();
    for position in copies {
        let ahead = position - camera_position;
        assert!(ahead.z < -1.0, "devant la caméra (regard −z) : {ahead:?}");
        assert!(ahead.length() < 2_600.0, "dans son volume de vision");
    }

    run_until_ready(&mut app, 200);
    assert_eq!(
        app.world_mut().query::<&WarmEntity>().iter(app.world()).count(),
        0,
        "les copies sont détruites avant le show"
    );
    // Les entités du monde, elles, n'ont pas été touchées.
    assert_eq!(app.world_mut().query::<&Mesh3d>().iter(app.world()).count(), 3 + curtain_quads(&mut app));
}

fn curtain_quads(app: &mut App) -> usize {
    app.world_mut()
        .query_filtered::<&Mesh3d, With<cf_render::warmup::Curtain>>()
        .iter(app.world())
        .count()
}

#[test]
fn pendant_le_chargement_toutes_les_couches_rendent_meme_si_le_look_en_eteint() {
    let mut app = app();
    app.init_resource::<FloatingOrigin>()
        .add_plugins(LayersPlugin {
            fallback_size: UVec2::new(64, 36),
        })
        .add_plugins(WarmupPlugin);
    // Un look qui éteint l'avant-plan et la surcouche.
    {
        let mut opacity = app.world_mut().resource_mut::<LayerOpacity>();
        opacity.set(Layer::Foreground, 0.0);
        opacity.set(Layer::Overlay, 0.0);
    }
    let active = |app: &mut App, layer: Layer| {
        app.world_mut()
            .query::<(&LayerCamera, &Camera)>()
            .iter(app.world())
            .find(|(camera, _)| camera.0 == layer)
            .map(|(_, camera)| camera.is_active)
            .expect("caméra de couche")
    };
    app.update();
    app.update();
    assert!(active(&mut app, Layer::Foreground), "préchauffe : l'avant-plan rend");
    assert!(active(&mut app, Layer::Overlay), "préchauffe : la surcouche rend");
    run_until_ready(&mut app, 200);
    app.update();
    assert!(!active(&mut app, Layer::Foreground), "après Ready : le look éteint l'avant-plan");
    assert!(!active(&mut app, Layer::Overlay));
    assert!(active(&mut app, Layer::World));
}
