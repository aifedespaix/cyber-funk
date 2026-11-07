//! `place_projectors` n'alloue pas en régime établi (règle 4) : le système, enregistré une fois, tourne sous
//! `assert_no_alloc` pendant que la voiture traverse des centaines de rangées (choix des douze plus proches, libération et
//! occupation d'emplacements). Les candidats vivent dans un tampon réservé au démarrage.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::{Duration, Instant};

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::warmup::LoadTracker;
use cf_render::{AbsolutePosition, CorePlugin, StreamFocus};
use cf_world_neon::graffiti::projector::{ProjectorState, place_projectors};
use cf_world_neon::graffiti::{GraffitiAtlas, GraffitiPlugin};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn place_projectors_n_alloue_pas_en_regime_etabli() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        GraffitiPlugin::always("../../assets"),
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: 0.0,
                speed_mps: 0.0,
            },
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .init_resource::<LoadTracker>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_micros(16_667)));
    let start = Instant::now();
    while app.world().get_resource::<GraffitiAtlas>().is_none() {
        assert!(start.elapsed() < Duration::from_secs(60), "atlas non cuit en 60 s");
        app.update();
        std::thread::sleep(Duration::from_millis(5));
    }
    app.update();
    let id = app.world_mut().register_system(place_projectors);
    let mut cars = app
        .world_mut()
        .query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();
    let mut advance = |app: &mut App, meters: f64| {
        for mut position in cars.iter_mut(app.world_mut()) {
            position.0.z -= meters;
        }
        app.world_mut().run_system(id).expect("le système tourne");
    };
    // Échauffement : initialisation des paramètres du système et premières poses.
    for _ in 0..20 {
        advance(&mut app, 30.0);
    }
    let mut placed = 0;
    assert_no_alloc(|| {
        for _ in 0..400 {
            // Un peu plus d'une rangée par appel : le choix est recalculé à chaque fois.
            for mut position in cars.iter_mut(app.world_mut()) {
                position.0.z -= 30.0;
            }
            app.world_mut().run_system(id).expect("le système tourne");
        }
    });
    placed += app
        .world()
        .resource::<ProjectorState>()
        .active()
        .iter()
        .flatten()
        .count();
    assert!(placed > 0, "des projecteurs étaient en place");
}
