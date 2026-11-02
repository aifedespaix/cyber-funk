//! Les motos texturées du trafic (X-10) : les GLB portent bien les matériaux de carrosserie qu'on teinte, et le pool de la
//! variante moto est préalloué avec ses modèles, sans entité créée ensuite.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::path::Path;
use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, CorePlugin, CullingPlugin, StreamFocus};
use cf_world_neon::car::CarModelAsset;
use cf_world_neon::pilot::rand::Rand;
use cf_world_neon::rider::RiderPaintMaterial;
use cf_world_neon::rider::anim::{MotoRig, WHEEL_DIAMETER_M};
use cf_world_neon::rider::moto::{BODY_MATERIALS, MOTO_COUNT, MOTO_FILES};
use cf_world_neon::traffic::{TrafficDirector, TrafficEntities, TrafficPlugin};

const FRAME: Duration = Duration::from_micros(16_667);

fn assets() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

/// Nom d'un matériau sans le suffixe de doublon de Blender (`.001`).
fn base_name(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) => head,
        _ => name,
    }
}

#[test]
fn les_glb_des_motos_portent_les_materiaux_de_carrosserie() {
    for (index, file) in MOTO_FILES.into_iter().enumerate() {
        let bytes = std::fs::read(assets().join("models/motos").join(file)).expect("lecture");
        let model = CarModelAsset::from_glb(&bytes).expect("décodage");
        for wanted in BODY_MATERIALS[index] {
            assert!(
                model.materials.iter().any(|m| base_name(&m.name) == *wanted),
                "{file} n'a pas de matériau « {wanted} » : {:?}",
                model.materials.iter().map(|m| m.name.as_str()).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn le_pool_des_motos_est_garni_de_modeles_et_ne_change_plus() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        CullingPlugin::default(),
        TrafficPlugin {
            assets_dir: assets(),
            ..TrafficPlugin::default()
        },
    ))
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    app.update();

    let bike = cf_world_neon::traffic::variant::BIKE;
    let slots = [0_u16, 1, 2, 3, 4, 5];
    for slot in slots {
        let body = app
            .world()
            .resource::<TrafficEntities>()
            .body(bike, slot)
            .expect("corps de moto");
        let children = app.world().get::<Children>(body).expect("la moto a un modèle");
        assert!(!children.is_empty(), "moto {slot} sans modèle");
    }
    let tinted = app.world().resource::<Assets<RiderPaintMaterial>>().len();
    assert!(tinted >= MOTO_COUNT, "matériaux teintés : {tinted}");

    let count = |app: &mut App| app.world_mut().query::<Entity>().iter(app.world()).count();
    let before = count(&mut app);
    for _ in 0..600 {
        app.update();
    }
    assert_eq!(count(&mut app), before, "aucune entité créée en régime établi");
}

/// Le pivot `name` (sa translation) et la boîte de son sous-arbre, dans le repère du pivot : (translation, taille, centre).
fn pivot_box(model: &CarModelAsset, name: &str) -> Option<(Vec3, Vec3, Vec3)> {
    use bevy::mesh::VertexAttributeValues;
    let root = model.nodes.iter().position(|n| n.name == name)?;
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    let mut stack = vec![(root, Mat4::IDENTITY, true)];
    while let Some((index, parent, is_root)) = stack.pop() {
        let node = &model.nodes[index];
        // La translation du pivot est ailleurs : la boîte est dans son repère, d'origine le pivot.
        let world = if is_root {
            parent
        } else {
            parent * node.transform.to_matrix()
        };
        for &(mesh, _) in &node.pieces {
            if let Some(VertexAttributeValues::Float32x3(points)) =
                model.meshes[mesh].attribute(Mesh::ATTRIBUTE_POSITION)
            {
                for p in points {
                    let p = world.transform_point3(Vec3::from(*p));
                    lo = lo.min(p);
                    hi = hi.max(p);
                }
            }
        }
        stack.extend(node.children.iter().map(|&c| (c, world, false)));
    }
    Some((model.nodes[root].transform.translation, hi - lo, (hi + lo) / 2.0))
}

/// Les roues et le pilote sont des nœuds à part, à leur place : roues rondes du diamètre annoncé, centrées sur leur essieu,
/// pilote au bassin. Sans quoi l'animation tournerait n'importe quoi autour de n'importe quoi.
#[test]
fn les_glb_portent_roues_et_pilote_comme_noeuds_animables() {
    for (index, file) in MOTO_FILES.into_iter().enumerate() {
        let bytes = std::fs::read(assets().join("models/motos").join(file)).expect("lecture");
        let model = CarModelAsset::from_glb(&bytes).expect("décodage");
        for (side, front) in [("spin_F", true), ("spin_R", false)] {
            let (axle, size, centre) = pivot_box(&model, side).unwrap_or_else(|| panic!("{file} : pas de nœud {side}"));
            let diameter = WHEEL_DIAMETER_M[index][usize::from(!front)];
            assert!(
                (size.y - diameter).abs() < 0.05 && (size.z - diameter).abs() < 0.05,
                "{file} {side} : pneu de {size} pour {diameter} m"
            );
            assert!(size.x < 0.5, "{file} {side} : {} m de large", size.x);
            // Le pneu est centré sur le pivot : sinon la roue tournerait en excentrique.
            assert!(
                centre.y.abs() < 0.02 && centre.z.abs() < 0.02,
                "{file} {side} : roue décentrée de {centre}"
            );
            assert!(
                axle.x.abs() < 0.02 && axle.y > 0.15 && axle.y < 0.5,
                "{file} {side} : essieu à {axle}"
            );
            // L'avant du modèle est +Z : la roue avant est devant.
            assert_eq!(axle.z > 0.0, front, "{file} {side} : essieu à {axle}");
        }
        let (pelvis, size, _) = pivot_box(&model, "rider").unwrap_or_else(|| panic!("{file} : pas de nœud rider"));
        assert!(
            pelvis.y > 0.4 && pelvis.y < 1.2 && pelvis.x.abs() < 0.02,
            "{file} : bassin à {pelvis}"
        );
        assert!(size.y > 0.5 && size.y < 1.6, "{file} : pilote de {size}");
    }
}

fn traffic_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        CullingPlugin::default(),
        TrafficPlugin {
            assets_dir: assets(),
            ..TrafficPlugin::default()
        },
    ))
    .init_asset::<Mesh>()
    .init_asset::<Image>()
    .init_asset::<StandardMaterial>()
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app.world_mut()
        .spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    app.update();
    app
}

/// Pose une moto devant la voiture, à `speed` m/s, et rend le corps de l'emplacement pris.
fn launch(app: &mut App, speed: f32) -> Entity {
    let mut rng = Rand::new(7);
    let id = {
        let mut director = app.world_mut().resource_mut::<TrafficDirector>();
        director
            .fleet
            .spawn(
                cf_world_neon::traffic::variant::BIKE,
                DVec3::new(3.0, 0.6, -400_040.0),
                Vec3::new(0.0, 0.0, -speed),
                LinearRgba::rgb(0.9, 0.3, 0.2),
                1.3,
                &mut rng,
            )
            .expect("une moto")
    };
    let slot = app
        .world()
        .resource::<TrafficDirector>()
        .fleet
        .get(id)
        .expect("vivante")
        .slot;
    app.world()
        .resource::<TrafficEntities>()
        .body(cf_world_neon::traffic::variant::BIKE, slot)
        .expect("corps")
}

fn rotation(app: &App, entity: Option<Entity>) -> Quat {
    app.world()
        .get::<Transform>(entity.expect("nœud"))
        .expect("transformation")
        .rotation
}

#[test]
fn une_moto_qui_roule_fait_tourner_ses_roues_et_bouger_son_pilote() {
    let mut app = traffic_app();
    let body = launch(&mut app, 30.0);
    app.update();
    let nodes = app.world().get::<MotoRig>(body).expect("moto animée").nodes;
    assert!(
        nodes.front.is_some() && nodes.rear.is_some() && nodes.rider.is_some(),
        "{nodes:?}"
    );
    let (wheel0, rider0) = (rotation(&app, nodes.rear), rotation(&app, nodes.rider.map(|r| r.0)));
    let mut wheel_changes = 0;
    let mut rider_min_max = (f32::MAX, f32::MIN);
    let mut last = wheel0;
    for _ in 0..90 {
        app.update();
        let now = rotation(&app, nodes.rear);
        if now.angle_between(last) > 1e-3 {
            wheel_changes += 1;
        }
        last = now;
        let tilt = rotation(&app, nodes.rider.map(|r| r.0)).to_euler(EulerRot::XYZ).0;
        rider_min_max = (rider_min_max.0.min(tilt), rider_min_max.1.max(tilt));
    }
    assert!(
        wheel_changes > 80,
        "la roue arrière tourne à chaque image : {wheel_changes}/90"
    );
    assert!(rotation(&app, nodes.front).angle_between(rotation(&app, nodes.rear)) >= 0.0);
    // Le pilote reste près de sa pose (quelques degrés) mais n'est pas figé.
    let _ = rider0;
    assert!(
        rider_min_max.1 - rider_min_max.0 > 1e-4,
        "le pilote respire : {rider_min_max:?}"
    );
    assert!(
        rider_min_max.0.abs() < 0.3 && rider_min_max.1.abs() < 0.3,
        "{rider_min_max:?}"
    );
}

#[test]
fn le_pilote_et_les_roues_ne_changent_pas_le_nombre_d_entites() {
    let mut app = traffic_app();
    let _ = launch(&mut app, 25.0);
    let count = |app: &mut App| app.world_mut().query::<Entity>().iter(app.world()).count();
    app.update();
    let before = count(&mut app);
    for _ in 0..300 {
        app.update();
    }
    assert_eq!(count(&mut app), before, "l'animation ne crée aucune entité");
}
