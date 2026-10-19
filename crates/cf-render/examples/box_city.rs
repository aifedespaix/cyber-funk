//! La ville de boîtes à l'écran, pour juger du tremblement à 400 km.
//!
//! ```text
//! cargo run -p cf-render --example box_city --features demo                    # départ à 400 km
//! CF_START_KM=0 cargo run -p cf-render --example box_city --features demo      # départ au km 0
//! CF_NO_ORIGIN=1 cargo run -p cf-render --example box_city --features demo     # sans origine flottante
//! CF_MEASURE=1 cargo run -p cf-render --example box_city --features demo       # mesure puis quitte
//! ```
//!
//! Une voiture (boîte verte, couleur absente de la ville) vole à 40 m/s, la caméra la suit à distance fixe : à
//! l'écran, la voiture ne doit pas bouger d'un pixel. Avec `CF_MEASURE=1`, l'exemple capture 90 images et mesure la
//! voiture à l'écran sur chacune : écart maximal de son barycentre, et nombre de pixels de sa silhouette qui changent
//! par rapport à la première image ; puis il quitte. `CF_SNAPSHOT=chemin.png` garde la première image mesurée.
//!
//! Sans origine flottante (`CF_NO_ORIGIN=1`), la voiture et la caméra sont rendues à 400 km de l'origine : la ville
//! ne suit pas (le point suivi est retiré), seule la voiture compte pour la mesure.
//!
//! Sans écran : `xvfb-run -a` et le rendu logiciel de Mesa (`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`).
//!
//! Le pas de simulation est fixe (1/60 s par image) : la mesure ne dépend pas de la vitesse de la machine.

use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};
use bevy::window::{PresentMode, WindowResolution};
use cf_render::{AbsolutePosition, BoxWorld, CorePlugin, StreamFocus, WorldPlugin};

const SPEED_MPS: f64 = 40.0;
const DT_S: f64 = 1.0 / 60.0;
/// La voiture vole à 5 m, au-dessus des voitures garées : rien ne passe jamais devant elle, sa silhouette ne change
/// que si le rendu tremble.
const CAR_HEIGHT_M: f64 = 5.0;
/// Caméra derrière la voiture. Volontairement pas un multiple de la grille des `f32` à 400 km (2⁻⁵ m) : sinon voiture
/// et caméra s'arrondiraient ensemble et le tremblement se cacherait.
const CAMERA_OFFSET: DVec3 = DVec3::new(0.37, 3.613, 12.537);
/// Images ignorées avant la mesure (compilation des pipelines, premier remplissage).
const WARMUP_FRAMES: u32 = 90;
const MEASURED_FRAMES: u32 = 90;

#[derive(Resource)]
struct Demo {
    z: f64,
    frame: u32,
    measure: bool,
    centroids: Vec<Vec2>,
    /// Silhouette de la voiture sur la première image mesurée, et le plus grand nombre de pixels qui en diffèrent.
    reference: Option<Vec<bool>>,
    max_flips: usize,
}

#[derive(Component)]
struct Car;

#[derive(Component)]
struct Rig;

fn main() {
    let start_km: f64 = std::env::var("CF_START_KM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(400.0);
    let no_origin = std::env::var_os("CF_NO_ORIGIN").is_some();
    let measure = std::env::var_os("CF_MEASURE").is_some();
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "cyber-funk · ville de boîtes".into(),
                resolution: WindowResolution::new(640, 360),
                present_mode: PresentMode::AutoNoVsync,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((CorePlugin, WorldPlugin::new(BoxWorld::new(7))))
        .insert_resource(ClearColor(Color::BLACK))
        .insert_resource(Demo {
            z: -start_km * 1000.0,
            frame: 0,
            measure,
            centroids: Vec::new(),
            reference: None,
            max_flips: 0,
        })
        .add_systems(
            Startup,
            move |commands: Commands,
                  meshes: ResMut<Assets<Mesh>>,
                  materials: ResMut<Assets<StandardMaterial>>,
                  demo: Res<Demo>| {
                spawn_scene(commands, meshes, materials, demo.z, no_origin);
            },
        )
        .add_systems(Update, (drive, measure_frame).chain())
        .run();
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    z: f64,
    no_origin: bool,
) {
    let car = DVec3::new(0.0, CAR_HEIGHT_M, z);
    let mut car_entity = commands.spawn((
        Car,
        Mesh3d(meshes.add(Cuboid::new(1.9, 1.3, 4.6))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.0, 1.0, 0.0),
            unlit: true,
            ..default()
        })),
        AbsolutePosition(car),
    ));
    if !no_origin {
        car_entity.insert(StreamFocus);
    }
    let eye = CAMERA_OFFSET.as_vec3();
    commands.spawn((
        Rig,
        Camera3d::default(),
        // Sans anticrénelage, un pixel est voiture ou fond : la silhouette ne dépend pas de ce qui passe derrière.
        Msaa::Off,
        Transform::from_translation(eye).looking_at(Vec3::ZERO, Vec3::Y),
        AbsolutePosition(car + CAMERA_OFFSET),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 2_000.0,
        ..default()
    });
}

/// Pas fixe : la voiture avance, la caméra la suit rigidement (positions absolues en `f64`).
fn drive(
    mut demo: ResMut<Demo>,
    mut car: Query<&mut AbsolutePosition, (With<Car>, Without<Rig>)>,
    mut rig: Query<&mut AbsolutePosition, (With<Rig>, Without<Car>)>,
) {
    demo.z -= SPEED_MPS * DT_S;
    let position = DVec3::new(0.0, CAR_HEIGHT_M, demo.z);
    for mut p in &mut car {
        p.0 = position;
    }
    for mut p in &mut rig {
        p.0 = position + CAMERA_OFFSET;
    }
}

fn measure_frame(mut commands: Commands, mut demo: ResMut<Demo>, mut exit: MessageWriter<AppExit>) {
    demo.frame += 1;
    if !demo.measure || demo.frame <= WARMUP_FRAMES {
        return;
    }
    if demo.frame <= WARMUP_FRAMES + MEASURED_FRAMES {
        let mut screenshot = commands.spawn(Screenshot::primary_window());
        screenshot.observe(record_centroid);
        // `CF_SNAPSHOT=chemin.png` : garde la première image mesurée, pour la regarder.
        if demo.frame == WARMUP_FRAMES + 1
            && let Some(path) = std::env::var_os("CF_SNAPSHOT")
        {
            screenshot.observe(save_to_disk(path));
        }
        return;
    }
    // Les captures arrivent avec une ou deux images de retard : on attend un peu avant de conclure.
    if demo.frame < WARMUP_FRAMES + MEASURED_FRAMES + 30 {
        return;
    }
    let n = demo.centroids.len();
    let mean = demo.centroids.iter().copied().sum::<Vec2>() / n.max(1) as f32;
    let worst = demo.centroids.iter().map(|c| c.distance(mean)).fold(0.0_f32, f32::max);
    info!(
        "mesure : {n} images, voiture à l'écran en {mean:?}, écart maximal au barycentre {worst:.3} px, \
         jusqu'à {} pixels de silhouette changés d'une image à l'autre",
        demo.max_flips
    );
    exit.write(AppExit::Success);
}

/// Barycentre (px) des pixels verts de la capture : la voiture à l'écran.
fn record_centroid(captured: On<ScreenshotCaptured>, mut demo: ResMut<Demo>) {
    let Ok(image) = captured.image.clone().try_into_dynamic() else {
        return;
    };
    let rgba = image.to_rgba8();
    let (mut sum, mut count) = (Vec2::ZERO, 0_u32);
    let mut mask = Vec::with_capacity(rgba.len() / 4);
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let [r, g, b, _] = pixel.0;
        let car = g > 100 && g > r.saturating_add(50) && g > b.saturating_add(50);
        if car {
            sum += Vec2::new(x as f32, y as f32);
            count += 1;
        }
        mask.push(car);
    }
    // Moins de 200 pixels : la voiture n'est pas encore dessinée (pipeline en compilation).
    if count <= 200 {
        return;
    }
    demo.centroids.push(sum / count as f32);
    match &demo.reference {
        Some(reference) => {
            let flips = reference.iter().zip(&mask).filter(|(a, b)| a != b).count();
            demo.max_flips = demo.max_flips.max(flips);
        }
        None => demo.reference = Some(mask),
    }
}
