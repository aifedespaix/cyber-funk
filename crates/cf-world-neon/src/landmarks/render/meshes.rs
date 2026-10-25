//! Les maillages des pièces : un [`MeshSpec`] devient un `Mesh` de Bevy, une fois, à l'ouverture. Le cube et chaque plan de
//! même taille sont partagés (une seule poignée), pour que les pièces qui se ressemblent se regroupent au dessin.

use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::landmarks::part::{MeshData, MeshSpec};

/// Les maillages déjà créés.
#[derive(Default)]
pub(super) struct MeshLibrary {
    cube: Option<Handle<Mesh>>,
    quads: HashMap<(u32, u32), Handle<Mesh>>,
}

/// Un maillage de Bevy à partir de données pures. Les attributs optionnels (`uv1`, `colors`, `tangents`) ne sont insérés que
/// s'ils sont remplis, un par sommet.
#[must_use]
pub fn mesh_of(data: &MeshData) -> Mesh {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone())
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, data.uvs.clone());
    if !data.uv1.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, data.uv1.clone());
    }
    if !data.colors.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, data.colors.clone());
    }
    if !data.tangents.is_empty() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, data.tangents.clone());
    }
    mesh.insert_indices(Indices::U32(data.indices.clone()));
    mesh
}

impl MeshLibrary {
    /// La poignée du maillage de `spec`, créé au premier appel.
    pub(super) fn get(&mut self, spec: &MeshSpec, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        match spec {
            MeshSpec::Cube => self
                .cube
                .get_or_insert_with(|| meshes.add(mesh_of(&MeshData::cube())))
                .clone(),
            MeshSpec::Quad { w, h } => self
                .quads
                .entry((w.to_bits(), h.to_bits()))
                .or_insert_with(|| meshes.add(mesh_of(&MeshData::quad(*w, *h))))
                .clone(),
            MeshSpec::Custom(data) => meshes.add(mesh_of(data)),
        }
    }
}
