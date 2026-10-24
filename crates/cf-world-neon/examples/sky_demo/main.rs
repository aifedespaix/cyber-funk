//! Le ciel de Neon City à l'écran : voûte, étoiles, aurores, astres, géante à anneau, étoiles filantes.
//!
//! ```text
//! cargo run -p cf-world-neon --example sky_demo --features demo                 # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/ciel cargo run ...                            # scènes scriptées, verdicts, captures
//! ```
//!
//! Interactif (la couche monde est éteinte : on ne voit que l'arrière-plan) : flèches pour orienter la caméra, `G` la
//! tourne vers la géante à anneau, `D` fait « voyager » le ciel jusqu'à elle (`sky_drift`), `K` envoie un kick, `S`
//! lance une étoile filante, `R` rejoue le fondu de la pré-intro, `W` rallume la ville ; `CF_FAR=1` place la caméra au km 400.
//!
//! Avec `CF_CHECK=1` : des scènes scriptées, une capture à chacune et des verdicts sur les pixels, entre autres que **le
//! ciel est identique au départ et à 400 km** (il est à l'infini) et qu'il grossit quand `sky_drift` tend vers la géante.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
#[path = "../road_demo/shots.rs"]
#[allow(
    dead_code,
    reason = "module partagé avec road_demo : sky_demo n'en utilise qu'une partie"
)]
mod shots;

use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{AudioFeatures, CameraPose, CfSet, CorePlugin, CullingPlugin, Layer, LayerOpacity, LayersPlugin};
use cf_world_neon::{NeonWorldPlugin, RingPlanet, ShootingStarRequest, SkyLook};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// Cadence du kick de l'interactif, en battements par minute.
const BPM: f32 = 128.0;
/// Retombée du pouls (s) après un kick.
const PULSE_DECAY_S: f32 = 0.12;
/// Vitesse de rotation de la caméra aux flèches, en rad/s.
const TURN_RAD_S: f32 = 0.9;

/// L'orientation de la caméra du spectacle de la démo.
#[derive(Resource, Debug, Default)]
struct View {
    yaw: f32,
    pitch: f32,
    /// Part du chemin vers la géante à anneau parcourue par `sky_drift`, 0..1.
    drift: f32,
}

/// Direction (unitaire) vers `target`, en lacet et tangage de la caméra.
pub(crate) fn look_at(target: Vec3) -> (f32, f32) {
    let d = target.normalize();
    ((-d.x).atan2(-d.z), d.y.asin())
}

/// La pose du ciel de la démo : orientation, position lointaine éventuelle, dérive vers la géante.
pub(crate) fn pose_for(yaw: f32, pitch: f32, z_m: f64, drift: f32, giant: Option<&RingPlanet>) -> CameraPose {
    CameraPose {
        position: DVec3::new(0.0, 2.0, z_m),
        rotation: Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0),
        // « La caméra tend vers le centre de la planète, en s'arrêtant à un rayon et demi de sa surface » : même
        // consigne que les plans de voyage de Sillage (`sky-trip.ts`).
        sky_drift: giant.map_or(Vec3::ZERO, |g| {
            let stop = g.center - g.center.normalize() * (g.radius * 2.5);
            stop * drift
        }),
        ..CameraPose::default()
    }
}

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · ciel".into(),
            resolution: WindowResolution::new(SIZE.0, SIZE.1),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins((
        CorePlugin,
        LayersPlugin::default(),
        CullingPlugin::default(),
        NeonWorldPlugin::embedded(),
    ))
    .init_resource::<CameraPose>()
    .init_resource::<View>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .add_systems(Startup, |mut opacity: ResMut<LayerOpacity>| {
        // Que le ciel : la ville, la route et leurs lumières sont éteintes.
        opacity.set(Layer::World, 0.0);
    });
    if check {
        app.init_resource::<check::Check>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.in_set(CfSet::Direct));
    } else {
        app.add_systems(Update, (keys, beat, apply_view).chain().in_set(CfSet::Input));
    }
    app.run();
}

/// Touches : flèches, `G`, `D`, `K`, `S`, `R`, `W`.
#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
fn keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    giant: Option<Res<RingPlanet>>,
    mut view: ResMut<View>,
    mut look: ResMut<SkyLook>,
    mut opacity: ResMut<LayerOpacity>,
    mut features: ResMut<AudioFeatures>,
    mut shoot: MessageWriter<ShootingStarRequest>,
    mut reveal: Local<Option<f32>>,
) {
    let dt = time.delta_secs();
    let axis =
        |plus: KeyCode, minus: KeyCode| f32::from(i8::from(keyboard.pressed(plus)) - i8::from(keyboard.pressed(minus)));
    view.yaw -= axis(KeyCode::ArrowRight, KeyCode::ArrowLeft) * TURN_RAD_S * dt;
    view.pitch = (view.pitch + axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * TURN_RAD_S * dt).clamp(-1.4, 1.4);
    if keyboard.just_pressed(KeyCode::KeyG)
        && let Some(giant) = giant.as_deref()
    {
        (view.yaw, view.pitch) = look_at(giant.center);
    }
    if keyboard.just_pressed(KeyCode::KeyD) {
        view.drift = if view.drift > 0.5 { 0.0 } else { 1.0 };
    }
    if keyboard.just_pressed(KeyCode::KeyK) {
        features.0.pulse = 1.0;
    }
    if keyboard.just_pressed(KeyCode::KeyS) {
        shoot.write(ShootingStarRequest);
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        *reveal = Some(0.0);
    }
    if keyboard.just_pressed(KeyCode::KeyW) {
        let on = opacity.get(Layer::World) > 0.5;
        opacity.set(Layer::World, if on { 0.0 } else { 1.0 });
    }
    if let Some(t) = reveal.as_mut() {
        *t += dt / 5.0;
        look.reveal = t.min(1.0);
        if *t >= 1.0 {
            *reveal = None;
        }
    }
}

/// Kick régulier : le pouls retombe entre deux.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let t = time.elapsed_secs();
    let beat_s = 60.0 / BPM;
    features.0.pulse = features.0.pulse.max((-(t % beat_s) / PULSE_DECAY_S).exp()) * 0.97;
}

/// Écrit la pose du ciel d'après l'orientation de la démo.
fn apply_view(view: Res<View>, giant: Option<Res<RingPlanet>>, mut pose: ResMut<CameraPose>) {
    let z = if std::env::var_os("CF_FAR").is_some() {
        -400_000.0
    } else {
        0.0
    };
    *pose = pose_for(view.yaw, view.pitch, z, view.drift, giant.as_deref());
}
