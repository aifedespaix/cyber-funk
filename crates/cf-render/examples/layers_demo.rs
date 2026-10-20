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

use std::collections::HashMap;

use bevy::camera::Camera;
use bevy::camera::visibility::RenderLayers;
use bevy::math::DVec3;
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{PresentMode, PrimaryWindow, WindowResolution};
use cf_render::{
    BackgroundLayer, CameraPose, CfSet, CorePlugin, ForegroundLayer, Layer, LayerCamera, LayerOpacity, LayersPlugin,
    OverlayLayer, WorldLayer,
};

/// Le mur : sa face avant est à `WALL_Z + WALL_DEPTH / 2`.
const WALL_Z: f32 = -10.0;
const WALL_DEPTH: f32 = 0.5;
/// Distance de l'objectif au mur dans la première étape, plus courte que celle du logo (`LOGO_DISTANCE`).
const WALL_GAP: f32 = 0.45;
const LOGO_DISTANCE: f32 = 0.9;
const WARMUP_FRAMES: u32 = 90;
/// Une étape : on règle l'état, on attend qu'il s'applique, on capture, on attend la capture.
const SETTLE_FRAMES: u32 = 12;
const PHASE_FRAMES: u32 = 22;

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
        app.init_resource::<Check>()
            .add_systems(Update, run_check.in_set(CfSet::Direct));
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

fn pose_at(z: f32) -> CameraPose {
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

// ───────────────────────── mode vérification ─────────────────────────

/// Une étape de la vérification : pose, opacités (fond, monde, avant-plan, surcouche) et ce qu'on en attend.
struct Phase {
    name: &'static str,
    z: f32,
    opacity: [f32; 4],
}

const PHASES: [Phase; 7] = [
    Phase {
        name: "1-mur-devant-le-logo",
        z: -9.3,
        opacity: [1.0; 4],
    },
    Phase {
        name: "2-camera-dans-le-mur",
        z: WALL_Z,
        opacity: [1.0; 4],
    },
    Phase {
        name: "3-temoin-sans-avant-plan",
        z: -9.3,
        opacity: [1.0, 1.0, 0.0, 1.0],
    },
    Phase {
        name: "4-monde-a-0",
        z: -9.3,
        opacity: [1.0, 0.0, 1.0, 1.0],
    },
    Phase {
        name: "5-monde-a-0,5",
        z: -9.3,
        opacity: [1.0, 0.5, 1.0, 1.0],
    },
    Phase {
        name: "6-surcouche-a-0",
        z: -9.3,
        opacity: [1.0, 1.0, 1.0, 0.0],
    },
    Phase {
        name: "7-fenetre-elargie",
        z: -9.3,
        opacity: [1.0; 4],
    },
];
/// Étape où la fenêtre est élargie, et sa nouvelle taille (largeur seule : l'aspect change, le titre ne doit pas).
const RESIZE_PHASE: usize = 6;
const RESIZED: (u32, u32) = (800, 360);

/// Une capture d'écran : RGBA, ligne par ligne depuis le coin haut gauche.
struct Shot {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl Shot {
    fn pixel(&self, x: u32, y: u32) -> [u8; 3] {
        let i = 4 * (y as usize * self.width as usize + x as usize);
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2]]
    }
}

#[derive(Resource, Default)]
struct Check {
    frame: u32,
    shots: HashMap<&'static str, Shot>,
    /// Caméras actives au moment de la capture, par étape : (fond, monde, avant-plan, surcouche).
    active: HashMap<&'static str, [bool; 4]>,
}

fn run_check(
    mut commands: Commands,
    mut check: ResMut<Check>,
    mut pose: ResMut<CameraPose>,
    mut opacity: ResMut<LayerOpacity>,
    cameras: Query<(&LayerCamera, &Camera)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut exit: MessageWriter<AppExit>,
) {
    check.frame += 1;
    let Some(frame) = check.frame.checked_sub(WARMUP_FRAMES) else {
        return;
    };
    let index = (frame / PHASE_FRAMES) as usize;
    let at = frame % PHASE_FRAMES;
    let Some(phase) = PHASES.get(index) else {
        // Les dernières captures arrivent avec une ou deux images de retard.
        if at > 8 {
            exit.write(evaluate(&check));
        }
        return;
    };
    if index == RESIZE_PHASE && at == 0 {
        window.resolution.set(RESIZED.0 as f32, RESIZED.1 as f32);
    }
    *pose = pose_at(phase.z);
    for (layer, value) in Layer::ALL.into_iter().zip(phase.opacity) {
        opacity.set(layer, value);
    }
    if at == SETTLE_FRAMES {
        let mut active = [false; 4];
        for (LayerCamera(layer), camera) in &cameras {
            active[layer.index()] = camera.is_active;
        }
        check.active.insert(phase.name, active);
        let name = phase.name;
        let mut shot = commands.spawn(Screenshot::primary_window());
        shot.observe(move |captured: On<ScreenshotCaptured>, mut check: ResMut<Check>| {
            if let Ok(image) = captured.image.clone().try_into_dynamic() {
                let rgba = image.to_rgba8();
                let (width, height) = (rgba.width(), rgba.height());
                check.shots.insert(
                    name,
                    Shot {
                        width,
                        height,
                        rgba: rgba.into_raw(),
                    },
                );
            }
        });
        if let Some(dir) = std::env::var_os("CF_SNAPSHOT_DIR") {
            let dir = std::path::PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            shot.observe(save_to_disk(dir.join(format!("{name}.png"))));
        }
    }
}

/// Pixel (0 à 1 depuis le coin haut gauche) en `[r, g, b]`.
fn at(image: &Shot, x: f32, y: f32) -> [u8; 3] {
    let px = ((x * image.width as f32) as u32).min(image.width - 1);
    let py = ((y * image.height as f32) as u32).min(image.height - 1);
    image.pixel(px, py)
}

fn is_green([r, g, b]: [u8; 3]) -> bool {
    g > 200 && r < 60 && b < 60
}

fn is_red([r, g, b]: [u8; 3]) -> bool {
    r > 200 && g < 40 && b < 40
}

/// Pixels quasi blancs de la bande du haut, là où se trouve le titre.
fn white_pixels(image: &Shot) -> usize {
    (0..image.height / 4)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| image.pixel(x, y).iter().all(|&c| c > 215))
        .count()
}

/// Largeur (px) du rectangle qui entoure les pixels quasi blancs de la bande du haut : la largeur du titre.
fn title_width(image: &Shot) -> u32 {
    let xs: Vec<u32> = (0..image.height / 4)
        .flat_map(|y| (0..image.width).map(move |x| (x, y)))
        .filter(|&(x, y)| image.pixel(x, y).iter().all(|&c| c > 215))
        .map(|(x, _)| x)
        .collect();
    match (xs.iter().min(), xs.iter().max()) {
        (Some(min), Some(max)) => max - min + 1,
        _ => 0,
    }
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn evaluate(check: &Check) -> AppExit {
    let mut failures = 0;
    let mut verdict = |ok: bool, what: &str| {
        info!("{} {what}", if ok { "OK    " } else { "ÉCHEC " });
        failures += usize::from(!ok);
    };
    let shot = |name: &str| check.shots.iter().find(|(k, _)| k.starts_with(name)).map(|(_, v)| v);
    let active = |name: &str| check.active.iter().find(|(k, _)| k.starts_with(name)).map(|(_, v)| *v);
    let (Some(wall), Some(inside), Some(control), Some(off), Some(half), Some(no_title), Some(wide)) = (
        shot("1-"),
        shot("2-"),
        shot("3-"),
        shot("4-"),
        shot("5-"),
        shot("6-"),
        shot("7-"),
    ) else {
        warn!("captures manquantes : {:?}", check.shots.keys().collect::<Vec<_>>());
        return AppExit::from_code(2);
    };
    let corner = (0.08, 0.5);
    let center = (0.5, 0.5);

    let c = at(wall, center.0, center.1);
    verdict(
        is_green(c),
        &format!("1. mur à 45 cm : le logo d'avant-plan reste visible au centre ({c:?})"),
    );
    let r = at(wall, corner.0, corner.1);
    verdict(
        is_red(r),
        &format!("1. mur à 45 cm : le reste de l'écran est le mur ({r:?})"),
    );
    verdict(
        white_pixels(wall) > 100,
        &format!("1. le titre 2D est dessiné ({} pixels blancs)", white_pixels(wall)),
    );

    let c = at(inside, center.0, center.1);
    verdict(
        is_green(c),
        &format!("2. caméra dans le mur : le logo reste visible ({c:?})"),
    );

    let c = at(control, center.0, center.1);
    verdict(
        is_red(c),
        &format!("3. témoin : un objet du monde à 90 cm est masqué par le mur ({c:?})"),
    );

    let sky = at(off, corner.0, corner.1);
    verdict(
        !is_red(sky) && sky[2] > 20,
        &format!("4. monde à 0 : le mur a disparu, on voit le ciel ({sky:?})"),
    );
    verdict(
        is_green(at(off, center.0, center.1)),
        "4. monde à 0 : le logo reste visible",
    );
    verdict(
        active("4-") == Some([true, false, true, true]),
        &format!(
            "4. monde à 0 : sa caméra est désactivée, les autres actives ({:?})",
            active("4-")
        ),
    );

    // Mélange en lumière linéaire : 0,5 × mur + 0,5 × ciel.
    let mixed = at(half, corner.0, corner.1);
    let wall_px = r;
    let expected: Vec<f32> = (0..3)
        .map(|i| 255.0 * linear_to_srgb(0.5 * srgb_to_linear(wall_px[i]) + 0.5 * srgb_to_linear(sky[i])))
        .collect();
    let worst = (0..3)
        .map(|i| (f32::from(mixed[i]) - expected[i]).abs())
        .fold(0.0, f32::max);
    verdict(
        worst <= 6.0,
        &format!("5. monde à 0,5 : mélange mur/ciel attendu {expected:?}, mesuré {mixed:?} (écart max {worst:.1}/255)"),
    );
    verdict(
        active("5-") == Some([true; 4]),
        "5. monde à 0,5 : la couche rend toujours",
    );

    verdict(
        white_pixels(no_title) == 0,
        &format!(
            "6. surcouche à 0 : plus de titre ({} pixels blancs)",
            white_pixels(no_title)
        ),
    );
    verdict(
        active("6-") == Some([true, true, true, false]),
        &format!("6. surcouche à 0 : sa caméra est désactivée ({:?})", active("6-")),
    );

    let (narrow_width, wide_width) = (title_width(wall), title_width(wide));
    verdict(
        wide.width == RESIZED.0 && wide.height == RESIZED.1,
        &format!(
            "7. la capture a la taille de la fenêtre élargie ({}×{})",
            wide.width, wide.height
        ),
    );
    verdict(
        narrow_width > 100 && narrow_width.abs_diff(wide_width) <= 3,
        &format!("7. les textures suivent la fenêtre : titre de {narrow_width} px avant, {wide_width} px après"),
    );
    verdict(
        is_green(at(wide, center.0, center.1)) && is_red(at(wide, corner.0, corner.1)),
        "7. l'image élargie est complète (logo et mur)",
    );

    if failures == 0 {
        info!("vérification : tout est bon");
        AppExit::Success
    } else {
        warn!("vérification : {failures} échec(s)");
        AppExit::from_code(1)
    }
}
