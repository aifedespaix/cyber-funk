//! Les projecteurs du décor et le paquet du drop à l'écran : faisceaux accrochés aux façades, onde de choc et gerbe
//! d'étincelles au drop.
//!
//! ```text
//! cargo run -p cf-world-neon --example beams_demo --features demo                 # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/faisceaux cargo run ...                          # scène scriptée, verdicts, captures
//! CF_CHECK=1 CF_FLASHES=0 CF_SNAPSHOT_DIR=/tmp/faisceaux-coupes cargo run ...      # flashs coupés : le drop ne montre rien
//! ```
//!
//! Interactif : `D` lance un drop (le paquet n'existe que sur cette image, comme `drop_now`), `F` coupe ou rend les flashs
//! (le limiteur les refuse alors tous), flèches pour orienter la caméra.
//!
//! Avec `CF_CHECK=1` : des captures avec et sans les faisceaux, puis juste avant le drop, **sur l'image du drop** et
//! quelques images plus tard ; des verdicts sur les pixels, sur les shaders (un shader invalide ne dessinerait rien : la
//! capture ne changerait pas) et sur la règle 8 (aucun pipeline compilé après `Ready`). Le journal ne doit contenir aucune
//! erreur de shader : `grep -iE "error|naga|shader" journal.txt`.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
#[path = "../road_demo/shots.rs"]
#[allow(
    dead_code,
    reason = "module partagé avec road_demo : beams_demo n'en utilise qu'une partie"
)]
mod shots;

use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, CfSet, CorePlugin, CullingPlugin, CurrentShowFx, DiagnosticsPlugin,
    LayersPlugin, ShowFxInput, StreamFocus, WarmupPlugin,
};
use cf_show::drop::DropFxPackage;
use cf_world_neon::NeonWorldPlugin;

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// La voiture (point suivi), au départ du monde.
pub(crate) const CAR: DVec3 = DVec3::new(0.0, 0.0, -300.0);
const TURN_RAD_S: f32 = 0.9;
const BPM: f32 = 128.0;
const PULSE_DECAY_S: f32 = 0.12;

/// Le paquet d'un drop de niveau élevé : trois ondes, une belle gerbe.
pub(crate) const PACKAGE: DropFxPackage = DropFxPackage {
    flash: 0.6,
    rgb_split: 0.4,
    city_pulse: 0.8,
    shockwaves: 3,
    burst: 60,
};

/// Les effets du spectacle de la démo : flashs permis ou coupés, et le paquet du drop sur cette seule image si demandé.
pub(crate) fn show_fx(flashes: bool, drop: Option<DropFxPackage>) -> CurrentShowFx {
    CurrentShowFx(ShowFxInput {
        flashes,
        breathing: false,
        heat: 0.5,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop,
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
    let flashes = !std::env::var("CF_FLASHES").is_ok_and(|v| v == "0");
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · projecteurs et drop".into(),
            resolution: WindowResolution::new(SIZE.0, SIZE.1),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins((
        CorePlugin,
        WarmupPlugin,
        LayersPlugin::default(),
        CullingPlugin::default(),
        DiagnosticsPlugin::default(),
        // `NeonWorldPlugin` installe `BeamsPlugin` et `DropFxPlugin`.
        NeonWorldPlugin::embedded(),
    ))
    .init_resource::<CameraPose>()
    .init_resource::<View>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(show_fx(flashes, None))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn((AbsolutePosition(CAR), StreamFocus));
    });
    if check {
        app.insert_resource(check::Check::new(flashes))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.in_set(CfSet::Input))
            .add_systems(Update, check::hide_beams.after(CfSet::Present));
    } else {
        app.add_systems(Update, (beat, keys).in_set(CfSet::Input))
            .add_systems(Update, apply_view.in_set(CfSet::Direct));
    }
    app.run();
}

/// Kick régulier : de quoi faire vivre les faisceaux.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let t = time.elapsed_secs();
    let beat_s = 60.0 / BPM;
    let pulse = (-(t % beat_s) / PULSE_DECAY_S).exp();
    features.0.pulse = pulse;
    features.0.kick = (t % beat_s) < time.delta_secs();
    features.0.mid = 0.4;
    features.0.beat = f64::from(t / beat_s);
}

/// Touches : `D`, `F`, flèches.
fn keys(keyboard: Res<ButtonInput<KeyCode>>, time: Res<Time>, mut fx: ResMut<CurrentShowFx>, mut view: ResMut<View>) {
    let dt = time.delta_secs();
    let axis =
        |plus: KeyCode, minus: KeyCode| f32::from(i8::from(keyboard.pressed(plus)) - i8::from(keyboard.pressed(minus)));
    view.yaw -= axis(KeyCode::ArrowRight, KeyCode::ArrowLeft) * TURN_RAD_S * dt;
    view.pitch = (view.pitch + axis(KeyCode::ArrowUp, KeyCode::ArrowDown) * TURN_RAD_S * dt).clamp(-1.4, 1.4);
    let flashes = if keyboard.just_pressed(KeyCode::KeyF) {
        !fx.0.flashes
    } else {
        fx.0.flashes
    };
    // Le paquet n'existe que sur l'image où la touche est pressée : c'est ce que fait `drop_now`.
    *fx = show_fx(flashes, keyboard.just_pressed(KeyCode::KeyD).then_some(PACKAGE));
}

/// La caméra de l'interactif : derrière la voiture, haute, vers la route.
fn apply_view(view: Res<View>, mut pose: ResMut<CameraPose>) {
    *pose = CameraPose {
        position: CAR + DVec3::new(6.0, 7.0, 16.0),
        rotation: Quat::from_euler(EulerRot::YXZ, view.yaw, view.pitch - 0.2, 0.0),
        ..CameraPose::default()
    };
}
