//! La ville de boîtes sous les quatre niveaux de culling, pour mesurer l'avant/après de J4-04.
//!
//! ```text
//! cargo run -p cf-render --example culling_city --features demo                  # mode complet, rue
//! CF_MODE=none CF_MEASURE=1 cargo run ... --example culling_city --features demo # une mesure puis quitte
//! ```
//!
//! Réglages (variables d'environnement) :
//!
//! | Variable        | Valeurs                              | Défaut   |
//! | --------------- | ------------------------------------ | -------- |
//! | `CF_MODE`       | `none`, `frustum`, `range`, `full`   | `full`   |
//! | `CF_VIEW`       | `street` (à 4 m), `aerial` (à 150 m) | `street` |
//! | `CF_ROWS`       | rangées d'immeubles par tronçon      | `13`     |
//! | `CF_MEASURE`    | nombre d'images mesurées, puis quitte | (absent) |
//! | `CF_SNAPSHOT`   | chemin d'un PNG de la fenêtre        | (absent) |
//!
//! Les modes, du moins au plus économe :
//!
//! - `none` : **l'état de Sillage** : `frustumCulled = false`, tout est dessiné à chaque image (`NoFrustumCulling`,
//!   voulu ici pour la mesure de référence, voir le commentaire du code) ;
//! - `frustum` : le frustum natif de Bevy, rien d'autre ;
//! - `range` : + `VisibilityRange` (rien au-delà de la brume) ;
//! - `full` : + occlusion GPU sur la caméra du monde (avec `DepthPrepass`).
//!
//! Une voiture invisible roule à 40 m/s, la caméra la suit : le monde se recycle pendant la mesure. Seule la couche
//! monde est allumée, pour que les chiffres ne mélangent pas le fond ni la surcouche. Le pas de simulation est fixe.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).
//! Les millisecondes GPU de `lavapipe` sont celles d'un processeur qui émule un GPU : elles comparent les modes
//! entre eux, elles ne disent rien des performances sur une carte graphique.

use std::time::Instant;

use bevy::camera::visibility::{NoFrustumCulling, ViewVisibility};
use bevy::diagnostic::DiagnosticsStore;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::diagnostic::RenderDiagnosticsPlugin;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::time::TimeUpdateStrategy;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::streaming::StreamedPiece;
use cf_render::{
    AbsolutePosition, BoxWorld, CameraPose, CfSet, CorePlugin, CullingPlugin, CullingSettings, Layer, LayerOpacity,
    LayersPlugin, StreamFocus, StreamStats, WorldLayer, WorldPlugin,
};

const SPEED_MPS: f64 = 40.0;
const DT_S: f64 = 1.0 / 60.0;
/// Images ignorées avant la mesure : compilation des pipelines, remplissage de la fenêtre de tronçons.
const WARMUP_FRAMES: u32 = 150;
const DEFAULT_MEASURED_FRAMES: u32 = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    None,
    Frustum,
    Range,
    Full,
}

impl Mode {
    fn from_env() -> Self {
        match std::env::var("CF_MODE").as_deref() {
            Ok("none") => Self::None,
            Ok("frustum") => Self::Frustum,
            Ok("range") => Self::Range,
            _ => Self::Full,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Frustum => "frustum",
            Self::Range => "range",
            Self::Full => "full",
        }
    }
}

#[derive(Resource)]
struct Demo {
    mode: Mode,
    aerial: bool,
    z: f64,
    frame: u32,
    measured_frames: Option<u32>,
    snapshot: Option<std::ffi::OsString>,
    visible: Vec<u32>,
    frame_ms: Vec<f32>,
    last: Option<Instant>,
}

#[derive(Component)]
struct Car;

fn main() {
    let mode = Mode::from_env();
    let rows: u16 = std::env::var("CF_ROWS").ok().and_then(|s| s.parse().ok()).unwrap_or(13);
    let measured_frames = std::env::var("CF_MEASURE")
        .ok()
        .map(|s| s.parse().unwrap_or(DEFAULT_MEASURED_FRAMES));
    let fog = matches!(mode, Mode::Range | Mode::Full).then_some(cf_render::FOG_FULL_M);
    let settings = CullingSettings {
        occlusion: mode == Mode::Full,
        ..default()
    };
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("cyber-funk · culling · {}", mode.name()),
                resolution: WindowResolution::new(640, 360),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((
            RenderDiagnosticsPlugin,
            CorePlugin,
            LayersPlugin::default(),
            WorldPlugin::new(BoxWorld::new(7).with_rows_per_chunk(rows).with_fog_cull(fog)),
            CullingPlugin { settings },
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f64(DT_S)))
        .insert_resource(Demo {
            mode,
            aerial: std::env::var("CF_VIEW").is_ok_and(|v| v == "aerial"),
            z: 0.0,
            frame: 0,
            measured_frames,
            snapshot: std::env::var_os("CF_SNAPSHOT"),
            visible: Vec::new(),
            frame_ms: Vec::new(),
            last: None,
        })
        .add_systems(Startup, spawn_scene)
        .add_systems(Update, (tag_pieces, drive.in_set(CfSet::Direct), measure).chain())
        .run();
}

fn spawn_scene(mut commands: Commands, mut opacity: ResMut<LayerOpacity>) {
    // Seule la couche monde : les chiffres ne mélangent ni le fond ni la surcouche.
    for layer in [Layer::Background, Layer::Foreground, Layer::Overlay] {
        opacity.set(layer, 0.0);
    }
    commands.spawn((
        Car,
        StreamFocus,
        AbsolutePosition(DVec3::new(0.0, 1.0, 0.0)),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            ..default()
        },
        bevy::camera::visibility::RenderLayers::layer(Layer::World.index()),
        Transform::from_xyz(3.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 600.0,
        ..default()
    });
}

/// Les pièces du streaming vont dans la couche monde (le rig ne rend que cette couche-là pour la caméra du monde).
/// `BoxWorld` ne le fait pas lui-même : voir « Écarts » de la PR.
fn tag_pieces(mut commands: Commands, demo: Res<Demo>, pieces: Query<Entity, Added<StreamedPiece>>) {
    for entity in &pieces {
        let mut piece = commands.entity(entity);
        piece.insert(WorldLayer);
        if demo.mode == Mode::None {
            // Mesure de référence uniquement : on veut l'état de Sillage (`frustumCulled = false`), où tout est
            // dessiné à chaque image. Aucun `Aabb` ne « conviendrait » : c'est précisément l'absence de culling qu'on
            // mesure, et cet exemple n'est pas du code de show.
            piece.insert(NoFrustumCulling);
        }
    }
}

fn drive(mut demo: ResMut<Demo>, mut car: Query<&mut AbsolutePosition, With<Car>>, mut pose: ResMut<CameraPose>) {
    demo.z -= SPEED_MPS * DT_S;
    let position = DVec3::new(0.0, 1.0, demo.z);
    for mut p in &mut car {
        p.0 = position;
    }
    let (offset, pitch) = if demo.aerial {
        (DVec3::new(0.0, 150.0, 40.0), -0.45_f32)
    } else {
        (DVec3::new(0.0, 4.0, 14.0), -0.05_f32)
    };
    *pose = CameraPose {
        position: position + offset,
        rotation: Quat::from_rotation_x(pitch),
        fov_deg: 70.0,
        ..default()
    };
}

#[allow(clippy::too_many_arguments)]
fn measure(
    mut commands: Commands,
    mut demo: ResMut<Demo>,
    stats: Res<StreamStats>,
    pieces: Query<&ViewVisibility, With<StreamedPiece>>,
    diagnostics: Res<DiagnosticsStore>,
    mut exit: MessageWriter<AppExit>,
) {
    demo.frame += 1;
    let now = Instant::now();
    let frame_ms = demo.last.replace(now).map(|t| (now - t).as_secs_f32() * 1000.0);
    if demo.frame <= WARMUP_FRAMES {
        return;
    }
    let Some(measured) = demo.measured_frames else {
        return;
    };
    let index = demo.frame - WARMUP_FRAMES;
    if index <= measured {
        let visible = pieces.iter().filter(|v| v.get()).count() as u32;
        demo.visible.push(visible);
        if let Some(ms) = frame_ms {
            demo.frame_ms.push(ms);
        }
        if index == measured / 2
            && let Some(path) = demo.snapshot.take()
        {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        }
        return;
    }
    // Quelques images de plus : la capture partie à mi-mesure arrive avec un peu de retard.
    if index < measured + 20 {
        return;
    }
    report(&demo, &stats, &diagnostics);
    exit.write(AppExit::Success);
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len().max(1) as f32
}

fn report(demo: &Demo, stats: &StreamStats, diagnostics: &DiagnosticsStore) {
    let visible: Vec<f32> = demo.visible.iter().map(|&v| v as f32).collect();
    let mut sorted = demo.frame_ms.clone();
    sorted.sort_by(f32::total_cmp);
    let p99 = sorted.get(sorted.len().saturating_sub(1) * 99 / 100).copied().unwrap_or(0.0);
    println!(
        "RESULT mode={} view={} pooled={} placed={} cpu_visible_mean={:.0} cpu_visible_min={:.0} \
         cpu_visible_max={:.0} frame_ms_mean={:.2} frame_ms_p99={:.2}",
        demo.mode.name(),
        if demo.aerial { "aerial" } else { "street" },
        stats.pieces_pooled,
        stats.pieces_used,
        mean(&visible),
        visible.iter().copied().fold(f32::INFINITY, f32::min),
        visible.iter().copied().fold(0.0, f32::max),
        mean(&demo.frame_ms),
        p99,
    );
    // Chaque durée GPU / compteur que Bevy a mesuré, moyenné sur la fenêtre de l'historique.
    let mut lines: Vec<String> = diagnostics
        .iter()
        .filter_map(|d| {
            let path = d.path().to_string();
            (path.contains("elapsed_gpu") || path.contains("invocations") || path.contains("primitives"))
                .then(|| format!("DIAG {path} avg={:.4}", d.average().unwrap_or(f64::NAN)))
        })
        .collect();
    lines.sort();
    for line in lines {
        println!("{line}");
    }
}
