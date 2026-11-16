//! Tests de l'éclairage de scène (`light.rs`).

use super::super::formula;
use super::super::modulation::{FogInput, FogModulation, FogTuning};
use super::*;

fn colors() -> LightColors {
    LightColors {
        velvet: Vec3::new(0.03, 0.01, 0.09),
        mood: Vec3::new(0.25, 0.1, 0.8),
        ice: Vec3::new(0.8, 0.9, 1.0),
        azure: Vec3::new(0.1, 0.4, 0.9),
    }
}

fn input(density: f32) -> LightInput {
    let fog = FogModulation::default().step(
        &FogTuning::default(),
        &FogInput {
            dt_s: 1.0 / 60.0,
            ..FogInput::default()
        },
    );
    LightInput {
        density,
        fog,
        kick_ages: [60.0; 4],
        source: DVec3::new(0.0, 0.5, -40_000.0),
        tubes: [Tube::default(); TUBES],
        spectrum: [0.7; SPECTRUM_BANDS],
        surface: Surface {
            puddles: 0.6,
            neon_saturation: 0.4,
            energy_slow: 0.1,
            detail: 1.0,
        },
    }
}

#[test]
fn le_rapport_principale_remplissage_va_de_8_a_16() {
    let calm = compose(&LightTuning::default(), &colors(), &input(0.0));
    let peak = compose(&LightTuning::default(), &colors(), &input(1.0));
    assert!((luma(calm.c_key) / luma(calm.c_fill) - 8.0).abs() < 0.01);
    assert!((luma(peak.c_key) / luma(peak.c_fill) - 16.0).abs() < 0.02);
    assert!((calm.dir_key.length() - 1.0).abs() < 1e-6 && (calm.dir_rim.length() - 1.0).abs() < 1e-6);
}

#[test]
fn les_positions_du_bloc_sont_relatives_a_l_origine() {
    let light = compose(&LightTuning::default(), &colors(), &input(0.5));
    let p = light.params(DVec3::new(0.0, 0.0, -40_000.0), 4.0, 0.25);
    assert!((p.source.truncate() - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-4);
    assert!((p.fog.x - 0.0037).abs() < 1e-6 && (p.fog.w - 1.0).abs() < f32::EPSILON);
    assert!((luma(p.rayleigh.truncate()) - 1.0).abs() < 1e-5);
}

#[test]
fn respiration_la_coquille_de_kick_est_a_plat() {
    let mut i = input(0.5);
    i.fog.shell = false;
    let light = compose(&LightTuning::default(), &colors(), &i);
    assert!(light.shell.depth.abs() < f32::EPSILON);
    // Et le vu-mètre des façades est à plat ; hors respiration il passe tel quel dans le bloc.
    assert!(light.spectrum.iter().all(|v| v.abs() < f32::EPSILON));
    // Les flaques ne frémissent plus et les néons ne suivent plus le haut-médium ; calme et détail ne pulsent pas.
    let quiet = light.params(DVec3::ZERO, 4.0, 0.25).surface;
    assert!(quiet.x.abs() < f32::EPSILON && quiet.y.abs() < f32::EPSILON);
    assert!((quiet.z - 0.6).abs() < 1e-6 && (quiet.w - 1.0).abs() < f32::EPSILON);
    let live = compose(&LightTuning::default(), &colors(), &input(0.5)).params(DVec3::ZERO, 4.0, 0.25);
    assert!(
        live.spectrum
            .iter()
            .all(|q| (q.x - 0.7).abs() < 1e-6 && (q.w - 0.7).abs() < 1e-6)
    );
}

/// DA §1.2, règles 1 et 2, sur la formule. La ville est faite de corps sombres piqués de fenêtres : c'est la brume
/// qui sépare les plans, du plus sombre (proche) au plus clair (lointain). Les corps éteints de P3 (20 m), P2
/// (150 m) et P1 (900 m) sont séparés d'au moins 0,12 en luminance affichée (après la courbe de transfert de
/// l'écran), et une silhouette d'horizon reste **sous** la brume qui est derrière elle.
#[test]
fn les_plans_sont_separes_en_valeur() {
    let light = compose(&LightTuning::default(), &colors(), &input(0.5));
    let cam = Vec3::new(0.0, 2.0, 0.0);
    let shade = |surface: Vec3, dist: f32, y: f32| {
        let p = Vec3::new(20.0, y, -dist);
        let ray = p - cam;
        let d = ray.length();
        let dir = ray / d;
        let tau = formula::optical_depth(light.fog.sigma_rho, light.fog.height_m, 0.0, cam, dir, d);
        let t = formula::transmittance(tau);
        let scatter = light.c_key * formula::phase_hg(dir.dot(light.dir_key), light.fog.g)
            + light.c_rim * formula::phase_hg(dir.dot(light.dir_rim), light.fog.g);
        let mist = light.c_fog + scatter * light.scatter;
        luma(formula::aerial(surface, mist, t))
    };
    let display = |l: f32| l.max(0.0).powf(1.0 / 2.2);
    let body = Vec3::new(0.004, 0.003, 0.008);
    let (p3, p2, p1) = (
        display(shade(body, 20.0, 8.0)),
        display(shade(body, 150.0, 8.0)),
        display(shade(body, 900.0, 8.0)),
    );
    assert!(p2 - p3 >= 0.12 && p1 - p2 >= 0.12, "P3 {p3}, P2 {p2}, P1 {p1}");
    let silhouette = shade(Vec3::new(0.004, 0.003, 0.008), 700.0, 40.0);
    let fog_behind = shade(Vec3::ZERO, 5000.0, 40.0);
    assert!(silhouette < fog_behind, "silhouette {silhouette}, brume {fog_behind}");
}
