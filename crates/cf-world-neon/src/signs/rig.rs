//! Les entités des enseignes : onze unités créées une fois au démarrage, cachées au repos, jamais détruites ni recréées
//! (aucun `spawn` en régime établi). Chaque unité est une racine à position absolue (origine flottante) qui porte :
//!
//! - un plan (`SignMaterial`) : l'image de l'enseigne ;
//! - treize pièces de structure (caisson, lisérés, pieds, poutre, corniche, balises), des boîtes et des cylindres
//!   unitaires mis à l'échelle ; leurs matériaux sont sans éclairage (`unlit`), comme les `MeshBasicMaterial` de Sillage.
//!
//! Chaque entité porte `WorldLayer` : Bevy ne propage pas les `RenderLayers` aux enfants. Les `Aabb` sont ceux des
//! maillages, mis à l'échelle par leurs transforms : aucun `NoFrustumCulling`.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use cf_render::{AbsolutePosition, WorldLayer};

use super::atlas::{CellInfo, StripInfo};
use super::flash::DisplayGate;
use super::formats::Look;
use super::material::SignMaterial;
use super::road::POOL;
use super::structure::{PART_COUNT, PartKind};

/// Rang d'une unité du pool, porté par sa racine.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignUnitIndex(pub usize);

/// Ce que l'atlas donne à un panneau posé, résolu une fois à la pose.
#[derive(Debug, Clone, Copy, Default)]
pub struct Resolved {
    /// La cellule, `None` si elle manque (le plan reste caché).
    pub cell: Option<CellInfo>,
    /// Le bandeau, `None` s'il n'y en a pas.
    pub strip: Option<StripInfo>,
    /// Le rendu.
    pub look: Option<Look>,
}

/// Les entités et l'état de rendu d'une unité.
#[derive(Debug)]
pub struct UnitRig {
    /// La racine (position absolue).
    pub root: Entity,
    /// Le plan de l'image.
    pub panel: Entity,
    /// Les pièces de structure, indexées par [`PartKind::index`].
    pub parts: [Entity; PART_COUNT],
    /// Le matériau du plan.
    pub material: Handle<SignMaterial>,
    /// Liseré lumineux, teinté à l'accent du panneau.
    pub light: Handle<StandardMaterial>,
    /// Balise rouge des pylônes.
    pub beacon: Handle<StandardMaterial>,
    /// L'écran, vu à travers les bornes de sécurité.
    pub gate: DisplayGate,
    /// Temps restant (s) pendant lequel le flash du radar de ce panneau est permis.
    pub radar_hold_s: f32,
    /// La résolution de la dernière pose.
    pub resolved: Resolved,
}

/// Toutes les unités.
#[derive(Resource, Debug)]
pub struct SignRig {
    /// Les onze unités.
    pub units: Vec<UnitRig>,
    /// Métal sombre : les pieds, la poutre, le dos du caisson.
    pub iron: Handle<StandardMaterial>,
}

fn placeholder(texels: u32, format: TextureFormat) -> Image {
    let size = Extent3d {
        width: texels,
        height: texels,
        depth_or_array_layers: 1,
    };
    let bytes = match format {
        TextureFormat::R8Unorm => vec![0_u8; (texels * texels) as usize],
        _ => vec![0_u8; (texels * texels * 4) as usize],
    };
    let mut image = Image::new(size, TextureDimension::D2, bytes, format, RenderAssetUsages::RENDER_WORLD);
    image.sampler = ImageSampler::linear();
    image
}

/// `Startup` : crée les unités, cachées. Le plan montre une image vide jusqu'à ce que l'atlas soit cuit.
pub fn spawn_rig(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut signs: ResMut<Assets<SignMaterial>>,
    mut standard: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let cylinder = meshes.add(Cylinder::new(0.5, 1.0).mesh().resolution(12));
    let blank_art = images.add(placeholder(1, TextureFormat::Rgba8Unorm));
    let blank_strip = images.add(placeholder(1, TextureFormat::R8Unorm));
    let unlit = |color: Color| StandardMaterial {
        base_color: color,
        unlit: true,
        fog_enabled: false,
        ..default()
    };
    let iron = standard.add(unlit(Color::linear_rgb(0.05, 0.03, 0.12)));
    let mut units = Vec::with_capacity(POOL);
    for index in 0..POOL {
        let material = signs.add(SignMaterial {
            art: Some(blank_art.clone()),
            strip: Some(blank_strip.clone()),
            ..default()
        });
        let light = standard.add(unlit(Color::BLACK));
        let beacon = standard.add(unlit(Color::BLACK));
        let root = commands
            .spawn((
                Name::new(format!("enseigne {index}")),
                SignUnitIndex(index),
                AbsolutePosition(DVec3::ZERO),
                Transform::default(),
                Visibility::Hidden,
                WorldLayer,
            ))
            .id();
        let panel = commands
            .spawn((
                Mesh3d(quad.clone()),
                MeshMaterial3d(material.clone()),
                Transform::default(),
                Visibility::Hidden,
                WorldLayer,
                ChildOf(root),
            ))
            .id();
        let parts = PartKind::ALL.map(|kind| {
            let (mesh, handle) = match kind {
                PartKind::Round(_) => (cylinder.clone(), iron.clone()),
                PartKind::Strip(_) => (cube.clone(), light.clone()),
                PartKind::Beacon(_) => (cube.clone(), beacon.clone()),
                _ => (cube.clone(), iron.clone()),
            };
            commands
                .spawn((
                    Mesh3d(mesh),
                    MeshMaterial3d(handle),
                    Transform::default(),
                    Visibility::Hidden,
                    WorldLayer,
                    ChildOf(root),
                ))
                .id()
        });
        units.push(UnitRig {
            root,
            panel,
            parts,
            material,
            light,
            beacon,
            gate: DisplayGate::default(),
            radar_hold_s: 0.0,
            resolved: Resolved::default(),
        });
    }
    commands.insert_resource(SignRig { units, iron });
}
