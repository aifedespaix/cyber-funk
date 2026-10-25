//! Les enseignes dans une vraie `App` Bevy, sans rendu : carnet de chargement, couche monde, aucune entité créée en régime
//! établi, panneaux posés devant la voiture et dans la portée de leur format.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::warmup::LoadTracker;
use cf_render::{AbsolutePosition, CorePlugin, Layer};
use cf_world_neon::signs::rig::{SignRig, SignUnitIndex};
use cf_world_neon::signs::{SignAtlas, SignRoadRes, SignShow, SignsPlugin};
use cf_world_neon::{ChasePlugin, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        SignsPlugin::always(),
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: 0.0,
                speed_mps: 60.0,
            },
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_resource::<LoadTracker>()
    .insert_resource(SignShow {
        active: false,
        dj_on_ads: true,
        slot: 0,
        slots: SignShow::names("Démesure", ["KRMN", "Nova Kaito"]),
    })
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

/// Fait tourner l'application jusqu'à ce que l'atlas soit cuit (tâche de fond).
fn until_baked(app: &mut App) {
    let start = Instant::now();
    while app.world().get_resource::<SignAtlas>().is_none() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "l'atlas n'a pas été cuit en 60 s"
        );
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn le_show_attend_les_textures_et_toutes_les_entites_sont_dans_la_couche_monde() {
    let mut app = app();
    app.update();
    {
        let tracker = app.world().resource::<LoadTracker>();
        assert!(
            !tracker.is_complete(),
            "l'étape « Enseignes (textures) » doit être en cours"
        );
        assert!(tracker.steps().iter().any(|s| s.label.contains("Enseignes")));
    }
    until_baked(&mut app);
    app.update();
    assert!(
        app.world().resource::<LoadTracker>().is_complete(),
        "l'étape est close quand l'atlas est prêt"
    );
    let atlas = app.world().resource::<SignAtlas>();
    assert!(atlas.bake_s < 20.0 && atlas.cells.len() > 250 && atlas.strips.len() > 100);

    let world_layers = Layer::World.render_layers();
    let rig = app.world().resource::<SignRig>();
    assert_eq!(rig.units.len(), 11);
    let mut with_mesh = app
        .world_mut()
        .query_filtered::<(&RenderLayers, &Mesh3d), With<ChildOf>>();
    let (mut count, mut wrong) = (0, 0);
    for (layers, _) in with_mesh.iter(app.world()) {
        count += 1;
        wrong += usize::from(*layers != world_layers);
    }
    // Onze plans et 13 pièces par unité (les enfants des racines ; le point suivi provisoire n'en est pas un).
    assert_eq!(count, 11 * 14);
    assert_eq!(
        wrong, 0,
        "chaque pièce porte la couche monde (Bevy ne la propage pas aux enfants)"
    );
    let mut roots = app.world_mut().query::<(&SignUnitIndex, &RenderLayers)>();
    assert_eq!(roots.iter(app.world()).filter(|(_, l)| **l == world_layers).count(), 11);
}

#[test]
fn rien_n_est_cree_ni_detruit_en_regime_etabli_et_les_panneaux_sont_devant_la_voiture() {
    let mut app = app();
    until_baked(&mut app);
    for _ in 0..30 {
        app.update();
    }
    let entities = app.world().entities().len();
    let mut seen_visible = 0;
    for frame in 0..60 * 40 {
        app.update();
        if frame % 30 == 0 {
            let car_z = {
                let mut q = app
                    .world_mut()
                    .query_filtered::<&AbsolutePosition, With<cf_render::StreamFocus>>();
                q.single(app.world()).expect("le point suivi").0.z
            };
            let road = &app.world().resource::<SignRoadRes>().0;
            for unit in road.units.iter().filter(|u| u.state.is_some() && u.visible) {
                let ahead = car_z - unit.z;
                let format = unit.format.expect("format").format();
                assert!(
                    ahead <= format.range + 1.0 && ahead >= -format.behind - 1.0,
                    "{ahead} m hors de portée"
                );
                seen_visible += 1;
            }
        }
    }
    assert_eq!(
        app.world().entities().len(),
        entities,
        "des entités ont été créées en cours de route"
    );
    assert!(seen_visible > 20, "{seen_visible} panneaux vus en 40 s de route");
}
