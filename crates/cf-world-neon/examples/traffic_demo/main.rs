//! Le trafic de Neon City à l'écran : le trafic de fond (quatre couloirs aériens) et une vitrine scénarisée devant la
//! voiture, qui montre ce que le socle sait dessiner : voiture de police (gyrophares), motos et leurs murs de lumière,
//! engins profilés et gros porteurs avec leurs traînées, points lumineux.
//!
//! ```text
//! cargo run -p cf-world-neon --example traffic_demo --features demo                       # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/trafic cargo run ... --example traffic_demo --features demo   # capture et verdict
//! ```
//!
//! La vitrine est un événement de test inscrit au registre du trafic (il n'existe pas dans Sillage : les vrais événements
//! arrivent avec J7-04 et J7-05). Les gyrophares ne battent que si le spectacle permet les flashs : l'exemple pose
//! `CurrentShowFx` avec `flashes` à vrai, et leur éclat passe par le limiteur.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
mod showcase;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::postfx::ShowFxInput;
use cf_render::{AudioFeatures, CfSet, CorePlugin, CullingPlugin, CurrentShowFx, LayersPlugin, PostFxPlugin};
use cf_world_neon::car::CarPlugin;
use cf_world_neon::pilot::{PilotGates, ShowInput};
use cf_world_neon::traffic::{EventName, TrafficAppExt, TrafficDirector, TrafficPlugin};
use cf_world_neon::{ChaseCameraPlugin, NeonWorldPlugin, PilotPlugin, PilotSettings};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// Cadence du kick, en battements par minute.
const BPM: f32 = 128.0;
/// Retombée du pouls (s).
const PULSE_DECAY_S: f32 = 0.12;

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · trafic".into(),
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
                // Au sol, allure de croisière : la vitrine est devant la voiture.
                show: Some(ShowInput {
                    level: 2,
                    heat: 0.5,
                    breathing: false,
                    drop_now: false,
                    time_scale: 1.0,
                    gates: PilotGates {
                        cruise: 1.0,
                        flight_rate: 0.0,
                        max_floor: 0,
                        flight_on_drop_only: false,
                        tricks: false,
                    },
                }),
                ..default()
            },
        },
        CarPlugin::default(),
        TrafficPlugin::default(),
    ))
    .register_traffic_event(EventName::Bikes, |ctx| Box::new(showcase::Showcase::new(ctx)))
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(CurrentShowFx(ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.5,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    }))
    .add_systems(Update, (beat, force_showcase).in_set(CfSet::Input));
    if check {
        app.init_resource::<check::Check>()
            // 1/20 s par image : le rendu logiciel est lent, la scène avance quand même de quatre secondes en 80 images.
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(50)))
            .add_systems(Update, check::run_check.in_set(CfSet::Present));
    }
    app.run();
}

/// Lance la vitrine une fois : un événement forcé, quelles que soient les portes.
fn force_showcase(mut director: ResMut<TrafficDirector>, mut done: Local<bool>) {
    if !*done {
        director.force(EventName::Bikes);
        *done = true;
    }
}

/// Kick régulier à 128 BPM : de quoi faire pulser les néons des véhicules.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let period = 60.0 / BPM;
    let t = time.elapsed_secs();
    let before = ((t - time.delta_secs()).max(0.0) / period).floor();
    let now = (t / period).floor();
    let f = &mut features.0;
    f.live = true;
    f.kick = now > before;
    f.count = now as u32;
    f.pulse = (-(t % period) / PULSE_DECAY_S).exp();
    f.bass = 0.5 * f.pulse + 0.3;
    f.mid = 0.4;
    f.loud = 0.55;
    f.bpm = Some(BPM);
    f.intensity = 0.85;
    f.section = cf_analysis::Section::Peak;
}
