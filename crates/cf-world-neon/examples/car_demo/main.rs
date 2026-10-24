//! La `CyberCar` de Neon City à l'écran, filmée par la caméra de poursuite provisoire.
//!
//! ```text
//! cargo run -p cf-world-neon --example car_demo --features demo                  # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/voiture cargo run ... --example car_demo --features demo   # scènes et verdicts
//! ```
//!
//! Interactif : un kick à 128 BPM fait pulser les lumières de la voiture ; `A` / `D` tournent le volant (clignotant du
//! côté où il tourne, la voiture dérive), `S` freine (feux stop, fumée), `C` bascule entre la poursuite et une vue
//! trois quarts avant. La voiture file à vitesse de croisière.
//!
//! Avec `CF_CHECK=1`, l'exemple passe par des scènes scriptées (roulage, kick, freinage, clignotant, glissade, trois
//! quarts), capture l'écran à chacune et vérifie sur les pixels que la voiture est dessinée, que les kicks la font
//! pulser, que les feux stop et le clignotant s'allument.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;
mod shots;

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{AudioFeatures, CameraPose, CfSet, CorePlugin, CullingPlugin, LayersPlugin, PostFxPlugin};
use cf_world_neon::car::{CarControls, ModelQuality};
use cf_world_neon::chase::CRUISE_SPEED_MPS;
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// Cadence du kick de l'interactif, en battements par minute.
const BPM: f32 = 128.0;
/// Retombée du pouls (s) : le pouls de `AudioFeatures` retombe en ~0,12 s après un kick.
const PULSE_DECAY_S: f32 = 0.12;

/// Vue de la caméra : la poursuite provisoire, ou une vue trois quarts avant / arrière pour juger la voiture.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum View {
    #[default]
    Chase,
    FrontThreeQuarter,
    RearThreeQuarter,
}

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let quality = if std::env::var_os("CF_LOW").is_some() {
        ModelQuality::Low
    } else {
        ModelQuality::High
    };
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · voiture".into(),
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
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: -1234.5,
                // Scènes de capture : c'est le script qui déplace la voiture.
                speed_mps: if check { 0.0 } else { CRUISE_SPEED_MPS },
            },
        },
        cf_world_neon::car::CarPlugin { quality, ..default() },
    ))
    .init_resource::<View>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .add_systems(Update, camera_view.after(CfSet::Direct).before(CfSet::Present));
    if check {
        app.init_resource::<check::Check>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.in_set(CfSet::Input));
    } else {
        app.add_systems(Update, (keys, beat).chain().in_set(CfSet::Input));
    }
    app.run();
}

/// Touches : `A` / `D` volant, `S` frein, `C` change de vue.
fn keys(keyboard: Res<ButtonInput<KeyCode>>, mut controls: ResMut<CarControls>, mut view: ResMut<View>) {
    let axis = f32::from(i8::from(keyboard.pressed(KeyCode::KeyD)) - i8::from(keyboard.pressed(KeyCode::KeyA)));
    controls.steer += (axis - controls.steer) * 0.1;
    controls.brake = f32::from(u8::from(keyboard.pressed(KeyCode::KeyS)));
    if keyboard.just_pressed(KeyCode::KeyC) {
        *view = match *view {
            View::Chase => View::FrontThreeQuarter,
            View::FrontThreeQuarter => View::RearThreeQuarter,
            View::RearThreeQuarter => View::Chase,
        };
    }
}

/// Kick régulier : de quoi voir pulser la voiture.
fn beat(time: Res<Time>, mut features: ResMut<AudioFeatures>) {
    let t = time.elapsed_secs();
    let since_kick = t % (60.0 / BPM);
    let pulse = (-since_kick / PULSE_DECAY_S).exp();
    features.0.live = true;
    features.0.pulse = pulse;
    features.0.bass = 0.5 * pulse;
    features.0.intensity = 0.6;
}

/// Après la caméra de poursuite : une vue trois quarts autour de la voiture, si demandée.
fn camera_view(
    view: Res<View>,
    car: Query<&cf_render::AbsolutePosition, With<cf_render::StreamFocus>>,
    mut pose: ResMut<CameraPose>,
) {
    let offset = match *view {
        View::Chase => return,
        View::FrontThreeQuarter => Vec3::new(3.4, 1.35, -5.6),
        View::RearThreeQuarter => Vec3::new(3.2, 1.35, 5.8),
    };
    let Ok(car) = car.single() else { return };
    let eye = car.0 + offset.as_dvec3();
    let target = car.0 + bevy::math::DVec3::new(0.0, 0.55, 0.0);
    let rotation = Transform::IDENTITY
        .looking_to((target - eye).as_vec3(), Vec3::Y)
        .rotation;
    *pose = CameraPose {
        position: eye,
        rotation,
        fov_deg: 45.0,
        ..CameraPose::default()
    };
}
