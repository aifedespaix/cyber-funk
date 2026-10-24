//! La voiture en pilote automatique dans Neon City, filmée par la caméra de poursuite provisoire.
//!
//! ```text
//! cargo run -p cf-world-neon --example pilot_demo --features demo                 # interactif, niveau 5
//! CF_LEVEL=0 cargo run ... --example pilot_demo --features demo                   # niveau 0 : la voiture ne vole pas
//! CF_CHECK=1 CF_LEVEL=5 CF_SNAPSHOT_DIR=/tmp/pilote cargo run ... --example pilot_demo --features demo
//! ```
//!
//! Les portes sont celles du vrai look du niveau (`assets/looks/niveau-N-*.ron`). Un kick à 128 BPM tient lieu de
//! musique. Avec `CF_CHECK=1`, l'exemple avance le pilote en accéléré (la logique seule, sans rendu) jusqu'à un envol,
//! capture l'écran en vol puis en pleine figure, et vérifie sur les pixels que la voiture est dessinée ; au niveau 0 il
//! laisse rouler trois minutes et vérifie qu'elle n'a jamais quitté le sol.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{AudioFeatures, CfSet, CorePlugin, CullingPlugin, LayersPlugin, PostFxPlugin};
use cf_world_neon::car::{CarPlugin, ModelQuality};
use cf_world_neon::pilot::{PilotGates, PilotSet, ShowInput};
use cf_world_neon::{ChaseCameraPlugin, NeonWorldPlugin, PilotPlugin, PilotSettings};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// Cadence du kick, en battements par minute.
const BPM: f32 = 128.0;
/// Retombée du pouls (s) : le pouls de `AudioFeatures` retombe en ~0,12 s après un kick.
const PULSE_DECAY_S: f32 = 0.12;

const LOOKS: [&str; 6] = [
    include_str!("../../../../assets/looks/niveau-0-pre-intro.ron"),
    include_str!("../../../../assets/looks/niveau-1-ville.ron"),
    include_str!("../../../../assets/looks/niveau-2-rues.ron"),
    include_str!("../../../../assets/looks/niveau-3-envol.ron"),
    include_str!("../../../../assets/looks/niveau-4-meteores.ron"),
    include_str!("../../../../assets/looks/niveau-5-cosmos.ron"),
];

/// Le spectacle du niveau, portes lues dans son look.
fn show_of(level: u8, heat: f64) -> Option<ShowInput> {
    let look = match cf_show::load::parse_look(LOOKS[usize::from(level.min(5))], "pilot_demo") {
        Ok(look) => look,
        Err(error) => {
            error!(%error, level, "look du niveau illisible : portes toutes ouvertes");
            return None;
        }
    };
    let g = look.gates;
    Some(ShowInput {
        level,
        heat,
        breathing: false,
        drop_now: false,
        time_scale: 1.0,
        gates: PilotGates {
            cruise: f64::from(g.cruise),
            flight_rate: f64::from(g.flight_rate),
            max_floor: g.max_floor,
            flight_on_drop_only: g.flight_on_drop_only,
            tricks: g.tricks,
        },
    })
}

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let level = std::env::var("CF_LEVEL")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(5);
    let quality = if std::env::var_os("CF_LOW").is_some() {
        ModelQuality::Low
    } else {
        ModelQuality::High
    };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: format!("cyber-funk · pilote · niveau {level}"),
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
        PostFxPlugin,
        NeonWorldPlugin::embedded(),
        ChaseCameraPlugin,
        PilotPlugin {
            settings: PilotSettings {
                show: show_of(level, 0.9),
                ..default()
            },
        },
        CarPlugin { quality, ..default() },
    ))
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .add_systems(Update, beat.in_set(CfSet::Input));
    if check {
        app.init_resource::<check::Check>()
            .insert_resource(check::Level(level))
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::fast_forward.in_set(CfSet::Simulate).before(PilotSet))
            .add_systems(Update, check::run_check.in_set(CfSet::Present));
    }
    app.run();
}

/// Kick régulier à 128 BPM : de quoi faire danser le pilote et pulser la voiture.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let period = 60.0 / BPM;
    let t = time.elapsed_secs();
    let before = ((t - time.delta_secs()).max(0.0) / period).floor();
    let now = (t / period).floor();
    let pulse = (-(t % period) / PULSE_DECAY_S).exp();
    let f = &mut features.0;
    f.live = true;
    f.kick = now > before;
    f.count = now as u32;
    f.pulse = pulse;
    f.bass = 0.5 * pulse + 0.3;
    f.loud = 0.55;
    f.bpm = Some(BPM);
    f.intensity = 0.85;
    f.section = cf_analysis::Section::Peak;
}
