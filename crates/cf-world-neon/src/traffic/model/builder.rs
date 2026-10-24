//! Assemble les pièces d'un véhicule en un seul maillage, comme `mergeGeometries` de Sillage : positions, normales et,
//! par sommet, la surface ([`super::super::variant::kind`]) dans le canal rouge de la couleur. Un maillage par modèle,
//! donc un seul lot de dessin par modèle (les entités qui le partagent avec le même matériau sont groupées par Bevy).

use bevy::asset::RenderAssetUsages;
use bevy::math::{Mat3, Mat4, Vec3};
use bevy::mesh::{Mesh, PrimitiveTopology};
use bevy::prelude::{Cuboid, MeshBuilder as _, Meshable};

/// Un maillage en construction : triangles non indexés.
#[derive(Debug, Default, Clone)]
pub struct MeshBuilder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    kinds: Vec<f32>,
}

impl MeshBuilder {
    /// Nombre de sommets.
    #[must_use]
    pub fn len(&self) -> usize {
        self.positions.len()
    }

    /// Aucun sommet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// Ajoute des triangles (indexés ou non), transformés par `transform`, de surface `kind`. Les normales suivent la
    /// transformation par l'inverse transposée (une échelle non uniforme ne les déforme pas).
    pub fn add_triangles(
        &mut self,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        indices: Option<&[u32]>,
        transform: Mat4,
        kind: f32,
    ) {
        let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();
        let mut push = |i: usize| {
            let p = transform.transform_point3(Vec3::from(positions[i]));
            let n = (normal_matrix * Vec3::from(normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]))).normalize_or_zero();
            self.positions.push(p.to_array());
            self.normals.push(n.to_array());
            self.kinds.push(kind);
        };
        match indices {
            Some(indices) => indices.iter().for_each(|&i| push(i as usize)),
            None => (0..positions.len()).for_each(&mut push),
        }
    }

    /// Ajoute un maillage de Bevy (boîte, cylindre, tore, sphère…), transformé.
    pub fn add_mesh(&mut self, mesh: &Mesh, transform: Mat4, kind: f32) {
        let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()) else {
            return;
        };
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(|a| a.as_float3())
            .unwrap_or(&[]);
        let indices: Option<Vec<u32>> = mesh.indices().map(|i| i.iter().map(|v| v as u32).collect());
        self.add_triangles(positions, normals, indices.as_deref(), transform, kind);
    }

    /// Ajoute une boîte de taille `size` centrée en `at`.
    pub fn add_box(&mut self, size: Vec3, at: Vec3, kind: f32) {
        let mesh = Cuboid::new(size.x, size.y, size.z).mesh().build();
        self.add_mesh(&mesh, Mat4::from_translation(at), kind);
    }

    /// Les coins min et max de la boîte englobante.
    #[must_use]
    pub fn bounds(&self) -> (Vec3, Vec3) {
        self.positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
            let p = Vec3::from(*p);
            (lo.min(p), hi.max(p))
        })
    }

    /// Applique `transform` (translation et échelle) à tout ce qu'il y a déjà : normales conservées.
    pub fn transform_all(&mut self, transform: Mat4) {
        for p in &mut self.positions {
            *p = transform.transform_point3(Vec3::from(*p)).to_array();
        }
    }

    /// Le maillage final : positions, normales, et la surface de chaque sommet dans le canal rouge de la couleur.
    #[must_use]
    pub fn finish(self) -> Mesh {
        let colors: Vec<[f32; 4]> = self.kinds.iter().map(|&k| [k, 0.0, 0.0, 1.0]).collect();
        // Les maillages gardent leur copie du monde principal : le culling (`Aabb`) la relit.
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_boite_donne_trente_six_sommets_et_ses_bornes() {
        let mut builder = MeshBuilder::default();
        builder.add_box(Vec3::new(2.0, 1.0, 4.0), Vec3::new(1.0, 0.0, 0.0), 3.0);
        assert_eq!(builder.len(), 36);
        let (lo, hi) = builder.bounds();
        assert!((lo - Vec3::new(0.0, -0.5, -2.0)).length() < 1e-6, "{lo}");
        assert!((hi - Vec3::new(2.0, 0.5, 2.0)).length() < 1e-6, "{hi}");
    }

    #[test]
    fn la_surface_de_chaque_sommet_est_dans_le_canal_rouge() {
        let mut builder = MeshBuilder::default();
        builder.add_box(Vec3::ONE, Vec3::ZERO, 5.0);
        builder.add_box(Vec3::ONE, Vec3::X, 7.0);
        let mesh = builder.finish();
        let colors = mesh
            .attribute(Mesh::ATTRIBUTE_COLOR)
            .and_then(|a| match a {
                bevy::mesh::VertexAttributeValues::Float32x4(v) => Some(v.clone()),
                _ => None,
            })
            .expect("couleurs");
        assert_eq!(colors.len(), 72);
        assert!(colors[..36].iter().all(|c| c[0] == 5.0));
        assert!(colors[36..].iter().all(|c| c[0] == 7.0));
    }

    #[test]
    fn une_echelle_non_uniforme_garde_des_normales_unitaires_et_perpendiculaires() {
        let mut builder = MeshBuilder::default();
        let sphere = bevy::prelude::Sphere::new(1.0).mesh().uv(12, 8);
        builder.add_mesh(&sphere, Mat4::from_scale(Vec3::new(0.2, 0.24, 1.15)), 1.0);
        assert!(builder.len() > 100);
        let mesh = builder.finish();
        let normals = mesh.attribute(Mesh::ATTRIBUTE_NORMAL).and_then(|a| a.as_float3()).expect("normales");
        assert!(normals.iter().all(|n| (Vec3::from(*n).length() - 1.0).abs() < 1e-4));
    }
}
