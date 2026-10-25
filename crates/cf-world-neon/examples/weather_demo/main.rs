//! La brume et la pluie à l'écran, et leur dégradation par la qualité adaptative.
//!
//! ```text
//! cargo run -p cf-world-neon --example weather_demo --features demo                 # interactif
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/meteo cargo run ... --features demo               # scènes scriptées, verdicts, captures
//! ```
//!
//! Interactif : `R` fait pleuvoir ou cesser, `Q` change de cran de qualité adaptative (0 → dernier), flèches pour orienter la
//! caméra. Avec `CF_CHECK=1` : averse forcée, une capture à pleine qualité, une au dernier cran, une sans averse ; verdicts
//! sur les pixels (la pluie ajoute de la lumière, moitié moins de gouttes au dernier cran).
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

#![allow(
    clippy::print_stderr,
    reason = "les verdicts du mode CF_CHECK sont écrits sur la sortie d'erreur"
)]

#[path = "../road_demo/shots.rs"]
#[allow(
    dead_code,
    reason = "module partagé avec road_demo : weather_demo n'en utilise qu'une partie"
)]
mod shots;

use std::time::{Duration, Instant};

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::quality::levers_for;
use cf_render::{
    AbsolutePosition, AudioFeatures, CameraPose, CfSet, CorePlugin, CullingPlugin, CurrentShowFx, LayersPlugin,
    MAX_LEVEL, ShowFxInput, StreamFocus,
};
use cf_world_neon::{NeonWorldPlugin, WeatherState};
use shots::{Shot, luma};

const SIZE: (u32, u32) = (960, 540);
const CAR: DVec3 = DVec3::new(0.0, 0.0, -300.0);
/// Temps réel (s) laissé au démarrage (compilation des pipelines par le rendu logiciel).
const WARMUP_S: f64 = 12.0;
/// Images de pluie avant chaque capture : la boîte est pleine, les gouttes ont tombé.
const SETTLE_FRAMES: u32 = 90;

/// Ce que la démo force : averse (0 ou 1) et cran de qualité.
#[derive(Resource, Debug, Clone, Copy)]
struct Forced {
    rain: f32,
    level: usize,
}

/// Le script du mode `CF_CHECK=1`.
#[derive(Resource, Default)]
struct Check {
    start: Option<Instant>,
    step: usize,
    frames: u32,
    awaiting: bool,
    shots: Vec<(&'static str, Shot)>,
}

/// Les trois scènes : `(nom, averse, cran)`.
const STEPS: [(&str, f32, usize); 3] = [
    ("1-pluie-pleine", 1.0, 0),
    ("2-pluie-qualite-max", 1.0, MAX_LEVEL),
    ("3-sec", 0.0, 0),
];

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · météo".into(),
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
    .insert_resource(CameraPose {
        position: CAR + DVec3::new(0.0, 3.0, 12.0),
        ..CameraPose::default()
    })
    .insert_resource(Forced { rain: 1.0, level: 0 })
    .insert_resource(levers_for(0))
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn((AbsolutePosition(CAR), StreamFocus));
    })
    // Après `step_weather` (Simulate) : l'averse forcée gagne sur le cycle libre.
    .add_systems(Update, force.in_set(CfSet::Direct));
    if check {
        app.init_resource::<Check>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, run_check.in_set(CfSet::Direct));
    } else {
        app.add_systems(Update, keys.in_set(CfSet::Input));
    }
    app.run();
}

fn force(forced: Res<Forced>, mut state: ResMut<WeatherState>, mut commands: Commands) {
    state.weather.rain = forced.rain;
    state.weather.wet = forced.rain;
    commands.insert_resource(levers_for(forced.level));
}

fn keys(keyboard: Res<ButtonInput<KeyCode>>, mut forced: ResMut<Forced>) {
    if keyboard.just_pressed(KeyCode::KeyR) {
        forced.rain = 1.0 - forced.rain;
    }
    if keyboard.just_pressed(KeyCode::KeyQ) {
        forced.level = (forced.level + 1) % (MAX_LEVEL + 1);
    }
}

fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut forced: ResMut<Forced>,
    mut time: ResMut<Time<Virtual>>,
    mut exit: MessageWriter<AppExit>,
) {
    let start = *check.start.get_or_insert_with(Instant::now);
    if start.elapsed().as_secs_f64() < WARMUP_S || check.awaiting {
        return;
    }
    if check.step >= STEPS.len() {
        verdicts(&check.shots);
        exit.write(AppExit::Success);
        return;
    }
    let (name, rain, level) = STEPS[check.step];
    *forced = Forced { rain, level };
    check.frames += 1;
    if check.frames < SETTLE_FRAMES {
        return;
    }
    check.frames = 0;
    check.awaiting = true;
    // Le rendu logiciel met des secondes à rendre une image : l'horloge est figée le temps de la capture.
    time.pause();
    let mut shot = commands.spawn(Screenshot::primary_window());
    shot.observe(
        move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>, mut time: ResMut<Time<Virtual>>| {
            time.unpause();
            if let Ok(image) = captured.image.clone().try_into_dynamic() {
                let rgba = image.to_rgba8();
                let width = rgba.width();
                check.shots.push((
                    name,
                    Shot {
                        width,
                        rgba: rgba.into_raw(),
                    },
                ));
            }
            check.step += 1;
            check.awaiting = false;
        },
    );
    if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
        let dir = std::path::PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&dir);
        shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
    }
}

/// Pixels nettement plus clairs que la scène sèche, dans la moitié haute de l'image (ni route mouillée ni gouttes de
/// l'objectif en bas : seulement les traînées de pluie devant les immeubles et le ciel).
fn streaks(shot: &Shot, dry: &Shot) -> usize {
    let mut n = 0;
    for y in 0..shot.height() / 2 {
        for x in 0..shot.width {
            if luma(shot.pixel(x, y)) > luma(dry.pixel(x, y)) + 8.0 {
                n += 1;
            }
        }
    }
    n
}

fn verdicts(shots: &[(&'static str, Shot)]) {
    let [(_, full), (_, worst), (_, dry)] = shots else {
        eprintln!("FAIL : captures manquantes ({})", shots.len());
        std::process::exit(1);
    };
    let (n_full, n_worst) = (streaks(full, dry), streaks(worst, dry));
    eprintln!("pixels éclaircis par la pluie : pleine qualité {n_full}, dernier cran {n_worst}");
    let ok = n_full > 50 && n_worst < n_full;
    eprintln!("{}", if ok { "OK" } else { "FAIL" });
    if !ok {
        std::process::exit(1);
    }
}
