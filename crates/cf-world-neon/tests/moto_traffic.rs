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
use cf_world_neon::rider::RiderPaintMaterial;
use cf_world_neon::rider::moto::{BODY_MATERIALS, MOTO_COUNT, MOTO_FILES};
use cf_world_neon::traffic::{TrafficEntities, TrafficPlugin};

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
