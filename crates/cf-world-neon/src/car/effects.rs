//! Traînée lumineuse et fumée des pneus dans le monde : deux entités à maillage dynamique, réécrit en place à chaque
//! image (aucune allocation en régime établi), et leurs `Aabb` recalculés à chaque image.
//!
//! Les deux suivent la voiture : leur maillage change donc d'emplacement à chaque image et l'`Aabb` automatique de Bevy
//! (calculé une fois sur le premier maillage) serait faux ; on en pose un juste, à la main, sur la boîte des points vivants.

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::camera::primitives::Aabb;
use bevy::math::{DVec3, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, CameraPose, FloatingOrigin, WorldLayer};

use super::materials::{CarLightMaterial, LightKind};
use super::rig::{Car, CarRig, CarState};
use super::smoke::{self, DriveFeel, Smoke};
use super::systems::{CRUISE_SPEED_MPS, CarControls, CarSettings};
use super::trail::{self, Trail};

/// Hauteur de l'arrière de la voiture où part la traînée, m (`rear.set(carX, 0.5 + altitude, …)`).
const TRAIL_HEIGHT_M: f64 = 0.5;
/// Recul du point de départ de la traînée dans le pare-chocs, m (`car.halfLength - 0.1`).
const TRAIL_INSET_M: f32 = 0.1;
/// Graine du hasard de la fumée : le même show donne la même fumée.
const SMOKE_SEED: u64 = 0x0C0F_FEE5_EED5;

/// La bande de traînée : sa mémoire, son maillage et les tampons d'écriture.
#[derive(Component, Debug)]
pub struct CarTrail {
    trail: Trail,
    mesh: Handle<Mesh>,
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

/// La fumée : ses particules, son maillage et les tampons d'écriture.
#[derive(Component, Debug)]
pub struct CarSmoke {
    smoke: Smoke,
    mesh: Handle<Mesh>,
    positions: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    /// Des particules étaient vivantes à l'image précédente : le maillage doit encore être réécrit (une fois de plus,
    /// pour les effacer), puis plus rien tant qu'il n'y en a pas.
    drawn: bool,
}

fn dynamic_mesh(vertices: usize, uvs: Vec<[f32; 2]>, indices: Vec<u32>) -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; vertices])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 1.0, 0.0]; vertices])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; vertices])
        .with_inserted_indices(Indices::U32(indices))
}

/// Crée les entités de la traînée et de la fumée.
pub(super) fn spawn_effects(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<CarLightMaterial>,
) {
    let mut ribbon = CarLightMaterial::new(LightKind::Ribbon, 0.0);
    ribbon.set(LinearRgba::WHITE, 1.0);
    let mesh = meshes.add(dynamic_mesh(trail::VERTICES, vec![[0.0; 2]; trail::VERTICES], trail::indices()));
    commands.spawn((
        Name::new("traînée"),
        CarTrail {
            trail: Trail::default(),
            mesh: mesh.clone(),
            positions: vec![[0.0; 3]; trail::VERTICES],
            colors: vec![[0.0; 4]; trail::VERTICES],
        },
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(ribbon)),
        Transform::IDENTITY,
        Aabb::from_min_max(Vec3::splat(-0.5), Vec3::splat(0.5)),
        Visibility::default(),
        WorldLayer,
    ));

    let mut cloud = CarLightMaterial::new(LightKind::Smoke, 0.0);
    cloud.set(LinearRgba::WHITE, 1.0);
    let mesh = meshes.add(dynamic_mesh(smoke::VERTICES, smoke::uvs(), smoke::indices()));
    commands.spawn((
        Name::new("fumée des pneus"),
        CarSmoke {
            smoke: Smoke::new(SMOKE_SEED),
            mesh: mesh.clone(),
            positions: vec![[0.0; 3]; smoke::VERTICES],
            colors: vec![[0.0; 4]; smoke::VERTICES],
            drawn: false,
        },
        Mesh3d(mesh),
        MeshMaterial3d(materials.add(cloud)),
        Transform::IDENTITY,
        Aabb::from_min_max(Vec3::splat(-0.5), Vec3::splat(0.5)),
        Visibility::default(),
        WorldLayer,
    ));
}

/// Copie les tampons dans les attributs du maillage, en place.
fn upload(mesh: &mut Mesh, positions: &[[f32; 3]], colors: &[[f32; 4]]) {
    if let Some(VertexAttributeValues::Float32x3(target)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        target.copy_from_slice(positions);
    }
    if let Some(VertexAttributeValues::Float32x4(target)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        target.copy_from_slice(colors);
    }
}

fn aabb(bounds: Option<(Vec3, Vec3)>, margin: f32) -> Aabb {
    let (lo, hi) = bounds.unwrap_or((Vec3::ZERO, Vec3::ZERO));
    Aabb::from_min_max(lo - Vec3::splat(margin), hi + Vec3::splat(margin))
}

/// `CfSet::Present` : pose un point de traînée derrière la voiture, raccourcit la bande avec la vitesse et le freinage,
/// réécrit le maillage (repère rendu, relatif à l'origine flottante).
pub(super) fn update_trail(
    origin: Res<FloatingOrigin>,
    controls: Res<CarControls>,
    cars: Query<(&AbsolutePosition, &CarRig, &CarState), With<Car>>,
    mut lines: Query<(&mut CarTrail, &mut Aabb)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Ok((position, rig, state)), Ok((mut line, mut bounds))) = (cars.single(), lines.single_mut()) else {
        return;
    };
    let speed_ratio = (state.speed_mps / CRUISE_SPEED_MPS).max(0.0);
    let rear = position.0
        + DVec3::new(
            0.0,
            TRAIL_HEIGHT_M,
            f64::from(rig.frame.half_length() - TRAIL_INSET_M),
        );
    let tint = state.glow.current();
    line.trail.push(rear, tint);
    line.trail
        .set_length((0.3 + speed_ratio.min(1.0) * 0.7) * (1.0 - controls.brake * 0.35));
    let line = &mut *line;
    let written = line.trail.write(origin.origin(), &mut line.positions, &mut line.colors);
    if let Some(mut mesh) = meshes.get_mut(&line.mesh) {
        upload(&mut mesh, &line.positions, &line.colors);
    }
    *bounds = aabb(written, 0.5);
}

/// `CfSet::Present` : émet de la fumée selon la conduite, avance les particules, réécrit le maillage.
pub(super) fn update_smoke(
    time: Res<Time>,
    origin: Res<FloatingOrigin>,
    controls: Res<CarControls>,
    settings: Res<CarSettings>,
    pose: Option<Res<CameraPose>>,
    cars: Query<(&AbsolutePosition, &CarState), With<Car>>,
    mut clouds: Query<(&mut CarSmoke, &mut Aabb)>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (Ok((position, state)), Ok((mut cloud, mut bounds))) = (cars.single(), clouds.single_mut()) else {
        return;
    };
    let dt = time.delta_secs();
    let speed_ratio = (state.speed_mps / CRUISE_SPEED_MPS).max(0.0);
    let feel = DriveFeel::measure(state.speed_mps, speed_ratio, state.lateral_mps, state.accel_mps2, controls.brake);
    // Centre de la voiture au sol.
    let ground = DVec3::new(position.0.x, 0.0, position.0.z);
    cloud.smoke.emit_for(dt, ground, state.speed_mps, &feel, settings.smoke_density);
    cloud.smoke.update(dt);
    if cloud.smoke.alive() == 0 && !cloud.drawn {
        return;
    }
    let rotation = pose.map_or(Quat::IDENTITY, |p| p.rotation);
    let axes = (rotation * Vec3::X, rotation * Vec3::Y);
    let tint = state.glow.current();
    let cloud = &mut *cloud;
    let written = cloud
        .smoke
        .write(origin.origin(), axes, tint, &mut cloud.positions, &mut cloud.colors);
    cloud.drawn = written.is_some();
    if let Some(mut mesh) = meshes.get_mut(&cloud.mesh) {
        upload(&mut mesh, &cloud.positions, &cloud.colors);
    }
    *bounds = aabb(written, 0.5);
}
