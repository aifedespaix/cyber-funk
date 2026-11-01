//! Pose un modèle GLB décodé (`CarModelAsset`) sous une entité : maillages, matériaux, arbre de nœuds. Sert au conducteur
//! de la voiture et aux motos du trafic. Les carrosseries peuvent être teintées ([`Tint`]) : le pilote par son masque de
//! peinture (`body_paint`), la moto par ses matériaux de carrosserie nommés.
//!
//! Le modèle est consommé : ses maillages et images passent dans les `Assets`, sans copie. Chaque entité porte
//! `WorldLayer` : Bevy ne propage pas ce marqueur aux enfants, une entité sans lui est invisible pour la caméra du monde.

use bevy::asset::{Assets, Handle};
use bevy::color::LinearRgba;
use bevy::ecs::system::SystemParam;
use bevy::image::Image;
use bevy::math::Vec3;
use bevy::pbr::{ExtendedMaterial, MeshMaterial3d, StandardMaterial};
use bevy::prelude::{ChildOf, Commands, Entity, Mesh, Mesh3d, Name, Transform, Visibility};
use cf_render::WorldLayer;

use super::tint::{MANUL_REF_LUMINANCE, MOTO_REF_LUMINANCE, RiderPaintMaterial, RiderTintExt};
use crate::car::model::{CarModelAsset, MaterialRole};
use crate::car::spawn::standard_material;

/// Les collections d'assets où le modèle dépose ses maillages, images et matériaux.
#[derive(SystemParam)]
#[allow(missing_debug_implementations)] // Les collections d'assets de Bevy n'implémentent pas `Debug`.
pub struct RiderAssets<'w> {
    /// Maillages.
    pub meshes: bevy::ecs::system::ResMut<'w, Assets<Mesh>>,
    /// Images (textures décodées).
    pub images: bevy::ecs::system::ResMut<'w, Assets<Image>>,
    /// Matériaux standards.
    pub standard: bevy::ecs::system::ResMut<'w, Assets<StandardMaterial>>,
    /// Matériaux teintés.
    pub tinted: bevy::ecs::system::ResMut<'w, Assets<RiderPaintMaterial>>,
}

impl RiderAssets<'_> {
    /// Les collections sous forme de simples références, pour les systèmes qui ont déjà leurs propres accès aux assets.
    pub fn refs(&mut self) -> ModelAssets<'_> {
        ModelAssets {
            meshes: &mut self.meshes,
            images: &mut self.images,
            standard: &mut self.standard,
            tinted: &mut self.tinted,
        }
    }
}

/// Les collections d'assets, empruntées : ce que consomme [`spawn_model`].
#[allow(missing_debug_implementations)] // Les collections d'assets de Bevy n'implémentent pas `Debug`.
pub struct ModelAssets<'a> {
    /// Maillages.
    pub meshes: &'a mut Assets<Mesh>,
    /// Images (textures décodées).
    pub images: &'a mut Assets<Image>,
    /// Matériaux standards.
    pub standard: &'a mut Assets<StandardMaterial>,
    /// Matériaux teintés.
    pub tinted: &'a mut Assets<RiderPaintMaterial>,
}

/// La teinte à donner aux carrosseries d'un modèle.
#[derive(Debug, Clone, Copy)]
pub struct Tint<'a> {
    /// Couleur visée (lumière linéaire).
    pub color: LinearRgba,
    /// Noms de matériaux de carrosserie teintés sur toute leur surface (sans suffixe `.001`) : les motos.
    pub body_materials: &'a [&'a str],
}

/// Les nœuds qu'une moto anime, retrouvés par leur nom dans le GLB (`tools/char/bake_rider.py`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MotoNodes {
    /// Roue avant (`spin_F`) : origine sur l'essieu, axe X.
    pub front: Option<Entity>,
    /// Roue arrière (`spin_R`).
    pub rear: Option<Entity>,
    /// Pilote (`rider`) : origine au bassin ; sa translation de repos y est gardée.
    pub rider: Option<(Entity, Vec3)>,
}

/// Ce que [`spawn_model`] a posé.
#[derive(Debug, Clone, Default)]
pub struct SpawnedModel {
    /// Les matériaux teintés : leur couleur peut être changée à la naissance d'une moto.
    pub tinted: Vec<Handle<RiderPaintMaterial>>,
    /// Roues et pilote, s'ils existent dans le modèle.
    pub nodes: MotoNodes,
}

enum Built {
    Standard(Handle<StandardMaterial>),
    Tinted(Handle<RiderPaintMaterial>),
}

/// Le nom d'un matériau sans le suffixe de doublon de Blender (`cor_base02.005` donne `cor_base02`).
fn base_name(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((base, suffix)) if !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) => base,
        _ => name,
    }
}

/// Pose `model` sous `parent` ; renvoie l'entité racine du modèle et ses matériaux teintés.
pub fn spawn_model(
    commands: &mut Commands,
    parent: Entity,
    mut model: CarModelAsset,
    assets: &mut ModelAssets<'_>,
    tint: Option<Tint<'_>>,
) -> (Entity, SpawnedModel) {
    let images: Vec<Handle<Image>> = model.images.drain(..).map(|i| assets.images.add(i)).collect();
    let meshes: Vec<Handle<Mesh>> = model.meshes.drain(..).map(|m| assets.meshes.add(m)).collect();
    let mut spawned = SpawnedModel::default();
    let built: Vec<Built> = model
        .materials
        .iter()
        .map(|def| {
            let standard = standard_material(def, &images);
            let tinted = |reference: f32, mask: Option<Handle<Image>>| {
                tint.map(|t| ExtendedMaterial {
                    base: standard.clone(),
                    extension: RiderTintExt::new(t.color, reference, mask),
                })
            };
            let body = tint.is_some_and(|t| t.body_materials.contains(&base_name(&def.name)));
            let extended = if def.role == MaterialRole::BodyPaint {
                // Le pilote : teinté dans son masque de peinture.
                def.mask_image
                    .and_then(|i| images.get(i).cloned())
                    .and_then(|mask| tinted(MANUL_REF_LUMINANCE, Some(mask)))
            } else if body {
                tinted(MOTO_REF_LUMINANCE, None)
            } else {
                None
            };
            match extended {
                Some(material) => {
                    let handle = assets.tinted.add(material);
                    spawned.tinted.push(handle.clone());
                    Built::Tinted(handle)
                }
                None => Built::Standard(assets.standard.add(standard)),
            }
        })
        .collect();

    let root = commands
        .spawn((
            Name::new("modèle"),
            Transform::IDENTITY,
            Visibility::default(),
            WorldLayer,
            ChildOf(parent),
        ))
        .id();
    let mut stack: Vec<(usize, Entity)> = model.roots.iter().map(|&r| (r, root)).collect();
    while let Some((index, parent)) = stack.pop() {
        let node = &model.nodes[index];
        let entity = commands
            .spawn((
                Name::new(node.name.clone()),
                node.transform,
                Visibility::default(),
                WorldLayer,
                ChildOf(parent),
            ))
            .id();
        match node.name.as_str() {
            "spin_F" => spawned.nodes.front = Some(entity),
            "spin_R" => spawned.nodes.rear = Some(entity),
            "rider" => spawned.nodes.rider = Some((entity, node.transform.translation)),
            _ => {}
        }
        for &(mesh, material) in &node.pieces {
            let mut piece = commands.spawn((
                Name::new(format!("{} · {}", node.name, model.materials[material].name)),
                Mesh3d(meshes[mesh].clone()),
                Transform::IDENTITY,
                Visibility::default(),
                WorldLayer,
                ChildOf(entity),
            ));
            match &built[material] {
                Built::Standard(handle) => {
                    piece.insert(MeshMaterial3d(handle.clone()));
                }
                Built::Tinted(handle) => {
                    piece.insert(MeshMaterial3d(handle.clone()));
                }
            }
        }
        stack.extend(node.children.iter().map(|&c| (c, entity)));
    }
    (root, spawned)
}

#[cfg(test)]
mod tests {
    use super::base_name;

    #[test]
    fn le_suffixe_de_doublon_de_blender_est_retire() {
        assert_eq!(base_name("cor_base02.005"), "cor_base02");
        assert_eq!(base_name("Carcass1"), "Carcass1");
        assert_eq!(base_name("borracha"), "borracha");
        assert_eq!(base_name("a.b"), "a.b");
    }
}
