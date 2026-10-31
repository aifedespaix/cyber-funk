//! Les lumières que le code ajoute à la voiture : volumes de phares, flaques sur la route, halos autour des optiques,
//! projecteurs (`CarLights` avec `fixtures = false` et `CyberGlow` de Sillage).
//!
//! Chaque entité porte `WorldLayer` ; les lumières de Bevy `Layer::World.render_layers()`. Les halos (carrés tournés vers
//! la caméra) ont un `Aabb` plus large que leur plan : ils occupent un volume.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::color::LinearRgba;
use bevy::light::{PointLight, SpotLight};
use bevy::math::{Mat4, Vec3};
use bevy::prelude::*;
use cf_render::{Layer, WorldLayer};

use super::frame::CarFrame;
use super::glow::NEON_INTENSITY;
use super::materials::{CarLightMaterial, LightKind};
use super::model::{CarModelAsset, Fore, MaterialRole, Side};
use super::rig::{Emissive, HaloRig, LightRig, SignalRig};
use super::shapes;
use super::spawn::CarAssets;

/// Teinte du bleu-blanc des projecteurs (`0xcfe9ff` de Sillage), en lumière linéaire.
const SPOT_COLOR: LinearRgba = LinearRgba::rgb(0.624, 0.807, 1.0);
/// Candela → lumens (Bevy) : `lumens = candela × 4π`.
const LUMENS_PER_CD: f32 = 4.0 * std::f32::consts::PI;
/// Biais de profondeur des flaques (`polygonOffsetFactor: -2` de Sillage) : au-dessus de la route sans scintiller.
const POOL_DEPTH_BIAS: f32 = -2.0;

/// Position des optiques dans le repère de la voiture, mesurée sur les sommets du modèle **avant** que ses maillages
/// ne passent dans les `Assets`.
#[derive(Debug)]
pub(super) struct Lamps {
    heads: Vec<Vec3>,
    tails: Vec<Vec3>,
    signals: Vec<(Side, Fore, Vec3)>,
}

impl Lamps {
    pub(super) fn measure(model: &CarModelAsset, frame: &CarFrame) -> Self {
        let mut signals = Vec::new();
        for (side, fore) in [
            (Side::Left, Fore::Front),
            (Side::Right, Fore::Front),
            (Side::Left, Fore::Rear),
            (Side::Right, Fore::Rear),
        ] {
            for center in model
                .lamp_centers(frame, MaterialRole::Signal(side, fore))
                .into_values()
            {
                signals.push((side, fore, center));
            }
        }
        Self {
            heads: model
                .lamp_centers(frame, MaterialRole::LightHead)
                .into_values()
                .collect(),
            tails: model
                .lamp_centers(frame, MaterialRole::LightBrake)
                .into_values()
                .collect(),
            signals,
        }
    }
}

/// Tout ce dont la pose des lumières a besoin.
pub(super) struct LightScene<'a> {
    pub(super) frame: &'a CarFrame,
    pub(super) lamps: &'a Lamps,
    pub(super) signal_mats: &'a [(Side, Fore, Emissive)],
    /// Racine de la voiture : les lueurs au sol y sont posées (la route ne penche pas avec la caisse).
    pub(super) root: Entity,
    /// Carrosserie : les volumes de phares y sont posés.
    pub(super) body: Entity,
    /// Châssis (nœud du GLB) : les halos y sont posés, ils suivent la suspension.
    pub(super) chassis: Entity,
    /// Teinte de départ des lumières néon.
    pub(super) glow: LinearRgba,
}

fn halo_aabb() -> Aabb {
    Aabb::from_min_max(Vec3::splat(-0.75), Vec3::splat(0.75))
}

fn light_material(assets: &mut CarAssets, kind: LightKind, bias: f32) -> Handle<CarLightMaterial> {
    assets.lights.add(CarLightMaterial::new(kind, bias))
}

/// Une lueur ou une flaque posée sur la route, fille de la racine.
fn ground_light(
    commands: &mut Commands,
    scene: &LightScene,
    mesh: Handle<Mesh>,
    material: &Handle<CarLightMaterial>,
    (at, scale, name): (Vec3, Vec3, &str),
) -> Entity {
    commands
        .spawn((
            Name::new(name.to_owned()),
            Mesh3d(mesh),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(at).with_scale(scale),
            Visibility::default(),
            WorldLayer,
            ChildOf(scene.root),
        ))
        .id()
}

/// Un halo posé sur le châssis : `center` est dans le repère de la voiture, ramené dans celui du châssis.
fn halo(
    commands: &mut Commands,
    assets: &mut CarAssets,
    scene: &LightScene,
    quad: &Handle<Mesh>,
    (center, outward, size, name): (Vec3, f32, f32, &str),
) -> HaloRig {
    let to_model: Mat4 = scene.frame.to_car.to_matrix().inverse();
    let material = light_material(assets, LightKind::Halo, 0.0);
    let local = to_model.transform_point3(center + Vec3::new(0.0, 0.0, outward));
    let entity = commands
        .spawn((
            Name::new(name.to_owned()),
            Mesh3d(quad.clone()),
            MeshMaterial3d(material.clone()),
            Transform::from_translation(local).with_scale(Vec3::splat(size)),
            Visibility::default(),
            halo_aabb(),
            WorldLayer,
            ChildOf(scene.chassis),
        ))
        .id();
    HaloRig { material, entity }
}

/// Pose les lumières ajoutées et retourne leur rig, plus celui des quatre clignotants.
pub(super) fn spawn_lights(
    commands: &mut Commands,
    assets: &mut CarAssets,
    model: &CarModelAsset,
    scene: &LightScene,
) -> (LightRig, Vec<SignalRig>) {
    let size = scene.frame.bounds.size();
    let (front, back) = (scene.frame.bounds.min.z, scene.frame.bounds.max.z);

    // Volumes lumineux des phares : un par optique, sur la carrosserie (ils ne suivent pas la suspension du modèle).
    let beam_mesh = assets.meshes.add(shapes::beam());
    let beam = light_material(assets, LightKind::Beam, 0.0);
    let (lamp_x, lamp_y) = (size.x * 0.32, size.y * 0.3);
    for sign in [-1.0_f32, 1.0] {
        commands.spawn((
            Name::new("faisceau"),
            Mesh3d(beam_mesh.clone()),
            MeshMaterial3d(beam.clone()),
            Transform::from_xyz(sign * lamp_x, lamp_y, front - 0.04),
            Visibility::default(),
            WorldLayer,
            ChildOf(scene.body),
        ));
    }

    // Flaques sur la route : devant (phares), derrière (feux rouges), sous la caisse (lueur néon).
    let front_pool = light_material(assets, LightKind::Pool, POOL_DEPTH_BIAS);
    let rear_pool = light_material(assets, LightKind::Pool, POOL_DEPTH_BIAS);
    let front_mesh = assets
        .meshes
        .add(shapes::pool(shapes::POOL_WIDTH_M, shapes::POOL_DEPTH_M, -1.0));
    let rear_mesh = assets
        .meshes
        .add(shapes::pool(shapes::POOL_WIDTH_M, shapes::POOL_DEPTH_M * 0.28, 1.0));
    ground_light(
        commands,
        scene,
        front_mesh,
        &front_pool,
        (Vec3::new(0.0, 0.04, front + 1.2), Vec3::ONE, "flaque des phares"),
    );
    ground_light(
        commands,
        scene,
        rear_mesh,
        &rear_pool,
        (Vec3::new(0.0, 0.04, back - 0.6), Vec3::ONE, "flaque des feux"),
    );
    let ground_quad = assets.meshes.add(shapes::ground_quad());
    let under_material = light_material(assets, LightKind::Glow, POOL_DEPTH_BIAS);
    let under_entity = ground_light(
        commands,
        scene,
        ground_quad.clone(),
        &under_material,
        (
            Vec3::new(0.0, 0.05, 0.0),
            Vec3::new(3.2, 1.0, 5.0),
            "lueur sous la caisse",
        ),
    );

    // Halos : les phares à l'avant (la route va vers −Z), les feux stop à l'arrière.
    let quad = assets.meshes.add(shapes::halo_quad());
    let heads = scene
        .lamps
        .heads
        .iter()
        .map(|&c| halo(commands, assets, scene, &quad, (c, -0.12, 1.1, "halo de phare")))
        .collect();
    let tails = scene
        .lamps
        .tails
        .iter()
        .map(|&c| halo(commands, assets, scene, &quad, (c, 0.12, 0.9, "halo de feu stop")))
        .collect();
    let signals = spawn_signals(commands, assets, scene, &quad, &ground_quad);
    let (spots, under_light, logo_light, cabin_light) = spawn_projectors(commands, model, scene);
    (
        LightRig {
            beam,
            front_pool,
            rear_pool,
            under: HaloRig {
                material: under_material,
                entity: under_entity,
            },
            heads,
            tails,
            spots,
            under_light,
            logo_light,
            cabin_light,
        },
        signals,
    )
}

/// Les quatre clignotants : un halo sur la lentille, une flaque sur la route qui dit de quel côté la voiture va tourner.
fn spawn_signals(
    commands: &mut Commands,
    assets: &mut CarAssets,
    scene: &LightScene,
    quad: &Handle<Mesh>,
    ground_quad: &Handle<Mesh>,
) -> Vec<SignalRig> {
    let mut signals = Vec::new();
    for &(side, fore, center) in &scene.lamps.signals {
        let Some((_, _, material)) = scene.signal_mats.iter().find(|(s, f, _)| *s == side && *f == fore) else {
            continue;
        };
        let is_front = center.z < 0.0;
        let halo_rig = halo(
            commands,
            assets,
            scene,
            quad,
            (
                center,
                if is_front { -0.1 } else { 0.12 },
                if is_front { 0.8 } else { 1.3 },
                "halo de clignotant",
            ),
        );
        // La flaque déborde sur le bas-côté.
        let sign = if side == Side::Left { -1.0 } else { 1.0 };
        let pool = light_material(assets, LightKind::Glow, POOL_DEPTH_BIAS);
        let scale = if is_front { 2.6 } else { 3.2 };
        let at = Vec3::new(
            center.x + sign * 0.9,
            0.05,
            center.z + if is_front { -0.4 } else { 0.4 },
        );
        let pool_entity = ground_light(
            commands,
            scene,
            ground_quad.clone(),
            &pool,
            (at, Vec3::new(scale, 1.0, scale), "flaque de clignotant"),
        );
        signals.push(SignalRig {
            side,
            material: material.clone(),
            halo: halo_rig.material,
            halo_entity: halo_rig.entity,
            pool,
            pool_entity,
        });
    }
    signals
}

/// Projecteurs (lumières de Bevy) : deux phares, une lueur sous la caisse, une lumière derrière le logo. Comme dans
/// Sillage, ils n'éclairent que ce qui est éclairé : la route et les néons sont émissifs.
fn spawn_projectors(
    commands: &mut Commands,
    model: &CarModelAsset,
    scene: &LightScene,
) -> (Vec<Entity>, Option<Entity>, Option<Entity>, Option<Entity>) {
    let layers: RenderLayers = Layer::World.render_layers();
    let mut spots = Vec::new();
    for name in ["anchor_headlight_L", "anchor_headlight_R"] {
        let Some(index) = model.node(name) else { continue };
        let at = model.nodes[index].transform.translation;
        let aim = Vec3::new(at.x, at.y - 0.9, at.z + 14.0);
        spots.push(
            commands
                .spawn((
                    Name::new("projecteur"),
                    SpotLight {
                        color: SPOT_COLOR.into(),
                        intensity: 60.0 * LUMENS_PER_CD,
                        range: 55.0,
                        radius: 0.0,
                        shadow_maps_enabled: false,
                        outer_angle: 0.5,
                        inner_angle: 0.15,
                        ..default()
                    },
                    Transform::from_translation(at).looking_at(aim, Vec3::Y),
                    Visibility::default(),
                    layers.clone(),
                    ChildOf(scene.chassis),
                ))
                .id(),
        );
    }
    let mut point = |anchor: &str, cd: f32, range: f32, name: &str| {
        let index = model.node(anchor)?;
        Some(
            commands
                .spawn((
                    Name::new(name.to_owned()),
                    PointLight {
                        color: Color::LinearRgba(scene.glow),
                        intensity: cd * LUMENS_PER_CD,
                        range,
                        radius: 0.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(model.nodes[index].transform.translation),
                    Visibility::default(),
                    layers.clone(),
                    ChildOf(scene.chassis),
                ))
                .id(),
        )
    };
    let under = point(
        "anchor_underglow",
        1.6 * NEON_INTENSITY / 0.45,
        5.0,
        "lumière sous la caisse",
    );
    let logo = point("anchor_rear_logo_light", 0.7, 2.5, "lumière du logo arrière");
    let cabin = point("anchor_cabin_light", 0.12, 3.0, "lumière de l'habitacle");
    (spots, under, logo, cabin)
}
