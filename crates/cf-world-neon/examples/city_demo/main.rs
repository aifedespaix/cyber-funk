//! La ville de Neon City à l'écran, filmée par la caméra de poursuite provisoire.
//!
//! ```text
//! cargo run -p cf-world-neon --example city_demo --features demo                 # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/ville cargo run ...                            # scènes scriptées, verdicts, captures
//! ```
//!
//! Interactif : un kick à 128 BPM fait pulser les façades, l'intensité passe lentement du calme au sommet. `W` mouille
//! les façades, `R` lance la pluie, `B` rejoue la révélation de la pré-intro, `Q` change de niveau de qualité (0 à 3).
//!
//! Avec `CF_CHECK=1` : des scènes scriptées (départ, kick, pluie, pré-intro, arche, recentrage d'origine), une capture à
//! chacune, les compteurs de culling de la surcouche de diagnostic (`counters.json`) et des verdicts sur les pixels, entre
//! autres que **la ville est identique de part et d'autre d'un recentrage d'origine**.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
#[path = "../road_demo/shots.rs"]
#[allow(
    dead_code,
    reason = "module partagé avec road_demo : city_demo n'en utilise qu'une partie"
)]
mod shots;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{AudioFeatures, CfSet, ChunkStream, CorePlugin, CullingPlugin, DiagnosticsPlugin, LayersPlugin};
use cf_world_neon::{ChasePlugin, FacadeLook, NeonWorld, NeonWorldPlugin, RoadLook, RunnerConfig};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
const BPM: f32 = 128.0;
const PULSE_DECAY_S: f32 = 0.12;
const INTENSITY_PERIOD_S: f32 = 40.0;

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · ville".into(),
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
        DiagnosticsPlugin::default(),
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: 0.0,
                // Scènes de capture : le point suivi reste où le script le met.
                speed_mps: if check {
                    0.0
                } else {
                    cf_world_neon::chase::CRUISE_SPEED_MPS
                },
            },
        },
    ))
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL));
    if check {
        app.init_resource::<check::Check>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.after(CfSet::Present));
    } else {
        app.add_systems(Update, (keys, beat).chain().in_set(CfSet::Input));
    }
    app.run();
}

/// Touches : `W` mouillé, `R` pluie, `B` révélation, `Q` qualité.
#[allow(clippy::too_many_arguments, reason = "un exemple : chaque réglage a sa ressource")]
fn keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut road: ResMut<RoadLook>,
    mut look: ResMut<FacadeLook>,
    mut world: ResMut<NeonWorld>,
    mut stream: ResMut<ChunkStream<NeonWorld>>,
    time: Res<Time>,
    mut intro: Local<Option<f32>>,
    mut quality: Local<usize>,
) {
    if keyboard.just_pressed(KeyCode::KeyW) {
        road.wet = 1.0 - road.wet;
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        road.rain = 1.0 - road.rain;
        road.wet = road.wet.max(road.rain);
    }
    if keyboard.just_pressed(KeyCode::KeyB) {
        *intro = Some(0.0);
    }
    if keyboard.just_pressed(KeyCode::KeyQ) {
        *quality = (*quality + 1) % 4;
        if world.set_quality(*quality) {
            stream.replan_all();
        }
        info!("qualité de la ville : niveau {}", *quality);
    }
    if let Some(t) = intro.as_mut() {
        *t += time.delta_secs() / 6.0;
        look.build = t.min(1.0);
        if *t >= 1.0 {
            *intro = None;
        }
    }
}

/// Kick régulier et intensité qui varie : de quoi voir réagir les façades.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let t = time.elapsed_secs();
    let beat_s = 60.0 / BPM;
    let since_kick = t % beat_s;
    let pulse = (-since_kick / PULSE_DECAY_S).exp();
    features.0.pulse = pulse;
    features.0.bass = 0.5 * pulse;
    features.0.high = 0.4;
    features.0.mid = 0.3;
    features.0.beat = f64::from(t / beat_s);
    features.0.intensity = 0.5 - 0.5 * (std::f32::consts::TAU * t / INTENSITY_PERIOD_S).cos();
}
