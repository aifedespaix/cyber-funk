//! Les enseignes de Neon City à l'écran : hologrammes, portiques, panneaux de toit, projections, néons, écrans-caméra,
//! annonces du DJ.
//!
//! ```text
//! cargo run -p cf-world-neon --example signs_demo --features demo                 # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/enseignes cargo run ...                          # scènes scriptées, verdicts, captures
//! ```
//!
//! Interactif : la voiture file sur la route, un kick à 128 BPM fait pulser les lisérés (par le limiteur de flashs).
//!
//! Avec `CF_CHECK=1` : une scène par format, une capture à chacune et des verdicts sur les pixels, entre autres que **le
//! show ne s'ouvre qu'une fois les textures cuites** (carnet de chargement) et qu'**aucun pipeline n'est compilé après
//! `Ready`** (règle 8 de `CLAUDE.md`), même quand un format qui ne s'était jamais affiché apparaît.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
#[path = "../road_demo/shots.rs"]
#[allow(
    dead_code,
    reason = "module partagé avec road_demo : signs_demo n'en utilise qu'une partie"
)]
mod shots;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{
    AudioFeatures, CfSet, CorePlugin, CullingPlugin, CurrentShowFx, DiagnosticsPlugin, LayersPlugin, ShowFxInput,
    WarmupPlugin,
};
use cf_world_neon::signs::SignShow;
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
const BPM: f32 = 128.0;
const PULSE_DECAY_S: f32 = 0.12;

/// Les flashs sont permis (sous le limiteur) : c'est le cas normal d'un show.
fn fx() -> CurrentShowFx {
    CurrentShowFx(ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    })
}

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · enseignes".into(),
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
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: 0.0,
                speed_mps: if check { 0.0 } else { cf_world_neon::chase::CRUISE_SPEED_MPS },
            },
        },
    ))
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(fx())
    // Les noms des DJ du show de la démo : leurs panneaux sont cuits avec le reste.
    .insert_resource(SignShow {
        active: false,
        dj_on_ads: true,
        slot: 0,
        slots: SignShow::names("Démesure Live", ["KRMN", "Nova Kaito b2b Suzu"]),
    });
    if check {
        app.init_resource::<check::Check>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.after(CfSet::Present));
    } else {
        app.add_systems(Update, beat.in_set(CfSet::Input));
    }
    app.run();
}

/// Kick régulier : de quoi voir pulser les lisérés.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let t = time.elapsed_secs();
    let beat_s = 60.0 / BPM;
    let pulse = (-(t % beat_s) / PULSE_DECAY_S).exp();
    features.0.pulse = pulse;
    features.0.kick = (t % beat_s) < time.delta_secs();
    features.0.bass = 0.5 * pulse;
    features.0.high = 0.4;
    features.0.mid = 0.3;
    features.0.beat = f64::from(t / beat_s);
    features.0.intensity = 0.6;
}
