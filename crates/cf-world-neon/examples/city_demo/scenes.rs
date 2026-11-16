//! Les scènes scriptées du mode `CF_CHECK=1` de `city_demo`, et le point de vue qu'une scène peut imposer.

use bevy::prelude::*;
use cf_render::AbsolutePosition;
use cf_world_neon::Runner;

/// Images laissées à une scène pour que l'origine, les transforms, les tronçons et le matériau soient à jour.
const SETTLE_FRAMES: u32 = 10;
/// Scènes d'atmosphère : assez long pour que la densité de la brume (constante de temps de 1,5 s) ait rejoint l'énergie
/// de la scène.
const ATMO_SETTLE_FRAMES: u32 = 240;

#[derive(Clone, Copy)]
pub(crate) struct Scene {
    pub(crate) name: &'static str,
    pub(crate) via_z: Option<f64>,
    pub(crate) car_z: f64,
    pub(crate) time_s: f64,
    pub(crate) pulse: f32,
    pub(crate) beat: f64,
    pub(crate) wet: f32,
    pub(crate) rain: f32,
    pub(crate) build: f32,
    /// Énergie lente de l'analyse (densité de la brume) et densité visuelle `Δ` (C3).
    pub(crate) intensity: f32,
    pub(crate) density: f32,
    /// Âge (s) du dernier kick, pour la coquille de vide de la brume (C3).
    pub(crate) kick_age: Option<f32>,
    /// Images laissées à la scène avant la capture.
    pub(crate) settle: u32,
    /// Point de vue imposé (C4) : décalage de la caméra par rapport au point suivi et direction du regard. `None` : la
    /// caméra de poursuite.
    pub(crate) view: Option<([f32; 3], [f32; 3])>,
    /// Frémissement des flaques (`Target::Puddles`), 0..1.
    pub(crate) puddles: f32,
}

const BASE: Scene = Scene {
    name: "",
    via_z: None,
    car_z: -1234.5,
    time_s: 3.0,
    pulse: 0.0,
    beat: 4.0,
    wet: 0.0,
    rain: 0.0,
    build: 1.0,
    intensity: 0.7,
    density: 0.65,
    kick_age: None,
    settle: SETTLE_FRAMES,
    view: None,
    puddles: 0.0,
};

pub(crate) const SCENES: [Scene; 15] = [
    Scene {
        name: "1-depart",
        ..BASE
    },
    Scene {
        name: "2-kick",
        pulse: 1.0,
        beat: 4.0,
        ..BASE
    },
    Scene {
        name: "3-pluie",
        wet: 1.0,
        rain: 0.8,
        time_s: 12.0,
        ..BASE
    },
    Scene {
        name: "4-revelation",
        build: 0.45,
        ..BASE
    },
    // À 110 m de la première arche (z = −1 203,4 m) : plus loin, la brume l'a déjà avalée (elle est pleine à 380 m).
    Scene {
        name: "5-arche",
        car_z: -1203.4 + 110.0,
        ..BASE
    },
    // Même point de vue de part et d'autre d'un recentrage d'origine (28 800 m) : la ville doit être identique.
    Scene {
        name: "6-seuil-direct",
        car_z: -14_000.0,
        ..BASE
    },
    Scene {
        name: "7-seuil-via-km30",
        via_z: Some(-30_000.0),
        car_z: -14_000.0,
        ..BASE
    },
    // Atmosphère (C3) : calme, sommet, et la coquille de vide d'un kick vieux de 0,45 s (front à 18 m du sujet).
    Scene {
        name: "8-atmo-calme",
        intensity: 0.2,
        density: 0.35,
        settle: ATMO_SETTLE_FRAMES,
        ..BASE
    },
    Scene {
        name: "9-atmo-sommet",
        intensity: 1.0,
        density: 1.0,
        settle: ATMO_SETTLE_FRAMES,
        pulse: 0.6,
        ..BASE
    },
    Scene {
        name: "10-atmo-coquille",
        kick_age: Some(0.45),
        ..BASE
    },
    // Sol mouillé (C4) : flaques, reflets, et l'anneau d'un kick vieux de 0,35 s sur l'eau.
    Scene {
        name: "11-mouille-kick",
        wet: 1.0,
        rain: 0.2,
        time_s: 12.0,
        kick_age: Some(0.35),
        ..BASE
    },
    // Fenêtres habitées, halo du sujet sur les murs, chanfrein de toit (C4) : la caméra quitte la poursuite et regarde
    // les façades de gauche, de près puis de biais (la parallaxe des pièces se lit sur les fenêtres en enfilade).
    Scene {
        name: "12-fenetres-face",
        view: Some(([5.0, 9.0, 0.0], [-1.0, 0.12, -0.25])),
        density: 1.0,
        intensity: 0.8,
        ..BASE
    },
    Scene {
        name: "13-fenetres-biais",
        car_z: -1560.0,
        view: Some(([-10.5, 2.4, -6.0], [-0.5, 0.25, -1.0])),
        density: 1.0,
        intensity: 0.8,
        ..BASE
    },
    // Rugosité réactive (C4) : le même sol mouillé pendant un break (flaques en miroir net) et sur un kick (reflets
    // élargis).
    Scene {
        name: "14-mouille-break",
        wet: 1.0,
        time_s: 12.0,
        intensity: 0.02,
        density: 0.3,
        settle: ATMO_SETTLE_FRAMES,
        ..BASE
    },
    Scene {
        name: "15-mouille-fremit",
        wet: 1.0,
        time_s: 12.0,
        puddles: 1.0,
        pulse: 0.8,
        ..BASE
    },
];

/// Le point de vue imposé par la scène en cours, lu par [`override_view`].
#[derive(Resource, Default, Clone, Copy)]
pub(crate) struct ForcedView(pub(crate) Option<([f32; 3], [f32; 3])>);

/// Entre `CfSet::Direct` et `CfSet::Present` : remplace la pose de la caméra de poursuite par celle de la scène.
pub(crate) fn override_view(
    forced: Res<ForcedView>,
    runner: Query<&AbsolutePosition, With<Runner>>,
    mut pose: ResMut<cf_render::CameraPose>,
) {
    let (Some((offset, dir)), Ok(car)) = (forced.0, runner.single()) else {
        return;
    };
    pose.position = car.0 + Vec3::from_array(offset).as_dvec3();
    pose.rotation = Transform::IDENTITY.looking_to(Vec3::from_array(dir), Vec3::Y).rotation;
    pose.fov_deg = 60.0;
}
