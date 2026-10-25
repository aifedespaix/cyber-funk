//! La ville de Neon City dans une vraie `App` Bevy, sans rendu : les pièces des tronçons (couche, culling, plan), les
//! arches, le matériau de façade nourri par la musique et la météo, la précision à 400 km.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::visibility::{RenderLayers, VisibilityRange};
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::streaming::StreamedPiece;
use cf_render::{
    AbsolutePosition, AudioFeatures, ChunkStream, CorePlugin, Layer, SceneColor, ScenePalette, StreamStats,
    aabb_encloses_mesh, mesh_aabb,
};
use cf_world_neon::route::{BridgeWindow, bridge_z};
use cf_world_neon::scenery::arches::ArchPool;
use cf_world_neon::scenery::layout::{MAX_PARTS, Row};
use cf_world_neon::scenery::{FOG_END_M, SceneryAssets};
use cf_world_neon::{ChasePlugin, FacadeLook, FacadeMaterial, NeonWorld, NeonWorldPlugin, RoadLook, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);
/// Tronçons vivants : 15 derrière, 1, 18 devant (`BEHIND`, `AHEAD`).
const CHUNKS: usize = 34;

fn app(plugin: NeonWorldPlugin, start_z_m: f64, speed_mps: f64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        plugin,
        ChasePlugin {
            config: RunnerConfig { start_z_m, speed_mps },
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

fn city(start_z_m: f64, speed_mps: f64) -> App {
    app(NeonWorldPlugin::embedded(), start_z_m, speed_mps)
}

type Piece<'a> = (&'a Visibility, &'a Transform, &'a MeshTag, &'a StreamedPiece);

fn visible_pieces(app: &mut App) -> usize {
    app.world_mut()
        .query::<Piece>()
        .iter(app.world())
        .filter(|(v, ..)| **v != Visibility::Hidden)
        .count()
}

#[test]
fn les_pieces_sont_dans_la_couche_monde_et_coupees_a_la_brume() {
    let mut app = city(-1234.5, 0.0);
    app.update();
    let mut pieces = app
        .world_mut()
        .query_filtered::<(&RenderLayers, &VisibilityRange), With<StreamedPiece>>();
    let mut n = 0;
    for (layers, range) in pieces.iter(app.world()) {
        assert_eq!(*layers, Layer::World.render_layers());
        assert!(
            range.use_aabb,
            "distance mesurée au pavé, pas à son origine 384 m sous la route"
        );
        assert!((range.end_margin.end - FOG_END_M).abs() < f32::EPSILON);
        n += 1;
    }
    assert_eq!(
        n,
        CHUNKS * MAX_PARTS,
        "toutes les pièces préallouées portent la couche et la plage"
    );
}

#[test]
fn un_pave_unite_a_l_aabb_exact() {
    let mut app = city(0.0, 0.0);
    app.update();
    let handle = app.world().resource::<SceneryAssets>().mesh.clone();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&handle).expect("mesh");
    let aabb = mesh_aabb(mesh).expect("positions");
    // Base à y = 0, arête 1 m : le `Transform` de chaque pièce en fait le pavé de la tour, sans `NoFrustumCulling`.
    assert!((aabb.center.y - 0.5).abs() < 1e-6 && (aabb.half_extents.x - 0.5).abs() < 1e-6);
    assert!(aabb_encloses_mesh(&aabb, mesh, 1e-5));
}

#[test]
fn les_pieces_d_un_troncon_sont_celles_de_sa_rangee() {
    let mut app = city(-1234.5, 0.0);
    for _ in 0..3 {
        app.update();
    }
    let world = app.world().resource::<NeonWorld>().clone();
    let stream = app.world().resource::<ChunkStream<NeonWorld>>();
    let index = stream.layout().chunk_at(1234.5);
    let slot = stream.slot_of(index).expect("le tronçon du point suivi vit");
    let family = app.world().resource::<SceneryAssets>().pieces;
    let entities: Vec<Entity> = stream.pieces(slot, family).to_vec();
    assert_eq!(entities.len(), MAX_PARTS);

    let row = Row::generate(index, world.scenery());
    let mut shown = 0;
    for (rank, &entity) in entities.iter().enumerate() {
        let visibility = *app.world().get::<Visibility>(entity).expect("visibilité");
        match row.parts().get(rank) {
            Some(part) => {
                let transform = app.world().get::<Transform>(entity).expect("transform");
                assert_ne!(visibility, Visibility::Hidden);
                assert_eq!(transform.scale.to_array(), part.size);
                assert_eq!(transform.translation.to_array(), [part.x, part.y, part.z]);
                assert_eq!(app.world().get::<MeshTag>(entity).expect("tag").value, part.tag());
                shown += 1;
            }
            None => assert_eq!(visibility, Visibility::Hidden, "pièce {rank} inutilisée"),
        }
    }
    assert!(shown > 5, "une rangée a des tours ({shown})");
    assert_eq!(
        app.world().resource::<StreamStats>().pieces_pooled as usize,
        CHUNKS * MAX_PARTS
    );
}

#[test]
fn la_ville_se_recycle_sans_creer_d_entite() {
    let mut app = city(0.0, 80.0);
    for _ in 0..10 {
        app.update();
    }
    let entities = app.world().entities().len();
    let shown = visible_pieces(&mut app);
    assert!(shown > 100, "la ville est là ({shown})");
    for _ in 0..900 {
        app.update();
    }
    assert_eq!(
        app.world().entities().len(),
        entities,
        "aucune entité créée en 15 s à 80 m/s"
    );
    // La densité reste celle de Sillage : environ la moitié des 36 emplacements d'une rangée sont pleins.
    let shown = visible_pieces(&mut app);
    assert!((CHUNKS * 8..CHUNKS * 20).contains(&shown), "{shown} pièces affichées");
}

#[test]
fn a_400_km_les_pieces_restent_pres_de_l_origine_rendue() {
    let mut app = city(-400_000.0, 40.0);
    for _ in 0..30 {
        app.update();
    }
    let mut pieces = app
        .world_mut()
        .query::<(&GlobalTransform, &Visibility, &StreamedPiece)>();
    let mut n = 0;
    for (global, visibility, _) in pieces.iter(app.world()) {
        if *visibility == Visibility::Hidden {
            continue;
        }
        // Une pièce est rendue à moins d'un demi-pas d'origine (+ hystérésis) : précision du millimètre pour le shader.
        assert!(global.translation().z.abs() < 16_000.0, "{}", global.translation().z);
        n += 1;
    }
    assert!(n > 100);
}

#[test]
fn les_arches_suivent_le_plan_de_route() {
    let target = bridge_z(2) + 100.0;
    let mut app = city(target, 0.0);
    for _ in 0..3 {
        app.update();
    }
    let window = BridgeWindow::around(target);
    let pool = app.world().resource::<ArchPool>().entities().to_vec();
    let mut placed = Vec::new();
    for &entity in &pool {
        if *app.world().get::<Visibility>(entity).expect("visibilité") != Visibility::Hidden {
            placed.push(app.world().get::<AbsolutePosition>(entity).expect("position").0.z);
        }
    }
    assert_eq!(
        placed.len(),
        3 * window.iter().count(),
        "trois pavés par arche affichée"
    );
    assert!(placed.iter().all(|z| (z - bridge_z(2)).abs() < 1e-9), "{placed:?}");
    // Loin de toute arche, rien n'est affiché.
    let mut far = city(-300.0, 0.0);
    far.update();
    far.update();
    let pool = far.world().resource::<ArchPool>().entities().to_vec();
    assert!(
        pool.iter()
            .all(|&e| *far.world().get::<Visibility>(e).expect("visibilité") == Visibility::Hidden)
    );
}

#[test]
fn sans_ville_il_ne_reste_que_la_route() {
    let mut app = app(NeonWorldPlugin::embedded().without_city(), bridge_z(1), 0.0);
    for _ in 0..3 {
        app.update();
    }
    assert_eq!(visible_pieces(&mut app), 0);
    let pool = app.world().resource::<ArchPool>().entities().to_vec();
    assert!(
        pool.iter()
            .all(|&e| *app.world().get::<Visibility>(e).expect("visibilité") == Visibility::Hidden)
    );
}

#[test]
fn la_qualite_retire_d_abord_les_couronnes_lointaines() {
    let mut app = city(-500.0, 0.0);
    for _ in 0..3 {
        app.update();
    }
    let full = visible_pieces(&mut app);
    let changed = app.world_mut().resource_mut::<NeonWorld>().set_quality(3);
    assert!(changed);
    app.world_mut().resource_mut::<ChunkStream<NeonWorld>>().replan_all();
    app.update();
    let reduced = visible_pieces(&mut app);
    assert!(reduced < full, "{reduced} pièces au plus bas niveau contre {full}");
    assert!(
        reduced * 10 > full * 6,
        "seules les couronnes lointaines partent ({reduced} / {full})"
    );
}

#[test]
fn le_materiau_de_facade_suit_la_musique_la_meteo_et_la_palette() {
    let mut app = city(0.0, 0.0);
    app.insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL));
    {
        let mut features = app.world_mut().resource_mut::<AudioFeatures>();
        features.0.pulse = 0.8;
        features.0.bass = 0.3;
        features.0.mid = 0.4;
        features.0.high = 0.6;
        features.0.beat = 17.25;
    }
    app.world_mut()
        .resource_mut::<cf_world_neon::WeatherSettings>()
        .drive_road_look = false;
    app.world_mut().resource_mut::<RoadLook>().wet = 0.5;
    app.world_mut().resource_mut::<RoadLook>().rain = 0.25;
    app.world_mut().resource_mut::<FacadeLook>().build = 0.4;
    app.world_mut().resource_mut::<FacadeLook>().clock_s = Some(2400.0 + 3.0);
    for _ in 0..3 {
        app.update();
    }
    let handle = app.world().resource::<SceneryAssets>().material.clone();
    let params = app
        .world()
        .resource::<Assets<FacadeMaterial>>()
        .get(&handle)
        .expect("matériau")
        .params;
    assert!(
        (params.frame.x - 3.0).abs() < 1e-4,
        "le temps repart de zéro toutes les 2 400 s : {}",
        params.frame.x
    );
    assert!((params.frame.y - 0.8).abs() < 1e-6 && (params.frame.z - 0.3).abs() < 1e-6);
    assert!((params.frame.w - 0.4).abs() < 1e-6 && (params.audio.x - 0.6).abs() < 1e-6);
    assert!((params.audio.y - 17.25).abs() < 1e-6 && (params.audio.z - 0.4).abs() < 1e-6);
    assert!((params.weather.x - 0.5).abs() < 1e-6 && (params.weather.y - 0.25).abs() < 1e-6);
    // La palette est celle de la scène : le cycle prismatique et l'encre.
    let palette = *app.world().resource::<ScenePalette>();
    let mint = palette.get(SceneColor::Mint);
    assert!((params.scan.x - mint.r).abs() < 1e-6 && (params.cycle[0].z - mint.b).abs() < 1e-6);
    assert!(
        (params.cycle[3].y - mint.g).abs() < 1e-6,
        "le cyan revient au rang 3 du cycle"
    );
    assert!(
        params.ink.x > 0.0 || params.ink.y > 0.0 || params.ink.z > 0.0,
        "palette de départ chargée"
    );
}
