//! Garde-fou du golden à l'échelle de l'application : avec les vrais plugins du monde (le moteur de quartiers compris)
//! et sans show, donc sans gouverneur actif, la ville générée reste celle de Sillage (`tests/golden/scenery.json`),
//! pièce pour pièce. Le golden n'est jamais régénéré : s'il diverge, c'est le moteur de quartiers qui a tort.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AudioFeatures, CorePlugin, DistrictState};
use cf_show::district::{DistrictId, DistrictKind};
use cf_world_neon::scenery::layout::{Row, SPACING_M};
use cf_world_neon::{ChasePlugin, Clearing, NeonWorld, NeonWorldPlugin, RunnerConfig, SceneryConfig};
use serde_json::Value;

/// Rangées derrière la voiture et devant elle : la fenêtre de Sillage (`BEHIND`, `ROWS`), comme `scenery_golden.rs`.
const BEHIND: i64 = 15;
const ROWS: i64 = 34;
/// Tolérance sur les positions, en m (celle de `scenery_golden.rs`).
const TOL_M: f64 = 0.02;
/// Une minute de route à 60 i/s : assez pour que le gouverneur ait eu mille occasions de proposer un changement.
const FRAMES: usize = 3600;
const FRAME: Duration = Duration::from_micros(16_667);

fn num(v: &Value) -> f64 {
    v.as_f64().expect("nombre")
}

fn nums<const N: usize>(v: &Value) -> [f64; N] {
    let array = v.as_array().expect("tableau");
    assert_eq!(array.len(), N);
    std::array::from_fn(|i| num(&array[i]))
}

fn clearing(v: &Value) -> Clearing {
    Clearing {
        z0: num(&v["z0"]),
        z1: num(&v["z1"]),
        side: i8::try_from(v["side"].as_i64().expect("côté")).expect("côté"),
    }
}

/// La configuration de l'application, avec la qualité et les zones dégagées du scénario (le plan de quartiers reste
/// celui que l'application a produit).
fn config_for(app_config: &SceneryConfig, case: &Value) -> SceneryConfig {
    let mut config = app_config.clone();
    config.set_quality(usize::try_from(case["quality"].as_u64().expect("qualité")).expect("qualité"));
    let clearings: Vec<Clearing> = case["clearings"]
        .as_array()
        .expect("zones")
        .iter()
        .map(clearing)
        .collect();
    config.set_clearings(&clearings);
    config.set_sign_clearing(
        case["signClearing"]
            .as_object()
            .map(|_| clearing(&case["signClearing"])),
    );
    config
}

/// Une pièce : rangée, position absolue, taille, teinte, graine.
type Seen = (i64, [f64; 3], [f64; 3], u8, f64);

fn key(s: &Seen) -> (i64, i64, i64, i64) {
    let r = |v: f64| (v * 100.0).round() as i64;
    (s.0, r(s.1[0]), r(s.1[1]), r(s.1[2]))
}

fn ours(config: &SceneryConfig, base: i64) -> Vec<Seen> {
    let mut parts: Vec<Seen> = (base..base + ROWS)
        .flat_map(|index| {
            Row::generate(index, config)
                .parts()
                .iter()
                .map(|p| {
                    (
                        index,
                        [
                            f64::from(p.x),
                            f64::from(p.y),
                            -(index as f64) * SPACING_M + f64::from(p.z),
                        ],
                        p.size.map(f64::from),
                        p.tint,
                        f64::from(p.seed),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    parts.sort_by_key(key);
    parts
}

fn theirs(case: &Value) -> Vec<Seen> {
    let mut parts: Vec<Seen> = case["parts"]
        .as_array()
        .expect("pièces")
        .iter()
        .map(|p| {
            (
                p["row"].as_i64().expect("rangée"),
                nums(&p["pos"]),
                nums(&p["size"]),
                u8::try_from(p["tint"].as_u64().expect("teinte")).expect("teinte"),
                nums::<4>(&p["info"])[0],
            )
        })
        .collect();
    parts.sort_by_key(key);
    parts
}

#[test]
fn golden_app_inchange() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
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
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    for _ in 0..FRAMES {
        app.update();
    }

    // Sans show, le gouverneur ne propose rien : centre néon, aucune cible.
    let neon = DistrictId {
        kind: DistrictKind::Neon,
        variant: 0,
    };
    let state = *app.world().resource::<DistrictState>();
    assert_eq!(state.current, neon);
    assert_eq!(state.target, None);
    let world = app.world().resource::<NeonWorld>();
    let app_config = world.scenery().clone();
    assert!(app_config.districts_active(), "le moteur de quartiers est branché");

    let data: Value = serde_json::from_str(include_str!("golden/scenery.json")).expect("scenery.json lisible");
    let cases = data["cases"].as_array().expect("scénarios");
    assert!(cases.len() >= 8);
    for case in cases {
        let car_z = num(&case["carZ"]);
        let base = (-car_z / SPACING_M).floor() as i64 - BEHIND;
        assert_eq!(base, case["base"].as_i64().expect("base"));
        let (a, b) = (ours(&config_for(&app_config, case), base), theirs(case));
        let context = format!("carZ {car_z}, qualité {}", case["quality"]);
        assert_eq!(a.len(), b.len(), "{context} : nombre de pièces");
        for (x, y) in a.iter().zip(&b) {
            let at = format!("{context}, rangée {}", y.0);
            assert_eq!(x.0, y.0, "{at}");
            for axis in 0..3 {
                assert!(
                    (x.1[axis] - y.1[axis]).abs() <= TOL_M,
                    "{at} : position {:?} ≠ {:?}",
                    x.1,
                    y.1
                );
                assert!(
                    (x.2[axis] - y.2[axis]).abs() <= TOL_M,
                    "{at} : taille {:?} ≠ {:?}",
                    x.2,
                    y.2
                );
            }
            assert_eq!(x.3, y.3, "{at} : teinte");
            assert!((x.4 - y.4).abs() <= TOL_M, "{at} : graine {} ≠ {}", x.4, y.4);
        }
    }
}
