//! La qualité adaptative sur un vrai rendu : on parcourt tous les niveaux et on vérifie, sous Xvfb + lavapipe, que
//! changer de qualité ne compile aucun pipeline (règle 8) et ne fait aucun saut de luminance (règle 7).
//!
//! ```text
//! CF_CHECK=1 xvfb-run -a cargo run -p cf-render --example quality_demo --features demo
//! ```
//!
//! Sans `CF_CHECK`, la fenêtre reste en mode automatique (la mesure décide) et la touche `1` à `4` impose un niveau,
//! `A` revient à la mesure.
//!
//! Avec `CF_CHECK=1` : attend `Ready`, puis impose les niveaux 1, 2, 3, 2, 1, 0 (120 images chacun) en capturant l'écran
//! toutes les 6 images, puis écrit des verdicts :
//! 1. `LatePipelines` reste à 0 ;
//! 2. la luminance moyenne de l'écran ne varie jamais de plus de `MAX_STEP` (relatif) entre deux captures successives ;
//! 3. les textures des couches 3D suivent l'échelle de rendu, celle de la surcouche reste à la taille de la fenêtre.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::time::Duration;

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{
    AppState, AudioFeatures, BackgroundLayer, CameraPose, CorePlugin, DiagnosticsPlugin, LatePipelines, Layer,
    LayerTargets, LayersPlugin, OverlayLayer, PostFxPlugin, QualityLevers, QualityMode, QualityPlugin, RenderScale,
    WarmupPlugin, WorldLayer,
};

const SIZE: (u32, u32) = (960, 540);
/// Niveaux imposés, dans l'ordre.
const SCRIPT: [usize; 6] = [1, 2, 3, 2, 1, 0];
/// Images par palier.
const HOLD: u32 = 120;
/// Captures toutes les N images (N / 60 s).
const EVERY: u32 = 6;
/// Variation relative maximale de la luminance moyenne entre deux captures successives : la moitié du seuil de 10 %
/// du WCAG 2.3.1 pour un flash.
const MAX_STEP: f32 = 0.05;

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · qualité".into(),
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
        DiagnosticsPlugin::default(),
        LayersPlugin::default(),
        PostFxPlugin,
        QualityPlugin,
    ))
    .insert_resource(CameraPose {
        position: DVec3::new(0.0, 1.6, 0.0),
        ..default()
    })
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .add_systems(Startup, spawn_scene);
    if check {
        app.init_resource::<Report>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(1.0 / 60.0)))
            .add_systems(
                Update,
                (
                    precapture.run_if(in_state(AppState::Loading)),
                    run_check.run_if(in_state(AppState::Ready)),
                ),
            );
    } else {
        app.add_systems(Update, keys);
    }
    app.run();
}

fn keys(keys: Res<ButtonInput<KeyCode>>, mut mode: ResMut<QualityMode>) {
    for (key, level) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
    ] {
        if keys.just_pressed(key) {
            *mode = QualityMode::Fixed(level);
        }
    }
    if keys.just_pressed(KeyCode::KeyA) {
        *mode = QualityMode::Auto;
    }
}

fn unlit(materials: &mut Assets<StandardMaterial>, r: f32, g: f32, b: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: Color::linear_rgb(r, g, b),
        unlit: true,
        fog_enabled: false,
        ..default()
    })
}

/// Une rue de boîtes aux arêtes néon (qui dépassent 1 : le bloom les prend), un ciel, un titre 2D.
fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
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
    commands.spawn((
        WorldLayer,
        Mesh3d(meshes.add(Cuboid::new(200.0, 0.02, 200.0))),
        MeshMaterial3d(unlit(&mut materials, 0.015, 0.015, 0.02)),
        Transform::from_xyz(0.0, -0.01, -90.0),
    ));
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
    commands.spawn((
        OverlayLayer,
        Text2d::new("CYBER-FUNK"),
        TextFont {
            font_size: FontSize::Px(64.0),
            ..default()
        },
        TextColor(Color::linear_rgb(0.5, 0.5, 0.5)),
        Transform::from_xyz(0.0, 120.0, 0.0),
    ));
}

/// Ce que le script a relevé.
#[derive(Resource, Default)]
struct Report {
    /// Images écoulées depuis `Ready`.
    frame: u32,
    /// Luminance moyenne de chaque capture, dans l'ordre.
    lumas: Vec<(u32, String, f32)>,
    /// Tailles `(niveau, rendu, sortie, surcouche)` relevées en fin de palier.
    sizes: Vec<(usize, UVec2, UVec2, UVec2)>,
    late_max: usize,
    done: bool,
}

fn capture(commands: &mut Commands, label: String, t: u32) {
    commands.spawn(Screenshot::primary_window()).observe(
        move |captured: On<ScreenshotCaptured>, mut report: ResMut<Report>| {
            if let Ok(image) = captured.image.clone().try_into_dynamic() {
                let rgba = image.to_rgba8();
                let n = (rgba.width() * rgba.height()) as f32;
                let sum: f32 = rgba
                    .pixels()
                    .map(|p| 0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2]))
                    .sum();
                report.lumas.push((t, label.clone(), sum / n));
            }
        },
    );
}

/// Une capture pendant le chargement : le pipeline de copie vers l'écran est celui des captures, pas de la qualité ;
/// il doit être compilé avant `Ready` pour que le détecteur ne le compte pas.
fn precapture(mut commands: Commands, mut done: Local<bool>) {
    if !*done {
        *done = true;
        commands.spawn(Screenshot::primary_window());
    }
}

#[allow(clippy::too_many_arguments, reason = "système Bevy")]
fn run_check(
    mut commands: Commands,
    mut report: ResMut<Report>,
    mut mode: ResMut<QualityMode>,
    late: Res<LatePipelines>,
    targets: Res<LayerTargets>,
    images: Res<Assets<Image>>,
    scale: Res<RenderScale>,
    levers: Res<QualityLevers>,
    mut exit: MessageWriter<AppExit>,
) {
    const SETTLE: u32 = 30;
    report.late_max = report.late_max.max(late.0);
    report.frame += 1;
    let frame = report.frame;
    if frame < SETTLE {
        return;
    }
    let t = frame - SETTLE;
    let step = (t / HOLD) as usize;
    let within = t % HOLD;
    if t.is_multiple_of(EVERY) && step <= SCRIPT.len() {
        let level = SCRIPT.get(step).map_or(0, |l| *l);
        capture(&mut commands, format!("t={t:3} niveau {level}"), t);
    }
    let Some(&level) = SCRIPT.get(step) else {
        // Laisse le temps aux dernières captures (asynchrones) d'arriver.
        if t > HOLD * SCRIPT.len() as u32 + 30 && !report.done {
            report.done = true;
            exit.write(verdict(&report));
        }
        return;
    };
    if within == 0 {
        *mode = QualityMode::Fixed(level);
    }
    if within == HOLD - 1 {
        let size = |layer| images.get(targets.image(layer)).map_or(UVec2::ZERO, Image::size);
        let sizes = (level, size(Layer::World), targets.size(), size(Layer::Overlay));
        println!(
            "niveau {level} : échelle {:.2}, leviers {:.2}, monde {}×{}, surcouche {}×{}, pipelines tardifs {}",
            scale.0, levers.render_scale, sizes.1.x, sizes.1.y, sizes.3.x, sizes.3.y, late.0
        );
        report.sizes.push(sizes);
    }
}

fn verdict(report: &Report) -> AppExit {
    let mut ok = true;
    println!("--- verdicts ---");
    println!("pipelines compilés après Ready : {}", report.late_max);
    ok &= report.late_max == 0;

    // Les captures arrivent peut-être dans le désordre : on trie par image.
    let mut lumas = report.lumas.clone();
    lumas.sort_by_key(|l| l.0);
    let mut max_step = (0.0_f32, String::new());
    for pair in lumas.windows(2) {
        let step = (pair[1].2 - pair[0].2).abs() / pair[0].2.max(1.0);
        if step > max_step.0 {
            max_step = (step, pair[1].1.clone());
        }
    }
    let first = lumas.first().map_or(0.0, |l| l.2);
    let lowest = lumas.iter().map(|l| l.2).fold(f32::MAX, f32::min);
    println!(
        "luminance moyenne : départ {first:.2}, minimum {lowest:.2} ({} captures)",
        lumas.len()
    );
    println!(
        "plus grande variation entre deux captures ({EVERY} images) : {:.2} % ({}), limite {:.0} %",
        max_step.0 * 100.0,
        max_step.1,
        MAX_STEP * 100.0
    );
    ok &= max_step.0 <= MAX_STEP && lumas.len() >= 100;

    for &(level, render, output, overlay) in &report.sizes {
        let wanted_scale = cf_render::PIXEL_SCALE[level];
        let wanted = UVec2::new(
            (output.x as f32 * wanted_scale).round() as u32,
            (output.y as f32 * wanted_scale).round() as u32,
        );
        let good = render == wanted && overlay == output;
        println!("niveau {level} : couches 3D {render}, surcouche {overlay}, sortie {output} → {good}");
        ok &= good;
    }
    println!("{}", if ok { "TOUT EST VERT" } else { "ÉCHEC" });
    if ok { AppExit::Success } else { AppExit::error() }
}
