//! Les entités des projecteurs, créées une fois au démarrage (pool : rien n'est créé ni détruit en régime établi).
//!
//! [`MAX_BEAMS`] cônes et autant de taches au sol, tous racines (une [`AbsolutePosition`] chacun), tous cachés au repos.
//! Tous les cônes partagent un maillage et un matériau, toutes les taches aussi : deux lots de dessin au plus. La teinte de
//! chaque projecteur voyage dans son `MeshTag`, le niveau d'éclat dans le matériau (un seul réécrit par image).
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Aucune pièce ne porte `NoFrustumCulling`. Le cône a l'`Aabb` exact de son maillage (rayon de la base, hauteur 1) que
//! Bevy calcule ; la longueur, la direction et la position sont dans le `Transform`, le culling suit donc le faisceau.
//! La tache est un rectangle plat unitaire, mis à l'échelle et couché par son `Transform`.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, MeshTag, PrimitiveTopology};
use bevy::prelude::*;
use cf_render::{AbsolutePosition, WorldLayer};

use super::field::{BEAM_RADIUS, MAX_BEAMS};
use super::material::BeamMaterial;

/// Côtés du cône.
const SEGMENTS: usize = 20;
/// Rayon de la pointe du cône (m).
const TIP_RADIUS: f32 = 0.1;

/// Tout ce qui fait partie des projecteurs : à interroger pour le compter ou le cacher en bloc.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct BeamPart;

/// Une pièce du pool et son rang.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(BeamPart)]
pub enum BeamPiece {
    /// Le faisceau du projecteur `n`.
    Cone(usize),
    /// La tache du projecteur `n` sur la route.
    Pool(usize),
}

/// Les matériaux partagés, pour les réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct BeamRig {
    pub(super) cone: Handle<BeamMaterial>,
    pub(super) pool: Handle<BeamMaterial>,
}

/// Le cône : pointe fine (`TIP_RADIUS`) à l'origine, évasement (`BEAM_RADIUS`) à `y = 1`, ouvert aux deux bouts.
///
/// Les normales sont **radiales** (sans composante en y), contrairement à celles du `CylinderGeometry` de Three.js : le
/// faisceau est ensuite étiré sur toute sa longueur, et une normale inclinée deviendrait presque parallèle à l'axe, ce qui
/// ferait dépendre l'éclat de l'angle de vue le long du faisceau plutôt que de son épaisseur (voir `beam.wesl`).
#[must_use]
pub fn cone_mesh() -> Mesh {
    let mut positions = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut normals = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut uvs = Vec::with_capacity((SEGMENTS + 1) * 2);
    let mut indices = Vec::with_capacity(SEGMENTS * 6);
    for i in 0..=SEGMENTS {
        let u = i as f32 / SEGMENTS as f32;
        let (sin, cos) = (u * std::f32::consts::TAU).sin_cos();
        for (y, radius) in [(0.0, TIP_RADIUS), (1.0, BEAM_RADIUS)] {
            positions.push([cos * radius, y, sin * radius]);
            normals.push([cos, 0.0, sin]);
            uvs.push([u, y]);
        }
    }
    for i in 0..SEGMENTS as u32 {
        let (a, b, c, d) = (i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 3);
        indices.extend([a, c, b, b, c, d]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

/// Crée le pool d'entités, une fois, au démarrage. Tout est caché ; `sync` montre ce qui vit.
pub(super) fn spawn_beams(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<BeamMaterial>>,
) {
    let cone_mesh = meshes.add(cone_mesh());
    let pool_mesh = meshes.add(Rectangle::new(1.0, 1.0).mesh().build());
    let cone = materials.add(BeamMaterial::default());
    let pool = materials.add(BeamMaterial::default());
    for i in 0..MAX_BEAMS {
        commands.spawn((
            Name::new("projecteur · faisceau"),
            BeamPiece::Cone(i),
            Mesh3d(cone_mesh.clone()),
            MeshMaterial3d(cone.clone()),
            MeshTag::new(0),
            Visibility::Hidden,
        ));
        commands.spawn((
            Name::new("projecteur · tache"),
            BeamPiece::Pool(i),
            Mesh3d(pool_mesh.clone()),
            MeshMaterial3d(pool.clone()),
            MeshTag::new(0),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(BeamRig { cone, pool });
}
