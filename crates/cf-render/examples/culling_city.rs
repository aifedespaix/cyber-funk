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
//! monde est allumée, pour que les chiffres ne mélangent pas le fond ni la surcouche. Le pas de simulation est fixe
//! (1/60 s par image) ; le temps d'image mesuré est, lui, le temps réel.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).
//! Les millisecondes GPU de `lavapipe` sont celles d'un processeur qui émule un GPU : elles comparent les modes
//! entre eux, elles ne disent rien des performances sur une carte graphique.

// Le résultat de la mesure est la sortie du programme : des lignes `RESULT` / `PASS` / `DIAG` que les scripts lisent.
#![allow(clippy::print_stdout, clippy::print_stderr)]
// `NoFrustumCulling` n'est importé que pour le mode de référence `none` (voir `tag_pieces`, où est écrit pourquoi aucun
// `Aabb` ne conviendrait).

use bevy::camera::visibility::{NoFrustumCulling, ViewVisibility};
use bevy::diagnostic::DiagnosticsStore;
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};
use bevy::window::{PresentMode, WindowResolution};
use cf_render::streaming::StreamedPiece;
use cf_render::{
    AbsolutePosition, BoxWorld, CameraPose, CfSet, CorePlugin, CullingPlugin, CullingSettings, DiagnosticsPlugin,
    DiagnosticsSnapshot, Layer, LayerOpacity, LayersPlugin, MeasurementSession, StreamFocus, WorldLayer, WorldPlugin,
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
            CorePlugin,
            // Surcouche de diagnostic (J4-05) : temps par passe, entités visibles, session CSV.
            DiagnosticsPlugin::default(),
            LayersPlugin::default(),
            WorldPlugin::new(BoxWorld::new(7).with_rows_per_chunk(rows).with_fog_cull(fog)),
            CullingPlugin { settings },
        ))
        .insert_resource(Demo {
            mode,
            aerial: std::env::var("CF_VIEW").is_ok_and(|v| v == "aerial"),
            z: 0.0,
            frame: 0,
            measured_frames,
            snapshot: std::env::var_os("CF_SNAPSHOT"),
            visible: Vec::new(),
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
    commands.spawn((Car, StreamFocus, AbsolutePosition(DVec3::new(0.0, 1.0, 0.0))));
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

fn measure(
    mut commands: Commands,
    mut demo: ResMut<Demo>,
    pieces: Query<&ViewVisibility, With<StreamedPiece>>,
    diagnostics: Res<DiagnosticsStore>,
    snapshot: Res<DiagnosticsSnapshot>,
    mut session: ResMut<MeasurementSession>,
    time: Res<Time<Real>>,
    mut exit: MessageWriter<AppExit>,
) {
    demo.frame += 1;
    if demo.frame <= WARMUP_FRAMES {
        return;
    }
    let Some(measured) = demo.measured_frames else {
        return;
    };
    let index = demo.frame - WARMUP_FRAMES;
    if index == 1 {
        session.start(time.elapsed_secs_f64());
    }
    if index <= measured {
        demo.visible.push(pieces.iter().filter(|v| v.get()).count() as u32);
        if index == measured / 2
            && let Some(path) = demo.snapshot.take()
        {
            commands.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
        }
        return;
    }
    session.stop();
    // Quelques images de plus : la capture partie à mi-mesure arrive avec un peu de retard.
    if index < measured + 20 {
        return;
    }
    report(&demo, &snapshot, &session, &diagnostics);
    if let Some(path) = std::env::var_os("CF_CSV") {
        match session.save_csv(std::path::Path::new(&path)) {
            Ok(rows) => println!("CSV {rows} lignes dans {}", std::path::Path::new(&path).display()),
            Err(error) => eprintln!("export CSV impossible : {error}"),
        }
    }
    exit.write(AppExit::Success);
}

fn mean(values: impl Iterator<Item = f32>) -> f32 {
    let (sum, n) = values.fold((0.0, 0_u32), |(s, n), v| (s + v, n + 1));
    sum / n.max(1) as f32
}

fn report(demo: &Demo, snapshot: &DiagnosticsSnapshot, session: &MeasurementSession, diagnostics: &DiagnosticsStore) {
    let stream = snapshot.stream.unwrap_or_default();
    let rows = session.rows();
    let mut frame_ms: Vec<f32> = rows.iter().map(|r| r.frame_ms).collect();
    frame_ms.sort_by(f32::total_cmp);
    let p99 = frame_ms
        .get(frame_ms.len().saturating_sub(1) * 99 / 100)
        .copied()
        .unwrap_or(0.0);
    let gpu: Vec<f32> = rows.iter().filter_map(|r| r.gpu_ms).map(|g| g as f32).collect();
    println!(
        "RESULT mode={} view={} frames={} pooled={} placed={} pieces_visible_mean={:.0} pieces_visible_min={} \
         pieces_visible_max={} renderable={} visible_entities_mean={:.0} frame_ms_mean={:.2} frame_ms_p99={p99:.2} \
         gpu_ms_mean={:.2} draw_batches_lower_bound={}",
        demo.mode.name(),
        if demo.aerial { "aerial" } else { "street" },
        rows.len(),
        stream.pieces_pooled,
        stream.pieces_used,
        mean(demo.visible.iter().map(|&v| v as f32)),
        demo.visible.iter().min().copied().unwrap_or(0),
        demo.visible.iter().max().copied().unwrap_or(0),
        snapshot.renderable_entities,
        mean(rows.iter().map(|r| r.visible_entities as f32)),
        mean(frame_ms.iter().copied()),
        mean(gpu.iter().copied()),
        snapshot.visible_mesh_kinds,
    );
    // Passes de premier niveau, vue de la surcouche (dernière image).
    for pass in snapshot.passes.iter().filter(|p| p.is_top_level()) {
        println!("PASS {} cpu_ms={:?} gpu_ms={:?}", pass.name, pass.cpu_ms, pass.gpu_ms);
    }
    // Compteurs de pipeline du GPU, moyennés sur l'historique de Bevy : sommets traités par passe. Un immeuble est un
    // cube de 24 sommets : sommets / 24 estime les immeubles réellement dessinés (occlusion comprise).
    let mut lines: Vec<String> = diagnostics
        .iter()
        .filter_map(|d| {
            let path = d.path().to_string();
            path.contains("vertex_shader_invocations")
                .then(|| format!("DIAG {path} avg={:.1}", d.average().unwrap_or(f64::NAN)))
        })
        .collect();
    lines.sort();
    for line in lines {
        println!("{line}");
    }
}
