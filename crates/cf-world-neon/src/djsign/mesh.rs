//! Le maillage de l'ossature : toutes les poutres de [`frame::build`](super::frame::build) fondues en un seul maillage
//! (un appel de dessin, comme les boîtes instanciées de Sillage). Reconstruit seulement quand le nom change.

use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::math::{Quat, Vec3, Vec3A};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

use super::frame::FrameBox;

/// Les six faces d'un pavé : normale, puis deux axes tels que `u × v = normale` (sens direct vu de dehors).
const FACES: [(Vec3, Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Y, Vec3::Z),
    (Vec3::NEG_X, Vec3::Z, Vec3::Y),
    (Vec3::Y, Vec3::Z, Vec3::X),
    (Vec3::NEG_Y, Vec3::X, Vec3::Z),
    (Vec3::Z, Vec3::X, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y, Vec3::X),
];
const CORNERS: [(f32, f32); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

/// Le maillage des poutres et sa boîte englobante (que Bevy ne recalcule pas quand on remplace un maillage). `None` sans
/// poutre.
#[must_use]
pub fn frame_mesh(boxes: &[FrameBox]) -> Option<(Mesh, Aabb)> {
    if boxes.is_empty() {
        return None;
    }
    let mut positions = Vec::with_capacity(boxes.len() * 24);
    let mut normals = Vec::with_capacity(boxes.len() * 24);
    let mut uvs = Vec::with_capacity(boxes.len() * 24);
    let mut indices = Vec::with_capacity(boxes.len() * 36);
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for b in boxes {
        let turn = Quat::from_rotation_z(b.roll);
        for (n, u, v) in FACES {
            let first = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            for (su, sv) in CORNERS {
                let local = 0.5 * (n + su * u + sv * v) * b.size;
                let p = b.center + turn * local;
                min = min.min(p);
                max = max.max(p);
                positions.push(p.to_array());
                normals.push((turn * n).to_array());
                uvs.push([f32::midpoint(su, 1.0), f32::midpoint(sv, 1.0)]);
            }
            indices.extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some((mesh, Aabb::from_min_max(min, max)))
}

/// Le centre de la boîte englobante, pour les tests.
#[must_use]
pub fn center_of(aabb: &Aabb) -> Vec3A {
    aabb.center
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::djsign::layout::{SIGN_LAYOUT, layout_sign};

    fn unit(center: Vec3, roll: f32) -> FrameBox {
        FrameBox {
            center,
            size: Vec3::new(2.0, 1.0, 4.0),
            roll,
        }
    }

    #[test]
    fn sans_poutre_pas_de_maillage() {
        assert!(frame_mesh(&[]).is_none());
    }

    #[test]
    fn une_poutre_donne_vingt_quatre_sommets_et_trente_six_indices() {
        let (mesh, _) = frame_mesh(&[unit(Vec3::ZERO, 0.0)]).expect("maillage");
        assert_eq!(mesh.count_vertices(), 24);
        assert_eq!(mesh.indices().map(Indices::len), Some(36));
    }

    #[test]
    fn la_boite_englobante_suit_la_poutre() {
        let (_, aabb) = frame_mesh(&[unit(Vec3::new(10.0, 5.0, -3.0), 0.0)]).expect("maillage");
        assert!((center_of(&aabb) - Vec3A::new(10.0, 5.0, -3.0)).length() < 1e-5);
        assert!((aabb.half_extents - Vec3A::new(1.0, 0.5, 2.0)).length() < 1e-5);
        // Tournée d'un quart de tour autour de z : x et y s'échangent.
        let (_, turned) = frame_mesh(&[unit(Vec3::ZERO, std::f32::consts::FRAC_PI_2)]).expect("maillage");
        assert!((turned.half_extents - Vec3A::new(0.5, 1.0, 2.0)).length() < 1e-4);
    }

    #[test]
    fn les_faces_regardent_vers_l_exterieur() {
        let (mesh, _) = frame_mesh(&[unit(Vec3::ZERO, 0.0)]).expect("maillage");
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(bevy::mesh::VertexAttributeValues::as_float3)
            .expect("positions");
        let normals = mesh
            .attribute(Mesh::ATTRIBUTE_NORMAL)
            .and_then(bevy::mesh::VertexAttributeValues::as_float3)
            .expect("normales");
        let indices: Vec<usize> = mesh.indices().expect("indices").iter().collect();
        for tri in indices.chunks(3) {
            let p = |i: usize| Vec3::from(positions[i]);
            let normal = (p(tri[1]) - p(tri[0])).cross(p(tri[2]) - p(tri[0])).normalize();
            assert!(normal.dot(Vec3::from(normals[tri[0]])) > 0.99, "face à l'envers");
        }
    }

    #[test]
    fn l_ossature_d_un_nom_complet_est_un_maillage_valide() {
        let layout = layout_sign("DJ NEON", &|_| 0.7, &SIGN_LAYOUT);
        let boxes = super::super::frame::build(&layout);
        let (mesh, aabb) = frame_mesh(&boxes).expect("maillage");
        assert_eq!(mesh.count_vertices(), boxes.len() * 24);
        assert!(aabb.half_extents.x > layout.width / 2.0);
    }
}
