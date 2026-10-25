//! Le monde Neon City dans une vraie `App` Bevy, sans rendu : couches, `Aabb`, suivi du point suivi, matériau nourri par
//! la musique, absence d'entités créées en régime établi, précision à 400 km.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, ChunkStream, CorePlugin, FloatingOrigin, Layer, StreamFocus,
    StreamStats, aabb_encloses_mesh, mesh_aabb,
};
use cf_world_neon::meteors::{MeteorPart, Piece};
use cf_world_neon::road::{AHEAD_M, LENGTH_M};
use cf_world_neon::route::{BridgeWindow, bridge_z};
use cf_world_neon::sky::SkyPart;
use cf_world_neon::{
    ChasePlugin, NeonWorld, NeonWorldPlugin, Road, RoadLook, RoadMaterial, RoutePlan, Runner, RunnerConfig,
};

const FRAME: Duration = Duration::from_micros(16_667);

fn app(start_z_m: f64, speed_mps: f64) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
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

fn single<C: Component>(app: &mut App) -> Entity {
    let mut query = app.world_mut().query_filtered::<Entity, With<C>>();
    query.single(app.world()).expect("une seule entité")
}

#[test]
fn tout_ce_que_le_monde_dessine_est_dans_la_couche_monde() {
    let mut app = app(0.0, 40.0);
    app.update();
    let world_layers = Layer::World.render_layers();
    // Le ciel est dans la couche arrière-plan (testé dans `sky.rs`) : ici, tout le reste.
    let mut meshes = app
        .world_mut()
        .query_filtered::<(&Name, &RenderLayers, &Mesh3d), (Without<SkyPart>, Without<MeteorPart>)>();
    let found: Vec<_> = meshes
        .iter(app.world())
        .map(|(n, l, _)| (n.to_string(), l.clone()))
        .collect();
    // La route, le point suivi et les six pavés des arches ont un nom ; les pièces des tronçons n'en ont pas (voir `city.rs`).
    // S'y ajoutent la pluie et la brume (J5-04, une entité chacune, créées au démarrage), les pools préalloués des
    // projecteurs (un cône et une tache chacun) et du paquet du drop (les ondes et la gerbe) : tous dans la couche monde.
    let pools = cf_world_neon::beams::MAX_BEAMS * 2 + cf_world_neon::dropfx::RINGS + 1;
    assert_eq!(
        found.len(),
        2 + 6 + 2 + pools,
        "la route, le point suivi, les arches, la pluie, la brume et les pools : {found:?}"
    );
    for (name, layers) in &found {
        assert_eq!(*layers, world_layers, "{name} n'est pas dans la couche monde");
    }
    // Chaque entité qui dessine un mesh porte un `RenderLayers` : aucune n'est restée à la couche 0 (le fond).
    let mut bare = app
        .world_mut()
        .query_filtered::<Entity, (With<Mesh3d>, Without<RenderLayers>)>();
    assert_eq!(bare.iter(app.world()).count(), 0);
}

#[test]
fn les_astres_sont_un_pool_cache_de_la_couche_monde_sans_culling_force() {
    use bevy::camera::primitives::Aabb;
    let mut app = app(0.0, 40.0);
    app.update();
    let mut parts = app
        .world_mut()
        .query_filtered::<(&Piece, &RenderLayers, &Visibility, Has<Aabb>), With<MeteorPart>>();
    let found: Vec<_> = parts
        .iter(app.world())
        .map(|(p, l, v, aabb)| (*p, l.clone(), *v, aabb))
        .collect();
    // Douze astres (corps, queue, halo), douze anneaux de brume, six explosions (éclair, onde, gerbe).
    assert_eq!(found.len(), 12 * 4 + 6 * 3);
    for (piece, layers, visibility, _) in &found {
        assert_eq!(
            *layers,
            Layer::World.render_layers(),
            "{piece:?} n'est pas dans la couche monde"
        );
        assert_eq!(*visibility, Visibility::Hidden, "{piece:?} est visible au repos");
    }
    // Le corps (bosselé par son vertex shader) et la gerbe (mouvante) portent un `Aabb` explicite.
    for (piece, .., aabb) in &found {
        if matches!(piece, Piece::Body(_) | Piece::Debris(_)) {
            assert!(aabb, "{piece:?} doit porter son Aabb explicite");
        }
    }
    // Aucune entité créée en régime établi : le pool est le même après une seconde.
    for _ in 0..60 {
        app.update();
    }
    let after = parts.iter(app.world()).count();
    assert_eq!(after, 12 * 4 + 6 * 3);
}

#[test]
fn l_aabb_de_la_route_est_celui_de_son_plan() {
    let mut app = app(0.0, 0.0);
    app.update();
    let road = single::<Road>(&mut app);
    let handle = app.world().get::<Mesh3d>(road).expect("mesh").0.clone();
    let meshes = app.world().resource::<Assets<Mesh>>();
    let mesh = meshes.get(&handle).expect("mesh chargé");
    let aabb = mesh_aabb(mesh).expect("positions");
    // Plan statique centré sur son origine : 18 m × 1 200 m, sans épaisseur. Pas de `NoFrustumCulling`.
    assert!((aabb.half_extents.x - 9.0).abs() < 1e-5 && (aabb.half_extents.z - LENGTH_M / 2.0).abs() < 1e-3);
    assert!(aabb.half_extents.y.abs() < 1e-6);
    assert!(
        aabb.center.length() < 1e-6,
        "centré sur l'origine de l'entité, qui suit le point suivi"
    );
    assert!(aabb_encloses_mesh(&aabb, mesh, 1e-4));
}

#[test]
fn la_route_suit_le_point_suivi_en_depassant_de_300_m_devant() {
    let mut app = app(-12_345.0, 40.0);
    for _ in 0..120 {
        app.update();
    }
    let focus = app
        .world_mut()
        .query_filtered::<&AbsolutePosition, With<StreamFocus>>()
        .single(app.world())
        .expect("point suivi")
        .0;
    assert!(focus.z < -12_345.0 - 70.0, "le point suivi avance : {}", focus.z);
    let road = single::<Road>(&mut app);
    let position = app.world().get::<AbsolutePosition>(road).expect("position").0;
    assert!((position.z - (focus.z - AHEAD_M)).abs() < 1e-9);
    assert!(position.x.abs() < 1e-12 && position.y.abs() < 1e-12);
}

#[test]
fn a_400_km_le_plan_reste_pres_de_l_origine_rendue() {
    let mut app = app(-400_000.0, 40.0);
    for _ in 0..30 {
        app.update();
    }
    let origin = app.world().resource::<FloatingOrigin>().origin();
    assert!(origin.z < -390_000.0, "l'origine a suivi : {}", origin.z);
    let road = single::<Road>(&mut app);
    let translation = app.world().get::<Transform>(road).expect("transform").translation;
    // Le plan est rendu à quelques centaines de mètres de l'origine : précision du millimètre pour le shader.
    assert!(
        translation.z.abs() < 16_000.0,
        "plan rendu à {} m de l'origine",
        translation.z
    );
    let mut position = app.world_mut().query_filtered::<&Transform, With<Runner>>();
    let car = position.single(app.world()).expect("point suivi").translation;
    assert!(
        (translation.z - car.z + AHEAD_M as f32).abs() < 1e-2,
        "{} contre {}",
        translation.z,
        car.z
    );
}

#[test]
fn le_materiau_suit_le_pouls_les_basses_et_la_teinte() {
    let mut app = app(0.0, 0.0);
    app.insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL));
    app.world_mut()
        .resource_mut::<cf_world_neon::WeatherSettings>()
        .drive_road_look = false;
    app.world_mut().resource_mut::<RoadLook>().wet = 0.5;
    {
        let mut features = app.world_mut().resource_mut::<AudioFeatures>();
        features.0.pulse = 0.8;
        features.0.bass = 0.3;
        features.0.intensity = 1.0;
    }
    for _ in 0..3 {
        app.update();
    }
    let road = single::<Road>(&mut app);
    let handle = app
        .world()
        .get::<MeshMaterial3d<RoadMaterial>>(road)
        .expect("matériau")
        .0
        .clone();
    let materials = app.world().resource::<Assets<RoadMaterial>>();
    let params = materials.get(&handle).expect("matériau chargé").params;
    assert!((params.frame.y - 0.8).abs() < 1e-6 && (params.frame.z - 0.3).abs() < 1e-6);
    assert!((params.look.y - 0.5).abs() < 1e-6);
    // Intensité 1 : la teinte d'ambiance est le corail du sommet.
    let rose = RoadLook::default().palette.rose;
    assert!((params.mood.x - rose.red).abs() < 1e-6 && (params.mood.z - rose.blue).abs() < 1e-6);
    // La voiture est à l'origine : la lueur est sous elle.
    assert!(params.car.truncate().length() < 1e-3 || params.car.z.abs() < 1e-3);
}

#[test]
fn aucune_entite_ni_aucun_tronçon_cree_en_regime_etabli() {
    let mut app = app(0.0, 60.0);
    for _ in 0..10 {
        app.update();
    }
    let entities = app.world().entities().len();
    let live = app.world().resource::<ChunkStream<NeonWorld>>().live().len();
    for _ in 0..600 {
        app.update();
    }
    assert_eq!(
        app.world().entities().len(),
        entities,
        "aucune entité créée ou détruite en 10 s de route"
    );
    let stream = app.world().resource::<ChunkStream<NeonWorld>>();
    assert_eq!(stream.live().len(), live);
    let stats = app.world().resource::<StreamStats>();
    assert!(
        stats.chunks_filled_total > u64::from(stats.chunks_capacity),
        "les tronçons tournent bien : {stats:?}"
    );
}

#[test]
fn le_plan_de_route_suit_les_ponts() {
    let target = bridge_z(2) + 100.0;
    let mut app = app(target, 0.0);
    app.update();
    app.update();
    let plan = *app.world().resource::<RoutePlan>();
    let expected = BridgeWindow::around(target);
    assert_eq!(plan.bridges, expected);
    assert!(
        plan.bridges.iter().any(|b| b.cell == 2),
        "l'arche de la cellule 2 est devant la voiture"
    );
}

#[test]
fn la_camera_de_poursuite_ecrit_la_pose() {
    let mut app = app(-1_000.0, 0.0);
    app.update();
    app.update();
    let pose = *app.world().resource::<CameraPose>();
    assert!((pose.position.z - (-1_000.0 + 6.2)).abs() < 1e-9);
    assert!(pose.position.y > 2.2 && pose.position.y < 2.5);
    assert_eq!(pose.position.x.to_bits(), DVec3::ZERO.x.to_bits());
}
