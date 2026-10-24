//! Pose la voiture dans le monde : l'arbre de nœuds du GLB, ses matériaux, les halos, flaques et volumes que le code
//! ajoute, les projecteurs.
//!
//! Toutes les entités portent [`WorldLayer`] (Bevy ne propage pas les `RenderLayers` aux enfants) et les lumières
//! `Layer::World.render_layers()` : sans cela la caméra du monde ne les voit pas (règle de `lib.rs`). Aucune n'a
//! `NoFrustumCulling` : les maillages du modèle ont l'`Aabb` que Bevy en calcule, les halos (carrés tournés vers la
//! caméra) en ont un volontairement plus large que leur plan.

use bevy::asset::{Assets, Handle};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use LinearRgba;
use bevy::image::Image;
use bevy::light::{PointLight, SpotLight};
use bevy::material::AlphaMode;
use bevy::math::{Mat4, Vec3};
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use cf_render::{Layer, WorldLayer};

use super::frame::CarFrame;
use super::glow::{CarPalette, NEON_INTENSITY};
use super::materials::{CarLightMaterial, CarPaintExt, CarPaintMaterial, LightKind, PaintParams};
use super::paint::PAINT_REF_HUE;
use super::model::{Alpha, CarModelAsset, Fore, MaterialData, MaterialRole, Side};
use super::rig::{CarRig, Emissive, HaloRig, LightRig, ModelMaterials, SignalRig};
use super::shapes;

/// Les collections d'assets où la voiture dépose ses maillages, images et matériaux.
#[derive(bevy::ecs::system::SystemParam)]
#[allow(missing_debug_implementations)] // Les collections d'assets de Bevy n'implémentent pas `Debug`.
pub struct CarAssets<'w> {
    /// Maillages.
    pub meshes: ResMut<'w, Assets<Mesh>>,
    /// Images (textures décodées).
    pub images: ResMut<'w, Assets<Image>>,
    /// Matériaux standards (le modèle).
    pub standard: ResMut<'w, Assets<StandardMaterial>>,
    /// Peinture.
    pub paint: ResMut<'w, Assets<CarPaintMaterial>>,
    /// Lumières additives.
    pub lights: ResMut<'w, Assets<CarLightMaterial>>,
}

/// Teinte du bleu-blanc des projecteurs (`0xcfe9ff` de Sillage), en lumière linéaire.
const SPOT_COLOR: LinearRgba = LinearRgba::rgb(0.624, 0.807, 1.0);
/// Candela → lumens (Bevy) : `lumens = candela × 4π`.
const LUMENS_PER_CD: f32 = 4.0 * std::f32::consts::PI;

fn standard_material(def: &MaterialData, images: &[Handle<Image>]) -> StandardMaterial {
    let image = |index: Option<usize>| index.and_then(|i| images.get(i)).cloned();
    let emissive = def.emissive;
    StandardMaterial {
        base_color: Color::LinearRgba(def.base_color),
        perceptual_roughness: def.roughness,
        metallic: def.metallic,
        emissive: LinearRgba::rgb(
            emissive.red * def.emissive_strength,
            emissive.green * def.emissive_strength,
            emissive.blue * def.emissive_strength,
        ),
        base_color_texture: image(def.base_color_image),
        metallic_roughness_texture: image(def.metallic_roughness_image),
        normal_map_texture: image(def.normal_image),
        double_sided: def.double_sided,
        cull_mode: if def.double_sided { None } else { Some(bevy::render::render_resource::Face::Back) },
        alpha_mode: match def.alpha {
            Alpha::Opaque => AlphaMode::Opaque,
            Alpha::Mask(cutoff) => AlphaMode::Mask(cutoff),
            Alpha::Blend => AlphaMode::Blend,
        },
        ..default()
    }
}

/// Vitre : le modèle la livre à 28 % d'opacité, Sillage la règle à 30 % et à 0,1 de rugosité (« un verre trop lisse
/// transforme chaque petite lumière en disque dur »).
fn glass(def: &MaterialData) -> StandardMaterial {
    let mut material = standard_material(def, &[]);
    let c = def.base_color;
    material.base_color = Color::LinearRgba(LinearRgba::new(c.red, c.green, c.blue, 0.3));
    material.perceptual_roughness = 0.1;
    material
}

/// Peinture : la finition « metallic » de Sillage (métal 0,85, rugosité 0,32, vernis 1 à 0,06 de rugosité, paillettes
/// 0,6, liseré 0,35), la teinte d'origine au départ.
fn paint_material(def: &MaterialData, images: &[Handle<Image>], palette: &CarPalette) -> CarPaintMaterial {
    let mut base = standard_material(def, images);
    base.clearcoat = 1.0;
    base.clearcoat_perceptual_roughness = 0.06;
    let rim = |c: LinearRgba| Vec4::new(c.red, c.green, c.blue, 1.0);
    CarPaintMaterial {
        base,
        extension: CarPaintExt {
            params: PaintParams {
                paint: Vec4::new(PAINT_REF_HUE, 1.0, 1.0, f32::from(def.mask_image.is_some())),
                finish: Vec4::new(0.32, 0.85, 0.6, 0.35),
                rim_a: rim(palette.violet),
                rim_b: rim(palette.rose),
            },
            mask: def.mask_image.and_then(|i| images.get(i)).cloned(),
        },
    }
}

/// Position des optiques dans le repère de la voiture, mesurée sur les sommets du modèle **avant** que ses maillages
/// ne passent dans les `Assets`.
struct Lamps {
    heads: Vec<Vec3>,
    tails: Vec<Vec3>,
    signals: Vec<(Side, Fore, Vec3)>,
}

impl Lamps {
    fn measure(model: &CarModelAsset, frame: &CarFrame) -> Self {
        let mut signals = Vec::new();
        for (side, fore) in [
            (Side::Left, Fore::Front),
            (Side::Right, Fore::Front),
            (Side::Left, Fore::Rear),
            (Side::Right, Fore::Rear),
        ] {
            for center in model.lamp_centers(frame, MaterialRole::Signal(side, fore)).into_values() {
                signals.push((side, fore, center));
            }
        }
        Self {
            heads: model.lamp_centers(frame, MaterialRole::LightHead).into_values().collect(),
            tails: model.lamp_centers(frame, MaterialRole::LightBrake).into_values().collect(),
            signals,
        }
    }
}

/// Un matériau construit : standard, peinture ou « à ne pas dessiner ».
enum Built {
    Standard(Handle<StandardMaterial>),
    Paint(Handle<CarPaintMaterial>),
    Skip,
}

/// Pose la voiture sous `root` : retourne le [`CarRig`] à poser sur l'entité racine.
///
/// `model` est consommé : ses maillages et images passent dans les `Assets`, sans copie.
pub fn spawn_car(
    commands: &mut Commands,
    root: Entity,
    mut model: CarModelAsset,
    assets: &mut CarAssets,
    palette: &CarPalette,
    glow: LinearRgba,
) -> CarRig {
    let frame = model.frame();
    let lamps = Lamps::measure(&model, &frame);
    let images: Vec<Handle<Image>> = model.images.drain(..).map(|i| assets.images.add(i)).collect();
    let meshes: Vec<Handle<Mesh>> = model.meshes.drain(..).map(|m| assets.meshes.add(m)).collect();
    let mut handles = ModelMaterials::default();
    let mut signal_mats: Vec<(Side, Fore, Emissive)> = Vec::new();
    let built: Vec<Built> = model
        .materials
        .iter()
        .map(|def| match def.role {
            MaterialRole::RainGlass => Built::Skip,
            MaterialRole::BodyPaint => {
                let handle = assets.paint.add(paint_material(def, &images, palette));
                handles.paint = Some(handle.clone());
                Built::Paint(handle)
            }
            role => {
                let mut material = match role {
                    MaterialRole::Glass => glass(def),
                    _ => standard_material(def, &images),
                };
                if role == MaterialRole::Screen {
                    // Écrans éteints : Sillage y dessine compteur et autoradio (jeu), pas repris en mode cinéma.
                    material.emissive = LinearRgba::BLACK;
                }
                let handle = assets.standard.add(material);
                let emissive = Emissive {
                    handle: handle.clone(),
                    color: def.emissive,
                    strength: def.emissive_strength,
                };
                match role {
                    MaterialRole::NeonPrimary => handles.neon_primary = Some(emissive),
                    MaterialRole::NeonSecondary => handles.neon_secondary = Some(emissive),
                    MaterialRole::LightHead => handles.head = Some(emissive),
                    MaterialRole::LightBrake => handles.brake = Some(emissive),
                    MaterialRole::LogoRear => handles.logo = Some(emissive),
                    MaterialRole::Glass => handles.glass = Some(handle.clone()),
                    MaterialRole::Signal(side, fore) => signal_mats.push((side, fore, emissive)),
                    _ => {}
                }
                Built::Standard(handle)
            }
        })
        .collect();

    let layer = WorldLayer;
    let group = |name: &str, transform: Transform, parent: Entity, commands: &mut Commands| {
        commands
            .spawn((Name::new(name.to_owned()), transform, Visibility::default(), layer, ChildOf(parent)))
            .id()
    };
    let body = group("carrosserie", Transform::IDENTITY, root, commands);
    let holder = group("repère du modèle", frame.to_car, body, commands);
    let chassis = group("châssis", Transform::IDENTITY, holder, commands);

    // Les roues restent au sol : elles ne suivent pas la suspension de la caisse.
    let wheel_roots = ["steer_FL", "steer_FR", "spin_RL", "spin_RR"];
    let mut entities: Vec<Option<Entity>> = vec![None; model.nodes.len()];
    let mut stack: Vec<(usize, Entity)> = model
        .roots
        .iter()
        .map(|&r| (r, if wheel_roots.contains(&model.nodes[r].name.as_str()) { holder } else { chassis }))
        .collect();
    while let Some((index, parent)) = stack.pop() {
        let node = &model.nodes[index];
        let pieces: Vec<_> = node
            .pieces
            .iter()
            .filter(|(_, material)| !matches!(built[*material], Built::Skip))
            .collect();
        let entity = group(&node.name, node.transform, parent, commands);
        entities[index] = Some(entity);
        for &&(mesh, material) in &pieces {
            let mut piece = commands.spawn((
                Name::new(format!("{} · {}", node.name, model.materials[material].name)),
                Mesh3d(meshes[mesh].clone()),
                Transform::IDENTITY,
                Visibility::default(),
                layer,
                ChildOf(entity),
            ));
            match &built[material] {
                Built::Standard(handle) => {
                    piece.insert(MeshMaterial3d(handle.clone()));
                }
                Built::Paint(handle) => {
                    piece.insert(MeshMaterial3d(handle.clone()));
                }
                Built::Skip => {}
            }
        }
        stack.extend(node.children.iter().map(|&c| (c, entity)));
    }
    let named = |name: &str| model.node(name).and_then(|i| entities[i]);
    let spin: Vec<Entity> = ["spin_FL", "spin_FR", "spin_RL", "spin_RR"].into_iter().filter_map(named).collect();
    let steer: Vec<Entity> = ["steer_FL", "steer_FR"].into_iter().filter_map(named).collect();
    let logo = model.node("logo_rear").and_then(|i| entities[i].map(|e| (e, model.nodes[i].transform.scale)));

    let (lights, signals) = spawn_lights(commands, assets, &model, &frame, &lamps, [root, body, chassis], &signal_mats, glow, layer);
    CarRig {
        frame,
        chassis,
        spin,
        steer,
        logo,
        materials: handles,
        signals,
        lights,
    }
}

/// Le `Aabb` d'un halo : un carré plan tourné vers la caméra occupe un volume, pas un plan : on le gonfle.
fn halo_aabb() -> Aabb {
    Aabb::from_min_max(Vec3::splat(-0.75), Vec3::splat(0.75))
}

/// Halos, flaques, volumes de phares et projecteurs (`CarLights` avec `fixtures = false` et `CyberGlow`).
fn spawn_lights(
    commands: &mut Commands,
    assets: &mut CarAssets,
    model: &CarModelAsset,
    frame: &CarFrame,
    lamps: &Lamps,
    [root, body, chassis]: [Entity; 3],
    signal_mats: &[(Side, Fore, Emissive)],
    glow: LinearRgba,
    layer: WorldLayer,
) -> (LightRig, Vec<SignalRig>) {
    let size = frame.bounds.size();
    let front = frame.bounds.min.z;
    let back = frame.bounds.max.z;
    let to_model: Mat4 = frame.to_car.to_matrix().inverse();
    let light_material = |kind: LightKind, bias: f32, assets: &mut CarAssets| assets.lights.add(CarLightMaterial::new(kind, bias));
    let ground_pool = |commands: &mut Commands, mesh: Handle<Mesh>, material: &Handle<CarLightMaterial>, at: Vec3, scale: Vec3, name: &str| {
        commands
            .spawn((
                Name::new(name.to_owned()),
                Mesh3d(mesh),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(at).with_scale(scale),
                Visibility::default(),
                layer,
                ChildOf(root),
            ))
            .id()
    };

    // Volumes lumineux des phares : un par optique, sur la carrosserie (ils ne suivent pas la suspension du modèle).
    let beam_mesh = assets.meshes.add(shapes::beam());
    let beam = light_material(LightKind::Beam, 0.0, assets);
    let (lamp_x, lamp_y) = (size.x * 0.32, size.y * 0.3);
    for sign in [-1.0_f32, 1.0] {
        commands.spawn((
            Name::new("faisceau"),
            Mesh3d(beam_mesh.clone()),
            MeshMaterial3d(beam.clone()),
            Transform::from_xyz(sign * lamp_x, lamp_y, front - 0.04),
            Visibility::default(),
            layer,
            ChildOf(body),
        ));
    }

    // Flaques sur la route : devant (phares), derrière (feux rouges), sous la caisse (lueur néon).
    let front_pool = light_material(LightKind::Pool, -2.0, assets);
    let rear_pool = light_material(LightKind::Pool, -2.0, assets);
    let front_mesh = assets.meshes.add(shapes::pool(shapes::POOL_WIDTH_M, shapes::POOL_DEPTH_M, -1.0));
    let rear_mesh = assets.meshes.add(shapes::pool(shapes::POOL_WIDTH_M, shapes::POOL_DEPTH_M * 0.28, 1.0));
    ground_pool(commands, front_mesh, &front_pool, Vec3::new(0.0, 0.04, front + 1.2), Vec3::ONE, "flaque des phares");
    ground_pool(commands, rear_mesh, &rear_pool, Vec3::new(0.0, 0.04, back - 0.6), Vec3::ONE, "flaque des feux");
    let ground_quad = assets.meshes.add(shapes::ground_quad());
    let under_material = light_material(LightKind::Glow, -2.0, assets);
    let under_entity = ground_pool(commands, ground_quad.clone(), &under_material, Vec3::new(0.0, 0.05, 0.0), Vec3::new(3.2, 1.0, 5.0), "lueur sous la caisse");

    // Halos : des carrés tournés vers la caméra, posés sur la caisse (ils suivent la suspension). Position calculée
    // dans le repère de la voiture, puis ramenée dans celui du châssis (nœud du GLB).
    let halo_mesh = assets.meshes.add(shapes::halo_quad());
    let halo = |commands: &mut Commands, assets: &mut CarAssets, center: Vec3, outward: f32, size: f32, name: &str| {
        let material = assets.lights.add(CarLightMaterial::new(LightKind::Halo, 0.0));
        let local = to_model.transform_point3(center + Vec3::new(0.0, 0.0, outward));
        let entity = commands
            .spawn((
                Name::new(name.to_owned()),
                Mesh3d(halo_mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(local).with_scale(Vec3::splat(size)),
                Visibility::default(),
                halo_aabb(),
                layer,
                ChildOf(chassis),
            ))
            .id();
        HaloRig { material, entity }
    };
    // Les phares à l'avant (la route va vers −Z), les feux stop à l'arrière.
    let heads = lamps
        .heads
        .iter()
        .map(|&c| halo(commands, assets, c, -0.12, 1.1, "halo de phare"))
        .collect();
    let tails = lamps
        .tails
        .iter()
        .map(|&c| halo(commands, assets, c, 0.12, 0.9, "halo de feu stop"))
        .collect();
    let mut signals = Vec::new();
    for &(side, fore, center) in &lamps.signals {
        let Some((_, _, material)) = signal_mats.iter().find(|(s, f, _)| *s == side && *f == fore) else {
            continue;
        };
        let is_front = center.z < 0.0;
        let halo_rig = halo(
            commands,
            assets,
            center,
            if is_front { -0.1 } else { 0.12 },
            if is_front { 0.8 } else { 1.3 },
            "halo de clignotant",
        );
        // La flaque déborde sur le bas-côté : elle dit de quel côté la voiture va tourner.
        let sign = if side == Side::Left { -1.0 } else { 1.0 };
        let pool = light_material(LightKind::Glow, -2.0, assets);
        let scale = if is_front { 2.6 } else { 3.2 };
        let at = Vec3::new(center.x + sign * 0.9, 0.05, center.z + if is_front { -0.4 } else { 0.4 });
        let pool_entity = ground_pool(commands, ground_quad.clone(), &pool, at, Vec3::new(scale, 1.0, scale), "flaque de clignotant");
        signals.push(SignalRig {
            side,
            material: material.clone(),
            halo: halo_rig.material,
            halo_entity: halo_rig.entity,
            pool,
            pool_entity,
        });
    }

    // Projecteurs (lumières de Bevy) : deux phares, une lueur sous la caisse, une lumière derrière le logo. Comme dans
    // Sillage, ils n'éclairent que ce qui est éclairé : la route et les néons sont émissifs.
    let world_layers: RenderLayers = Layer::World.render_layers();
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
                    world_layers.clone(),
                    ChildOf(chassis),
                ))
                .id(),
        );
    }
    let point = |commands: &mut Commands, anchor: &str, cd: f32, range: f32, name: &str| {
        let index = model.node(anchor)?;
        Some(
            commands
                .spawn((
                    Name::new(name.to_owned()),
                    PointLight {
                        color: Color::LinearRgba(glow),
                        intensity: cd * LUMENS_PER_CD,
                        range,
                        radius: 0.0,
                        shadow_maps_enabled: false,
                        ..default()
                    },
                    Transform::from_translation(model.nodes[index].transform.translation),
                    Visibility::default(),
                    world_layers.clone(),
                    ChildOf(chassis),
                ))
                .id(),
        )
    };
    let under_light = point(commands, "anchor_underglow", 1.6 * NEON_INTENSITY / 0.45, 5.0, "lumière sous la caisse");
    let logo_light = point(commands, "anchor_rear_logo_light", 0.7, 2.5, "lumière du logo arrière");

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
        },
        signals,
    )
}
