//! La ville de Neon City contre celle de Sillage : le code TypeScript d'origine (`scenery.ts`, `bridges.ts`, `random.ts`)
//! a rempli `tests/golden/scenery.json` (voir `tools/sillage-golden/scenery/`) ; on vérifie que le générateur Rust place
//! les mêmes pièces, aux mêmes endroits et aux mêmes tailles, avec les mêmes boîtes de collision, points d'attache de
//! projecteurs et réponses à `collides`, `roofAt` et `facade`.

#![allow(clippy::expect_used, clippy::panic)]

use bevy::math::DVec3;
use cf_world_neon::scenery::layout::{BASE_DEPTH_M, FLOOR_HEIGHT_M, Row, SPACING_M, Tier};
use cf_world_neon::scenery::query::{collides, facade, roof_at};
use cf_world_neon::{Clearing, SceneryConfig};
use serde_json::Value;

/// Rangées derrière la voiture et devant elle (34 en tout) : la fenêtre de Sillage (`BEHIND`, `ROWS`).
const BEHIND: i64 = 15;
const ROWS: i64 = 34;
/// Tolérance sur les positions, en m : Sillage range ses boîtes en `f32`, à 60 km l'ulp de z vaut ~4 mm.
const TOL_M: f64 = 0.02;

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/scenery.json")).expect("scenery.json lisible")
}

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

fn config_of(case: &Value) -> SceneryConfig {
    let mut config = SceneryConfig::default();
    config.set_quality(usize::try_from(case["quality"].as_u64().expect("qualité")).expect("qualité"));
    let clearings: Vec<Clearing> = case["clearings"].as_array().expect("zones").iter().map(clearing).collect();
    config.set_clearings(&clearings);
    config.set_sign_clearing(case["signClearing"].as_object().map(|_| clearing(&case["signClearing"])));
    config
}

fn close(a: f64, b: f64, what: &str, context: &str) {
    assert!((a - b).abs() <= TOL_M, "{context} : {what} {a} ≠ {b}");
}

/// Une pièce vue de l'extérieur : rangée, position absolue, taille, teinte, et les trois champs de `aInfo`.
#[derive(Debug)]
struct Seen {
    row: i64,
    pos: [f64; 3],
    size: [f64; 3],
    tint: u8,
    info: [f64; 4],
}

fn key(s: &Seen) -> (i64, i64, i64, i64) {
    let r = |v: f64| (v * 100.0).round() as i64;
    (s.row, r(s.pos[0]), r(s.pos[1]), r(s.pos[2]))
}

fn rust_parts(config: &SceneryConfig, base: i64) -> Vec<Seen> {
    let mut parts = Vec::new();
    for index in base..base + ROWS {
        for part in Row::generate(index, config).parts() {
            let spot = if part.spot_floor > 0 {
                (f64::from(part.spot_floor) - 0.5) * f64::from(FLOOR_HEIGHT_M)
            } else {
                -1.0
            };
            // Le décalage vertical du bloc : le corps commence sous la route, la couronne prolonge ses étages.
            let offset = match part.tier {
                Tier::Body | Tier::Far => -f64::from(BASE_DEPTH_M),
                Tier::Crown => f64::from(part.y),
                Tier::Antenna => 0.0,
            };
            parts.push(Seen {
                row: index,
                pos: [f64::from(part.x), f64::from(part.y), -(index as f64) * SPACING_M + f64::from(part.z)],
                size: part.size.map(f64::from),
                tint: part.tint,
                info: [f64::from(part.seed), spot, f64::from(part.tier as u8), offset],
            });
        }
    }
    parts.sort_by_key(key);
    parts
}

fn golden_parts(case: &Value) -> Vec<Seen> {
    let mut parts: Vec<Seen> = case["parts"]
        .as_array()
        .expect("pièces")
        .iter()
        .map(|p| Seen {
            row: p["row"].as_i64().expect("rangée"),
            pos: nums(&p["pos"]),
            size: nums(&p["size"]),
            tint: u8::try_from(p["tint"].as_u64().expect("teinte")).expect("teinte"),
            info: nums(&p["info"]),
        })
        .collect();
    parts.sort_by_key(key);
    parts
}

#[test]
fn les_pieces_sont_celles_de_sillage() {
    let data = golden();
    let cases = data["cases"].as_array().expect("scénarios");
    assert!(cases.len() >= 8);
    for case in cases {
        let car_z = num(&case["carZ"]);
        let base = case["base"].as_i64().expect("base");
        assert_eq!(base, (-car_z / SPACING_M).floor() as i64 - BEHIND);
        let config = config_of(case);
        let (ours, theirs) = (rust_parts(&config, base), golden_parts(case));
        let context = format!("carZ {car_z}, qualité {}", case["quality"]);
        assert_eq!(ours.len(), theirs.len(), "{context} : nombre de pièces");
        for (a, b) in ours.iter().zip(&theirs) {
            let at = format!("{context}, rangée {}", b.row);
            assert_eq!(a.row, b.row, "{at}");
            for axis in 0..3 {
                close(a.pos[axis], b.pos[axis], "position", &at);
                close(a.size[axis], b.size[axis], "taille", &at);
            }
            assert_eq!(a.tint, b.tint, "{at} : teinte");
            // Graine, étage du projecteur, type, décalage vertical.
            for field in 0..4 {
                close(a.info[field], b.info[field], "aInfo", &at);
            }
        }
    }
}

#[test]
fn les_boites_et_les_projecteurs_sont_ceux_de_sillage() {
    let data = golden();
    for case in data["cases"].as_array().expect("scénarios") {
        let base = case["base"].as_i64().expect("base");
        let config = config_of(case);
        let context = format!("carZ {}", case["carZ"]);
        let rows: Vec<Row> = (base..base + ROWS).map(|i| Row::generate(i, &config)).collect();

        let boxes: Vec<_> = rows.iter().flat_map(|r| r.hitboxes()).collect();
        let theirs = case["hitboxes"].as_array().expect("boîtes");
        assert_eq!(boxes.len(), theirs.len(), "{context} : nombre de boîtes");
        for (a, b) in boxes.iter().zip(theirs) {
            let [min_x, max_x, min_z, max_z, top] = nums::<5>(b);
            close(f64::from(a.x[0]), min_x, "x min", &context);
            close(f64::from(a.x[1]), max_x, "x max", &context);
            close(a.z[0], min_z, "z min", &context);
            close(a.z[1], max_z, "z max", &context);
            close(f64::from(a.top), top, "sommet", &context);
        }

        let spots: Vec<_> = rows.iter().flat_map(|r| r.spots()).collect();
        let theirs = case["spots"].as_array().expect("projecteurs");
        assert_eq!(spots.len(), theirs.len(), "{context} : nombre de projecteurs");
        for (a, b) in spots.iter().zip(theirs) {
            close(f64::from(a.x), num(&b["x"]), "x", &context);
            close(f64::from(a.y), num(&b["y"]), "y", &context);
            close(a.z, num(&b["z"]), "z", &context);
            close(f64::from(a.phase), num(&b["phase"]), "phase", &context);
            assert_eq!(i64::from(a.side), b["side"].as_i64().expect("côté"), "{context}");
            assert_eq!(u64::from(a.tint), b["tint"].as_u64().expect("teinte"), "{context}");
        }
    }
}

#[test]
fn collisions_toits_et_facades_repondent_comme_sillage() {
    let data = golden();
    let mut hits = 0;
    for case in data["cases"].as_array().expect("scénarios") {
        let config = config_of(case);
        let context = format!("carZ {}", case["carZ"]);
        for q in case["queries"].as_array().expect("questions") {
            let [x, y, z] = nums(&q["at"]);
            let radius = num(&q["radius"]);
            let at = format!("{context}, en ({x:.2}, {y:.2}, {z:.2}), rayon {radius}");
            let expected = q["collides"].as_bool().expect("booléen");
            hits += usize::from(expected);
            assert_eq!(collides(&config, DVec3::new(x, y, z), radius), expected, "collides {at}");

            // Sillage répond −1 quand il n'y a pas de toit.
            let roof = num(&q["roof"]);
            match roof_at(&config, x, z) {
                Some(top) => close(f64::from(top), roof, "toit", &at),
                None => assert!(roof < 0.0, "toit {at} : Sillage en voit un ({roof})"),
            }

            let side = i8::try_from(q["side"].as_i64().expect("côté")).expect("côté");
            let depth = num(&q["depth"]);
            match (facade(&config, side, z, depth), q["facade"].as_f64()) {
                (Some(ours), Some(theirs)) => close(f64::from(ours), theirs, "façade", &at),
                (None, None) => {}
                (ours, theirs) => panic!("façade {at} : {ours:?} ≠ {theirs:?}"),
            }
        }
    }
    assert!(hits > 50, "le jeu de questions touche des tours ({hits})");
}
