//! Les quatre couches à l'écran : ciel dégradé, boîtes, logo d'avant-plan, titre 2D.
//!
//! ```text
//! cargo run -p cf-render --example layers_demo --features demo                 # la caméra traverse le mur, en boucle
//! CF_CHECK=1 cargo run -p cf-render --example layers_demo --features demo      # vérifie les critères sur des captures
//! CF_CHECK=1 CF_SNAPSHOT_DIR=/tmp/couches cargo run ...                        # garde aussi les captures (PNG)
//! ```
//!
//! En mode interactif, les touches 1 à 4 éteignent / rallument en fondu le fond, le monde, l'avant-plan et la
//! surcouche.
//!
//! Le monde est un rang de boîtes et un **mur** rouge plein cadre ; le logo d'avant-plan est vert, à 90 cm de
//! l'objectif, en repère caméra. Avec `CF_CHECK=1`, l'exemple place la caméra contre le mur ou dedans, règle les
//! opacités, capture l'écran à chaque étape et vérifie sur les pixels :
//!
//! 1. mur à 45 cm de l'objectif (devant le logo) : le logo reste visible, le reste de l'écran est rouge ;
//! 2. caméra dans le mur : le logo reste visible ;
//! 3. témoin : avant-plan éteint, un objet ordinaire du monde (magenta, à 90 cm) est bien masqué par le mur ;
//! 4. monde à 0 : le mur disparaît, la caméra du monde est désactivée ;
//! 5. monde à 0,5 : le mur est un mélange à parts égales du mur et du ciel (en lumière linéaire) ;
//! 6. surcouche à 0 : le titre disparaît, sa caméra est désactivée ;
//! 7. fenêtre élargie (640×360 → 800×360) : les textures de couche suivent, le titre garde sa taille en pixels.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).

mod check;

use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{
    BackgroundLayer, CameraPose, CfSet, CorePlugin, ForegroundLayer, Layer, LayerOpacity, LayersPlugin, OverlayLayer,
    WorldLayer,
};

/// Le mur : sa face avant est à `WALL_Z + WALL_DEPTH / 2`.
pub(crate) const WALL_Z: f32 = -10.0;
const WALL_DEPTH: f32 = 0.5;
/// Distance de l'objectif au mur dans la première étape, plus courte que celle du logo (`LOGO_DISTANCE`).
const WALL_GAP: f32 = 0.45;
const LOGO_DISTANCE: f32 = 0.9;

fn main() {
    let check = std::env::var_os("CF_CHECK").is_some();
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "cyber-funk · couches".into(),
            resolution: WindowResolution::new(640, 360),
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    }))
    .add_plugins((CorePlugin, LayersPlugin::default()))
    .insert_resource(Time::<Fixed>::from_hz(60.0))
    .add_systems(Startup, spawn_scene)
    .add_systems(Update, spin_logo);
    if check {
        app.init_resource::<check::Check>()
            .add_systems(Update, check::run_check.in_set(CfSet::Direct));
    } else {
        app.init_resource::<Fades>()
            .add_systems(Update, (fly, fade_keys).chain().in_set(CfSet::Direct));
    }
    app.run();
}

#[derive(Component)]
struct Logo;

fn unlit(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        unlit: true,
        ..default()
    }
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Ciel : voûte dégradée (violet profond au zénith, rose à l'horizon, nuit sous le sol), vue de l'intérieur.
    let mut sky = Sphere::new(1000.0).mesh().uv(32, 18);
    let colors: Option<Vec<[f32; 4]>> = match sky.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(positions)) => Some(
            positions
                .iter()
                .map(|p| {
                    let h = (p[1] / 1000.0).clamp(-1.0, 1.0);
                    let up = Vec3::new(0.06, 0.0, 0.30).lerp(Vec3::new(0.85, 0.15, 0.50), (1.0 - h.max(0.0)).powi(2));
                    let c = if h < 0.0 {
                        up.lerp(Vec3::new(0.02, 0.0, 0.05), (-h * 4.0).min(1.0))
                    } else {
                        up
                    };
                    [c.x, c.y, c.z, 1.0]
                })
                .collect(),
        ),
        _ => None,
    };
    if let Some(colors) = colors {
        sky.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
    commands.spawn((
        BackgroundLayer,
        Mesh3d(meshes.add(sky)),
        MeshMaterial3d(materials.add(StandardMaterial {
            unlit: true,
            cull_mode: None,
            ..default()
        })),
    ));

    // Monde : un rang de boîtes de part et d'autre de la route, un mur plein cadre, un objet témoin.
    let boxes = meshes.add(Cuboid::new(3.0, 8.0, 3.0));
    let box_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.55, 0.9),
        ..default()
    });
    for i in 0..16 {
        let z = -2.0 - 4.0 * i as f32;
        for side in [-1.0_f32, 1.0] {
            commands.spawn((
                WorldLayer,
                Mesh3d(boxes.clone()),
                MeshMaterial3d(box_material.clone()),
                Transform::from_xyz(side * 6.0, 4.0 + (i % 3) as f32, z),
            ));
        }
    }
    commands.spawn((
        WorldLayer,
        Mesh3d(meshes.add(Cuboid::new(120.0, 60.0, WALL_DEPTH))),
        MeshMaterial3d(materials.add(unlit(Color::srgb(1.0, 0.0, 0.0)))),
        Transform::from_xyz(0.0, 0.0, WALL_Z),
    ));
    // Témoin : un objet ordinaire du monde, à la place que le logo occupe par rapport à l'objectif de l'étape 1.
    commands.spawn((
        WorldLayer,
        Mesh3d(meshes.add(Cuboid::from_length(0.25))),
        MeshMaterial3d(materials.add(unlit(Color::srgb(1.0, 0.0, 1.0)))),
        Transform::from_xyz(0.0, 0.0, wall_pose_z() - LOGO_DISTANCE),
    ));
    // Les lumières ont leurs `RenderLayers` : celle-ci n'éclaire que le monde.
    commands.spawn((
        DirectionalLight {
            illuminance: 9_000.0,
            ..default()
        },
        RenderLayers::layer(Layer::World.index()),
        Transform::from_xyz(3.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 400.0,
        ..default()
    });

    // Avant-plan : le logo, en repère caméra (−z devant l'objectif), à 90 cm.
    commands.spawn((
        ForegroundLayer,
        Logo,
        Mesh3d(meshes.add(Cuboid::from_length(0.25))),
        MeshMaterial3d(materials.add(unlit(Color::srgb(0.0, 1.0, 0.0)))),
        Transform::from_xyz(0.0, 0.0, -LOGO_DISTANCE),
    ));

    // Surcouche 2D : le titre, en pixels depuis le centre.
    commands.spawn((
        OverlayLayer,
        Text2d::new("CYBER-FUNK"),
        TextFont {
            font_size: FontSize::Px(44.0),
            ..default()
        },
        TextColor(Color::WHITE),
        Transform::from_xyz(0.0, 130.0, 0.0),
    ));
}

/// `z` de la caméra quand le mur est à `WALL_GAP` de l'objectif.
fn wall_pose_z() -> f32 {
    WALL_Z + WALL_DEPTH / 2.0 + WALL_GAP
}

fn spin_logo(time: Res<Time>, mut logo: Query<&mut Transform, With<Logo>>) {
    for mut transform in &mut logo {
        transform.rotate_y(0.8 * time.delta_secs());
        transform.rotate_x(0.5 * time.delta_secs());
    }
}

pub(crate) fn pose_at(z: f32) -> CameraPose {
    CameraPose {
        position: DVec3::new(0.0, 0.0, f64::from(z)),
        ..default()
    }
}

// ───────────────────────── mode interactif ─────────────────────────

/// Opacité visée par couche (touches 1 à 4) ; l'opacité réelle la rejoint en fondu d'une seconde.
#[derive(Resource)]
struct Fades([f32; 4]);

impl Default for Fades {
    fn default() -> Self {
        Self([1.0; 4])
    }
}

/// La caméra va et vient le long de la route, mur compris.
fn fly(time: Res<Time>, mut pose: ResMut<CameraPose>) {
    let t = time.elapsed_secs();
    let z = -9.5 + 8.0 * (t * 0.35).sin();
    *pose = pose_at(z);
}

fn fade_keys(
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut fades: ResMut<Fades>,
    mut opacity: ResMut<LayerOpacity>,
) {
    for (layer, key) in Layer::ALL
        .into_iter()
        .zip([KeyCode::Digit1, KeyCode::Digit2, KeyCode::Digit3, KeyCode::Digit4])
    {
        if keys.just_pressed(key) {
            let target = &mut fades.0[layer.index()];
            *target = if *target > 0.5 { 0.0 } else { 1.0 };
        }
        let now = opacity.get(layer);
        let step = time.delta_secs();
        let next = now + (fades.0[layer.index()] - now).clamp(-step, step);
        if (next - now).abs() > f32::EPSILON {
            opacity.set(layer, next);
        }
    }
}
