//! Le post-process à l'écran : une rue de boîtes aux arêtes néon, une charte de couleurs, un titre 2D.
//!
//! ```text
//! cargo run -p cf-render --example postfx_demo --features demo                 # interactif
//! CF_CHECK=1 cargo run -p cf-render --example postfx_demo --features demo      # étapes scriptées et verdicts
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/postfx cargo run ...                         # garde les captures et les uniformes
//! ```
//!
//! Interactif : `K` kick, `D` drop, `C` cue suivant (vague), `H` chaleur, `B` bullet-time, `R` respiration, `F` flashs
//! coupés, `L` gouttes sur l'objectif, `G` bloom. Tout flash passe par le limiteur : marteler `D` ne donne jamais plus
//! de 3 flashs par seconde.
//!
//! Avec `CF_CHECK=1`, l'exemple passe par sept étapes (neutre, kick, chaleur, flash de drop, vague, bullet-time, sans
//! bloom), capture l'écran à chacune, écrit les uniformes effectivement envoyés au shader (`phases.json`, pour rendre
//! la même image de référence avec le shader de Sillage) et vérifie sur les pixels que chaque effet se voit **et que
//! le titre 2D n'en subit aucun**.
//!
//! La scène est volontairement simple et sans éclairage (matériaux `unlit`) : elle est reproduite à l'identique dans
//! three.js pour la capture côte à côte avec Sillage (`tools/sillage-golden/postfx/`).
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;

use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{
    AudioFeatures, BackgroundLayer, CameraPose, CfSet, CorePlugin, CurrentShowFx, LayersPlugin, LensWeather,
    OverlayLayer, PostFxPlugin, PostFxSettings, ShowFxInput, WorldLayer,
};
use cf_show::drop::DropFxPackage;

/// Taille de la fenêtre (et des captures).
pub(crate) const SIZE: (u32, u32) = (960, 540);
/// Couleur (linéaire) du titre : un gris moyen, que tout effet ferait bouger.
pub(crate) const TITLE_GREY: f32 = 0.5;

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · post-process".into(),
            resolution: WindowResolution::new(SIZE.0, SIZE.1),
            present_mode: PresentMode::AutoNoVsync,
            resizable: false,
            ..default()
        }),
        ..default()
    }))
    .add_plugins((CorePlugin, LayersPlugin::default(), PostFxPlugin))
    .insert_resource(CameraPose {
        position: DVec3::new(0.0, 1.6, 0.0),
        ..default()
    })
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .add_systems(Startup, spawn_scene);
    if check {
        app.init_resource::<check::Check>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(Update, check::run_check.after(CfSet::Present));
    } else {
        app.init_resource::<Play>()
            .insert_resource(CurrentShowFx(CALM))
            .add_systems(Update, play.in_set(CfSet::Input));
    }
    app.run();
}

fn unlit(materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::linear_rgb(r, g, b),
        unlit: true,
        fog_enabled: false,
        ..default()
    })
}

/// La scène de référence. Tout est en lumière linéaire ; les néons dépassent 1 pour que le bloom les prenne.
///
/// **À garder identique** à `tools/sillage-golden/postfx/scene.ts`.
fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Fond : voûte violet nuit.
    commands.spawn((
        BackgroundLayer,
        Mesh3d(meshes.add(Sphere::new(1000.0).mesh().uv(32, 18))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::linear_rgb(0.02, 0.012, 0.04),
            unlit: true,
            cull_mode: None,
            fog_enabled: false,
            ..default()
        })),
    ));
    // Sol.
    commands.spawn((
        WorldLayer,
        Mesh3d(meshes.add(Cuboid::new(200.0, 0.02, 200.0))),
        MeshMaterial3d(unlit(&mut materials, 0.015, 0.015, 0.02)),
        Transform::from_xyz(0.0, -0.01, -90.0),
    ));
    // Rangs de boîtes, arête intérieure du toit en néon (cyan à gauche, violet à droite).
    let body = unlit(&mut materials, 0.04, 0.03, 0.08);
    let neon = [
        unlit(&mut materials, 0.0, 3.0, 4.0),
        unlit(&mut materials, 3.0, 0.6, 4.0),
    ];
    let strip = meshes.add(Cuboid::new(0.15, 0.15, 3.0));
    for i in 0..8 {
        let z = -8.0 - 6.0 * i as f32;
        let height = 6.0 + (i % 3) as f32 * 2.0;
        let block = meshes.add(Cuboid::new(3.0, height, 3.0));
        for (side, neon) in [(-1.0_f32, &neon[0]), (1.0, &neon[1])] {
            commands.spawn((
                WorldLayer,
                Mesh3d(block.clone()),
                MeshMaterial3d(body.clone()),
                Transform::from_xyz(side * 5.0, height / 2.0, z),
            ));
            commands.spawn((
                WorldLayer,
                Mesh3d(strip.clone()),
                MeshMaterial3d(neon.clone()),
                Transform::from_xyz(side * 3.5, height, z),
            ));
        }
    }
    // Ligne médiane ambrée.
    let dash = meshes.add(Cuboid::new(0.2, 0.02, 1.5));
    let amber = unlit(&mut materials, 2.5, 2.0, 0.6);
    for i in 0..14 {
        commands.spawn((
            WorldLayer,
            Mesh3d(dash.clone()),
            MeshMaterial3d(amber.clone()),
            Transform::from_xyz(0.0, 0.01, -3.0 - 4.0 * i as f32),
        ));
    }
    // Charte : rouge, vert, bleu, cyan, blanc, gris 18 %.
    let patch = meshes.add(Rectangle::new(0.7, 0.7));
    for (i, (r, g, b)) in check::CHART.into_iter().enumerate() {
        commands.spawn((
            WorldLayer,
            Mesh3d(patch.clone()),
            MeshMaterial3d(unlit(&mut materials, r, g, b)),
            Transform::from_xyz(-2.0 + 0.8 * i as f32, 1.6, -6.0),
        ));
    }
    // Surcouche 2D : titre en haut (sur la vignette), nom du DJ dans le coin (là où elle est la plus forte).
    let grey = Color::linear_rgb(TITLE_GREY, TITLE_GREY, TITLE_GREY);
    commands.spawn((
        OverlayLayer,
        Text2d::new("CYBER-FUNK"),
        TextFont {
            font_size: FontSize::Px(64.0),
            ..default()
        },
        TextColor(grey),
        Transform::from_xyz(0.0, 120.0, 0.0),
    ));
    commands.spawn((
        OverlayLayer,
        Text2d::new("NOM DU DJ"),
        TextFont {
            font_size: FontSize::Px(40.0),
            ..default()
        },
        TextColor(grey),
        Transform::from_xyz(-330.0, -230.0, 0.0),
    ));
}

/// Les effets du drop : flash plein demandé (le limiteur le ramène), séparation RVB moyenne.
pub(crate) const DROP: DropFxPackage = DropFxPackage {
    flash: 1.0,
    rgb_split: 0.5,
    city_pulse: 0.0,
    shockwaves: 1,
    burst: 20,
};

/// Un spectacle au repos : flashs permis, rien d'autre.
pub(crate) const CALM: ShowFxInput = ShowFxInput {
    flashes: true,
    breathing: false,
    heat: 0.0,
    time_scale: 1.0,
    cue_seq: 0,
    fx_rgb_split: 1.0,
    fx_flash: 1.0,
    drop: None,
};

// ───────────────────────── mode interactif ─────────────────────────

#[derive(Resource)]
struct Play {
    show: ShowFxInput,
    pulse: f32,
    rain: bool,
}

impl Default for Play {
    fn default() -> Self {
        Self {
            show: CALM,
            pulse: 0.0,
            rain: false,
        }
    }
}

#[allow(clippy::too_many_arguments, reason = "un exemple : chaque touche a sa ressource")]
fn play(
    mut show: ResMut<CurrentShowFx>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut play: ResMut<Play>,
    mut audio: ResMut<AudioFeatures>,
    mut settings: ResMut<PostFxSettings>,
    mut weather: ResMut<LensWeather>,
) {
    let play = &mut *play;
    play.show.drop = keys.just_pressed(KeyCode::KeyD).then_some(DROP);
    if keys.just_pressed(KeyCode::KeyK) {
        play.pulse = 1.0;
    }
    play.pulse = (play.pulse - time.delta_secs() * 3.0).max(0.0);
    if keys.just_pressed(KeyCode::KeyC) {
        play.show.cue_seq += 1;
    }
    if keys.just_pressed(KeyCode::KeyH) {
        play.show.heat = if play.show.heat > 0.5 { 0.0 } else { 1.0 };
    }
    if keys.just_pressed(KeyCode::KeyB) {
        play.show.time_scale = if play.show.time_scale < 0.5 { 1.0 } else { 0.2 };
    }
    if keys.just_pressed(KeyCode::KeyR) {
        play.show.breathing = !play.show.breathing;
    }
    if keys.just_pressed(KeyCode::KeyF) {
        play.show.flashes = !play.show.flashes;
    }
    if keys.just_pressed(KeyCode::KeyL) {
        play.rain = !play.rain;
    }
    if keys.just_pressed(KeyCode::KeyG) {
        settings.bloom = !settings.bloom;
    }
    audio.0.pulse = play.pulse;
    audio.0.section = cf_analysis::Section::Peak;
    *weather = LensWeather {
        rain: if play.rain { 1.0 } else { 0.0 },
        speed_ratio: 0.8,
    };
    show.0 = play.show;
}

