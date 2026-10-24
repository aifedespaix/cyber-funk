//! Le ciel de Neon City contre celui de Sillage : le code TypeScript d'origine (`sky.ts`, `planets.ts`,
//! `shooting-stars.ts`) a rempli `tests/golden/sky.json` (voir `tools/sillage-golden/sky/`) ; on vérifie que le tirage
//! des étoiles, la place et la taille des astres et la géante à anneau sont les mêmes. (Les départs d'étoiles filantes
//! sont comparés dans `sky/shooting.rs`, qui a la main sur l'horloge de la cadence.)

#![allow(clippy::expect_used, clippy::panic)]

use cf_render::SceneColor;
use cf_world_neon::sky::{PLANETS, RING_PLANET_TEXTURE, RingPlanet, STAR_LAYERS, star_field};
use serde_json::Value;

/// Tolérance sur les positions, en m : Sillage les range en `f32`, à 1 000 m l'ulp vaut 0,06 mm, et le fichier est arrondi
/// à 0,1 mm.
const TOL_M: f32 = 0.02;

fn golden() -> Value {
    serde_json::from_str(include_str!("golden/sky.json")).expect("sky.json lisible")
}

fn num(v: &Value) -> f32 {
    v.as_f64().expect("nombre") as f32
}

fn near(a: f32, b: f32, tol: f32, what: &str) {
    assert!((a - b).abs() <= tol, "{what} : {a} contre {b}");
}

fn role_id(color: SceneColor) -> u64 {
    SceneColor::ALL.iter().position(|c| *c == color).expect("rôle") as u64
}

#[test]
fn les_etoiles_sont_celles_de_sillage() {
    let golden = golden();
    let stars = star_field();
    // Teintes dans l'ordre de `tints` de `sky.ts` : glace, glace, azur, menthe, ambre, corail.
    let tints = [
        SceneColor::Ice,
        SceneColor::Ice,
        SceneColor::Azure,
        SceneColor::Mint,
        SceneColor::Amber,
        SceneColor::Rose,
    ];
    let layers = golden["layers"].as_array().expect("couches");
    assert_eq!(layers.len(), STAR_LAYERS.len());
    let mut start = 0usize;
    let mut checked = 0;
    for (layer, spec) in layers.iter().zip(STAR_LAYERS) {
        assert_eq!(layer["count"].as_u64(), Some(u64::from(spec.count)));
        for sample in layer["samples"].as_array().expect("échantillons") {
            let i = usize::try_from(sample["i"].as_u64().expect("indice")).expect("indice");
            let star = stars[start + i];
            for (axis, value) in star.position.to_array().iter().enumerate() {
                near(
                    *value,
                    num(&sample["position"][axis]),
                    TOL_M,
                    &format!("étoile {i} position"),
                );
            }
            near(star.size_px, num(&sample["size"]), 1e-3, "taille");
            near(star.phase, num(&sample["phase"]), 1e-3, "phase");
            assert_eq!(
                sample["color"].as_u64(),
                Some(role_id(tints[usize::from(star.tint)])),
                "teinte de l'étoile {i}"
            );
            checked += 1;
        }
        start += spec.count as usize;
    }
    assert!(checked > 100, "{checked} étoiles comparées");
}

#[test]
fn les_astres_sont_ceux_de_sillage() {
    let golden = golden();
    let planets = golden["planets"].as_array().expect("astres");
    assert_eq!(planets.len(), PLANETS.len());
    for (planet, spec) in planets.iter().zip(PLANETS) {
        for (axis, value) in spec.center().to_array().iter().enumerate() {
            near(*value, num(&planet["center"][axis]), TOL_M, spec.texture);
        }
        near(spec.tilt_rad, num(&planet["tilt"]), 1e-4, "inclinaison");
        near(spec.radius_m, num(&planet["radius"]), 1e-4, "rayon");
        near(spec.rpm, num(&planet["rpm"]), 1e-4, "rpm");
    }
}

#[test]
fn la_geante_a_anneau_est_celle_de_sillage() {
    let golden = golden();
    let reference = &golden["ringPlanet"];
    let ring = PLANETS
        .iter()
        .find(|spec| spec.texture == RING_PLANET_TEXTURE)
        .and_then(RingPlanet::from_spec)
        .expect("la géante à anneau");
    for (axis, value) in ring.center.to_array().iter().enumerate() {
        near(*value, num(&reference["center"][axis]), TOL_M, "centre");
    }
    near(ring.radius, num(&reference["radius"]), 1e-3, "rayon");
    near(ring.ring_inner, num(&reference["ringInner"]), 1e-2, "anneau intérieur");
    near(ring.ring_outer, num(&reference["ringOuter"]), 1e-2, "anneau extérieur");
    for (axis, value) in ring.ring_normal.to_array().iter().enumerate() {
        near(*value, num(&reference["ringNormal"][axis]), 1e-3, "normale de l'anneau");
    }
    near(ring.ring_normal.length(), 1.0, 1e-5, "normale unitaire");
}
