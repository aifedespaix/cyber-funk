//! Les astres qui tombent à l'écran : vol, queue et halo, explosion contre la ville (éclair, onde de choc, gerbe), anneau
//! de brume.
//!
//! ```text
//! cargo run -p cf-world-neon --example meteor_demo --features demo                 # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/meteores cargo run ...                           # scènes scriptées, verdicts, captures
//! CF_CHECK=1 CF_CITY=0 CF_SNAPSHOT_DIR=/tmp/meteores-brume cargo run ...           # sans ville : l'anneau de brume
//! ```
//!
//! Interactif : `M` lance un astre, `P` une pluie de douze, `F` coupe ou rend les flashs (le limiteur les refuse alors tous),
//! flèches pour orienter la caméra.
//!
//! Avec `CF_CHECK=1` : un astre est lancé et suivi, puis la caméra regarde son explosion (ou son anneau de brume sans
//! ville) ; une capture à chaque étape, des verdicts sur les pixels et sur le niveau du limiteur de flashs.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
#[path = "../road_demo/shots.rs"]
#[allow(
    dead_code,
    reason = "module partagé avec road_demo : meteor_demo n'en utilise qu'une partie"
)]
mod shots;

use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, CfSet, CorePlugin, CullingPlugin, CurrentShowFx, LayersPlugin,
    ShowFxInput, StreamFocus,
};
use cf_world_neon::{Meteors, NeonWorldPlugin};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// La voiture (point suivi), au départ du monde.
pub(crate) const CAR: DVec3 = DVec3::new(0.0, 0.0, -300.0);
const TURN_RAD_S: f32 = 0.9;

/// Les effets du spectacle de la démo : flashs permis ou coupés.
pub(crate) fn show_fx(flashes: bool) -> CurrentShowFx {
    CurrentShowFx(ShowFxInput {
        flashes,
        breathing: false,
        heat: 0.5,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    })
}

/// La pose d'une caméra en `eye` qui regarde `target` (absolus).
pub(crate) fn pose_to(eye: DVec3, target: DVec3) -> CameraPose {
    let d = (target - eye).as_vec3().normalize_or(Vec3::NEG_Z);
    CameraPose {
        position: eye,
        rotation: Quat::from_euler(EulerRot::YXZ, (-d.x).atan2(-d.z), d.y.asin(), 0.0),
        ..CameraPose::default()
    }
}

#[derive(Resource, Debug, Default)]
struct View {
    yaw: f32,
    pitch: f32,
}

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let city = !std::env::var("CF_CITY").is_ok_and(|v| v == "0");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · astres".into(),
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
        if city {
            NeonWorldPlugin::embedded()
        } else {
            NeonWorldPlugin::embedded().without_city()
        },
    ))
    .init_resource::<CameraPose>()
    .init_resource::<View>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(show_fx(true))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn((AbsolutePosition(CAR), StreamFocus));
    });
    if check {
        app.insert_resource(check::Check::new(city))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.in_set(CfSet::Direct));
    } else {
        app.add_systems(Update, keys.in_set(CfSet::Input))
            .add_systems(Update, apply_view.in_set(CfSet::Direct));
    }
    app.run();
}

/// Touches : `M`, `P`, `F`, flèches.
fn keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    textures: Option<Res<cf_world_neon::sky::PlanetTextures>>,
    mut meteors: ResMut<Meteors>,
    mut fx: ResMut<CurrentShowFx>,
    mut view: ResMut<View>,
) {
    let dt = time.delta_secs();
    let axis =
        |plus: KeyCode, minus: KeyCode| f32::from(i8::from(keyboard.pressed(plus)) - i8::from(keyboard.pressed(minus)));
    view.yaw -= axis(KeyCode::ArrowRight, KeyCode::ArrowLeft) * TURN_RAD_S * dt;
    view.pitch = (view.pitch + axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * TURN_RAD_S * dt).clamp(-1.4, 1.4);
    let count = textures.map_or(0, |t| t.0.len());
    if keyboard.just_pressed(KeyCode::KeyM) {
        meteors.launch(CAR, false, count);
    }
    if keyboard.just_pressed(KeyCode::KeyP) {
        meteors.start_shower(12, 5.0);
    }
    if keyboard.just_pressed(KeyCode::KeyF) {
        let on = fx.0.flashes;
        *fx = show_fx(!on);
    }
}

/// La caméra de l'interactif : derrière la voiture, un peu haute.
fn apply_view(view: Res<View>, mut pose: ResMut<CameraPose>) {
    *pose = CameraPose {
        position: CAR + DVec3::new(0.0, 14.0, 40.0),
        rotation: Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch + 0.18, 0.0),
        ..CameraPose::default()
    };
}
