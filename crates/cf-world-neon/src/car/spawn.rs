//! Pose la voiture dans le monde : l'arbre de nœuds du GLB, ses matériaux, les halos, flaques et volumes que le code
//! ajoute, les projecteurs.
//!
//! Toutes les entités portent [`WorldLayer`] (Bevy ne propage pas les `RenderLayers` aux enfants) et les lumières
//! `Layer::World.render_layers()` : sans cela la caméra du monde ne les voit pas (règle de `lib.rs`). Aucune n'a
//! `NoFrustumCulling` : les maillages du modèle ont l'`Aabb` que Bevy en calcule, les halos (carrés tournés vers la
//! caméra) en ont un volontairement plus large que leur plan.

use bevy::asset::{Assets, Handle};
use bevy::color::LinearRgba;
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;
use cf_render::WorldLayer;

use super::glow::CarPalette;
use super::materials::{CarLightMaterial, CarPaintExt, CarPaintMaterial, PaintParams};
use super::model::{Alpha, CarModelAsset, Fore, MaterialData, MaterialRole, Side};
use super::paint::PAINT_REF_HUE;
use super::rig::{CarRig, Emissive, ModelMaterials};
use super::screens::{SCREEN_EMISSIVE, ScreenKind, ScreenRig, blank_image};
use super::spawn_lights::{Lamps, LightScene, spawn_lights};

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
        cull_mode: if def.double_sided {
            None
        } else {
            Some(bevy::render::render_resource::Face::Back)
        },
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
    // Reflets nets des néons dans le pare-brise : réflectance maximale (0,5 par défaut) et verre lisse.
    material.reflectance = 1.0;
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

/// Un matériau construit : standard, peinture ou « à ne pas dessiner ».
enum Built {
    Standard(Handle<StandardMaterial>),
    Paint(Handle<CarPaintMaterial>),
    Skip,
}

/// Ce que construit [`build_materials`] : matériaux, poignées pilotées, clignotants, écrans.
type BuiltMaterials = (Vec<Built>, ModelMaterials, Vec<(Side, Fore, Emissive)>, Vec<ScreenRig>);

/// Les matériaux du modèle : un par définition du GLB, plus les poignées que les systèmes repeignent ou allument.
fn build_materials(
    model: &CarModelAsset,
    assets: &mut CarAssets,
    images: &[Handle<Image>],
    palette: &CarPalette,
) -> BuiltMaterials {
    let mut handles = ModelMaterials::default();
    let mut signal_mats: Vec<(Side, Fore, Emissive)> = Vec::new();
    let mut screens: Vec<ScreenRig> = Vec::new();
    let built: Vec<Built> = model
        .materials
        .iter()
        .map(|def| match def.role {
            MaterialRole::RainGlass => Built::Skip,
            MaterialRole::BodyPaint => {
                let handle = assets.paint.add(paint_material(def, images, palette));
                handles.paint = Some(handle.clone());
                Built::Paint(handle)
            }
            role => {
                let mut material = match role {
                    MaterialRole::Glass => glass(def),
                    _ => standard_material(def, images),
                };
                if role == MaterialRole::Screen {
                    // Compteur et autoradio : une image propre, redessinée par `screens::update_screens`. Un autre écran
                    // (le rétroviseur) reste éteint : Sillage y dessine la vue arrière (jeu), pas reprise en mode cinéma.
                    material.emissive = LinearRgba::BLACK;
                    if let Some(kind) = ScreenKind::from_material(&def.name) {
                        let image = assets.images.add(blank_image(kind));
                        material.emissive = SCREEN_EMISSIVE;
                        material.emissive_texture = Some(image.clone());
                        screens.push(ScreenRig { kind, image });
                    }
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
                    MaterialRole::NeonGrad(index) => handles.neon_grad.push((index, emissive)),
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
    (built, handles, signal_mats, screens)
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
    let (built, handles, signal_mats, screens) = build_materials(&model, assets, &images, palette);

    let layer = WorldLayer;
    let group = |name: &str, transform: Transform, parent: Entity, commands: &mut Commands| {
        commands
            .spawn((
                Name::new(name.to_owned()),
                transform,
                Visibility::default(),
                layer,
                ChildOf(parent),
            ))
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
        .map(|&r| {
            (
                r,
                if wheel_roots.contains(&model.nodes[r].name.as_str()) {
                    holder
                } else {
                    chassis
                },
            )
        })
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
    let spin: Vec<Entity> = ["spin_FL", "spin_FR", "spin_RL", "spin_RR"]
        .into_iter()
        .filter_map(named)
        .collect();
    let steer: Vec<Entity> = ["steer_FL", "steer_FR"].into_iter().filter_map(named).collect();
    let logo = model
        .node("logo_rear")
        .and_then(|i| entities[i].map(|e| (e, model.nodes[i].transform.scale)));

    let scene = LightScene {
        frame: &frame,
        lamps: &lamps,
        signal_mats: &signal_mats,
        root,
        body,
        chassis,
        glow,
    };
    let (lights, signals) = spawn_lights(commands, assets, &model, &scene);
    CarRig {
        frame,
        chassis,
        spin,
        steer,
        logo,
        screens,
        materials: handles,
        signals,
        lights,
    }
}
