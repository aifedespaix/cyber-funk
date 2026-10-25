//! Les entités du paquet du drop, créées une fois au démarrage (pool : rien n'est créé ni détruit en régime établi).
//!
//! [`RINGS`] ondes (un plan chacune, un matériau chacune : leur éclat et leur rayon diffèrent) et une gerbe (un seul maillage
//! de [`PARTICLES`] carrés), tous racines (une [`AbsolutePosition`] chacun), tous cachés au repos.
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Aucune pièce ne porte `NoFrustumCulling`. Les ondes ont l'`Aabb` exact du rectangle unité que Bevy calcule : leur
//! `Transform` (échelle, couchage) fait le reste. La gerbe a un `Aabb` **recalculé à chaque image** d'après ses particules
//! vivantes (voir `sync`), marge d'un demi-côté de carré comprise : ses sommets sont tous au centre de leur carré, l'`Aabb`
//! calculé du maillage au repos serait nul.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, WorldLayer};

use super::logic::{PARTICLES, RINGS};
use super::material::{DropRingMaterial, DropSparkMaterial, RING_HALF_M};

/// Tout ce qui fait partie du paquet du drop : à interroger pour le compter ou le cacher en bloc.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct DropPart;

/// Une pièce du pool.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(DropPart)]
pub enum DropPiece {
    /// L'onde de choc `n`.
    Ring(usize),
    /// La gerbe d'étincelles.
    Sparks,
}

/// Les poignées des matériaux et du maillage dynamique, pour les réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct DropRig {
    pub(super) rings: [Handle<DropRingMaterial>; RINGS],
    pub(super) sparks_mesh: Handle<Mesh>,
}

/// Le maillage de la gerbe : [`PARTICLES`] carrés dont les quatre sommets sont au centre de l'étincelle (le vertex shader les
/// écarte). Positions et couleurs sont réécrites à chaque image tant que la gerbe vit.
#[must_use]
pub fn sparks_mesh() -> Mesh {
    let corners = [[0.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut uv = Vec::with_capacity(PARTICLES * 4);
    let mut indices = Vec::with_capacity(PARTICLES * 6);
    for i in 0..PARTICLES {
        uv.extend(corners);
        let base = (i * 4) as u32;
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; PARTICLES * 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; PARTICLES * 4])
        .with_inserted_indices(Indices::U32(indices))
}

/// Crée le pool d'entités, une fois, au démarrage. Tout est caché ; `sync` montre ce qui vit.
pub(super) fn spawn_dropfx(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut ring_materials: ResMut<Assets<DropRingMaterial>>,
    mut spark_materials: ResMut<Assets<DropSparkMaterial>>,
) {
    let quad = meshes.add(Rectangle::new(1.0, 1.0).mesh().build());
    let rings = std::array::from_fn::<_, RINGS, _>(|i| {
        let handle = ring_materials.add(DropRingMaterial::default());
        commands.spawn((
            Name::new("drop · onde de choc"),
            DropPiece::Ring(i),
            Mesh3d(quad.clone()),
            MeshMaterial3d(handle.clone()),
            // Le quad unité devient un carré de `2 × RING_HALF_M` mètres, couché sur la route par `sync`.
            Transform::from_scale(Vec3::new(RING_HALF_M * 2.0, RING_HALF_M * 2.0, 1.0)),
            Visibility::Hidden,
        ));
        handle
    });
    let sparks_mesh = meshes.add(sparks_mesh());
    commands.spawn((
        Name::new("drop · gerbe"),
        DropPiece::Sparks,
        Mesh3d(sparks_mesh.clone()),
        MeshMaterial3d(spark_materials.add(DropSparkMaterial::default())),
        // Recalculé à chaque image par `sync` ; cette boîte de départ n'est jamais vue (la gerbe est cachée).
        Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)),
        Visibility::Hidden,
    ));
    commands.insert_resource(DropRig { rings, sparks_mesh });
}
