//! La route de Neon City à l'écran, filmée par la caméra de poursuite provisoire.
//!
//! ```text
//! cargo run -p cf-world-neon --example road_demo --features demo                 # interactif
//! CF_START_KM=400 cargo run -p cf-world-neon --example road_demo --features demo # départ au km 400
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/route cargo run ...                            # scènes scriptées, verdicts, captures
//! ```
//!
//! Interactif : un kick à 128 BPM fait pulser la route, l'intensité du morceau passe lentement du calme au sommet (la
//! teinte d'ambiance des liserés suit). `W` mouille la chaussée, `R` lance la pluie, `I` rejoue la pré-intro.
//!
//! Avec `CF_CHECK=1`, l'exemple passe par des scènes scriptées (départ, kick, pluie, pré-intro, freinage, seuil de
//! l'origine flottante, km 403), capture l'écran à chacune, écrit leurs réglages (`scenes.json`, pour rendre la même
//! image de référence avec le code d'origine de Sillage, `tools/sillage-golden/road-render/`) et vérifie sur les
//! pixels, entre autres, que **les motifs sont identiques de part et d'autre d'un recentrage d'origine et à 403 km**.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
mod json;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{AudioFeatures, CfSet, CorePlugin, CullingPlugin, LayersPlugin};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RoadLook, RunnerConfig};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// Cadence du kick de l'interactif, en battements par minute.
const BPM: f32 = 128.0;
/// Retombée du pouls (s) : le pouls de `AudioFeatures` retombe en ~0,12 s après un kick.
const PULSE_DECAY_S: f32 = 0.12;
/// Période (s) du va-et-vient de l'intensité du morceau.
const INTENSITY_PERIOD_S: f32 = 40.0;

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let start_km: f64 = std::env::var("CF_START_KM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · route".into(),
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
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: -start_km * 1000.0,
                // Scènes de capture : le point suivi reste où le script le met.
                speed_mps: if check { 0.0 } else { cf_world_neon::chase::CRUISE_SPEED_MPS },
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

/// Touches : `W` chaussée mouillée, `R` pluie, `I` pré-intro (la route se redessine depuis la voiture).
fn keys(keyboard: Res<ButtonInput<KeyCode>>, mut look: ResMut<RoadLook>, time: Res<Time>, mut intro: Local<Option<f32>>) {
    if keyboard.just_pressed(KeyCode::KeyW) {
        look.wet = 1.0 - look.wet;
    }
    if keyboard.just_pressed(KeyCode::KeyR) {
        look.rain = 1.0 - look.rain;
        look.wet = look.wet.max(look.rain);
    }
    if keyboard.just_pressed(KeyCode::KeyI) {
        *intro = Some(0.0);
    }
    if let Some(t) = intro.as_mut() {
        *t += time.delta_secs() / 6.0;
        look.draw = t.min(1.0);
        if *t >= 1.0 {
            *intro = None;
        }
    }
}

/// Kick régulier et intensité qui varie : de quoi voir réagir la route.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let t = time.elapsed_secs();
    let beat_s = 60.0 / BPM;
    let since_kick = t % beat_s;
    let pulse = (-since_kick / PULSE_DECAY_S).exp();
    features.0.pulse = pulse;
    features.0.bass = 0.5 * pulse;
    features.0.intensity = 0.5 - 0.5 * (std::f32::consts::TAU * t / INTENSITY_PERIOD_S).cos();
}
