//! Les maillages des lumières de la voiture : volume de phare, flaques, quadrilatères des halos et des lueurs.
//!
//! Tous portent les quatre attributs que lit `car_light.wesl` (position, normale, UV, couleur) : un seul agencement de
//! sommets pour tous les motifs.

use bevy::asset::RenderAssetUsages;
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

/// Longueur du volume lumineux d'un phare, m (`BEAM_LENGTH`).
pub const BEAM_LENGTH_M: f32 = 28.0;
/// Rayon du volume à son extrémité, m (`BEAM_RADIUS`).
pub const BEAM_RADIUS_M: f32 = 2.6;
/// Rayon du volume à la pointe, m.
const BEAM_TIP_RADIUS_M: f32 = 0.06;
/// Pointe du faisceau vers le sol, rad : il lèche la route au lieu de partir dans le ciel (`BEAM_DIP`).
pub const BEAM_DIP_RAD: f32 = 0.055;
/// Largeur de la flaque de lumière devant la voiture, m (`POOL_WIDTH`).
pub const POOL_WIDTH_M: f32 = 7.0;
/// Profondeur de la flaque de lumière devant la voiture, m (`POOL_DEPTH`).
pub const POOL_DEPTH_M: f32 = 22.0;
/// Segments autour du cône.
const BEAM_SEGMENTS: u32 = 20;

fn build(positions: Vec<[f32; 3]>, normals: Vec<[f32; 3]>, uvs: Vec<[f32; 2]>, indices: Vec<u32>) -> Mesh {
    let colors = vec![[1.0_f32; 4]; positions.len()];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

/// Volume lumineux d'un phare : un cône ouvert qui part de l'origine vers −Z, incliné de [`BEAM_DIP_RAD`] vers le sol.
///
/// `uv.y` vaut 0 à la pointe et 1 au bout ; les normales sont celles d'un cône (comme `CylinderGeometry` de three.js),
/// ce qui donne l'effet de volume (plus dense au centre qu'aux bords).
#[must_use]
pub fn beam() -> Mesh {
    let tilt = Quat::from_rotation_x(-BEAM_DIP_RAD);
    let axis = tilt * Vec3::NEG_Z;
    let (right, up) = (tilt * Vec3::X, tilt * Vec3::Y);
    let slope = (BEAM_RADIUS_M - BEAM_TIP_RADIUS_M) / BEAM_LENGTH_M;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for (along, radius) in [(0.0_f32, BEAM_TIP_RADIUS_M), (1.0, BEAM_RADIUS_M)] {
        for i in 0..=BEAM_SEGMENTS {
            let theta = i as f32 / BEAM_SEGMENTS as f32 * std::f32::consts::TAU;
            let radial = right * theta.cos() + up * theta.sin();
            positions.push((axis * (along * BEAM_LENGTH_M) + radial * radius).to_array());
            normals.push((radial - axis * slope).normalize().to_array());
            uvs.push([i as f32 / BEAM_SEGMENTS as f32, along]);
        }
    }
    let ring = BEAM_SEGMENTS + 1;
    let mut indices = Vec::new();
    for i in 0..BEAM_SEGMENTS {
        let (a, b, c, d) = (i, i + 1, ring + i, ring + i + 1);
        indices.extend_from_slice(&[a, c, b, b, c, d]);
    }
    build(positions, normals, uvs, indices)
}

/// Flaque de lumière : un rectangle posé sur la route (plan XZ), de `width` m de large, qui part de z = 0 et s'étend de
/// `depth` m vers −Z (`direction` = −1, devant la voiture) ou vers +Z (+1, derrière). `uv.y` vaut 0 contre la voiture et
/// 1 au bout de la flaque.
#[must_use]
pub fn pool(width: f32, depth: f32, direction: f32) -> Mesh {
    let h = width / 2.0;
    let z = direction.signum() * depth;
    build(
        vec![[-h, 0.0, 0.0], [h, 0.0, 0.0], [-h, 0.0, z], [h, 0.0, z]],
        vec![[0.0, 1.0, 0.0]; 4],
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        if direction < 0.0 {
            vec![0, 2, 1, 1, 2, 3]
        } else {
            vec![0, 1, 2, 1, 3, 2]
        },
    )
}

/// Carré unité posé sur la route (plan XZ, centré) : l'échelle du nœud en fait une lueur ronde (`PlaneGeometry(1, 1)`
/// couché de `glow.ts`).
#[must_use]
pub fn ground_quad() -> Mesh {
    build(
        vec![[-0.5, 0.0, -0.5], [0.5, 0.0, -0.5], [-0.5, 0.0, 0.5], [0.5, 0.0, 0.5]],
        vec![[0.0, 1.0, 0.0]; 4],
        vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
        vec![0, 2, 1, 1, 2, 3],
    )
}

/// Carré unité dans le plan XY (centré) : le halo, que le shader tourne vers la caméra (`Sprite` de three.js).
#[must_use]
pub fn halo_quad() -> Mesh {
    build(
        vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [-0.5, 0.5, 0.0], [0.5, 0.5, 0.0]],
        vec![[0.0, 0.0, 1.0]; 4],
        vec![[0.0, 1.0], [1.0, 1.0], [0.0, 0.0], [1.0, 0.0]],
        vec![0, 1, 2, 1, 3, 2],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn positions(mesh: &Mesh) -> Vec<[f32; 3]> {
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .map(<[_]>::to_vec)
            .unwrap_or_default()
    }

    #[test]
    fn le_faisceau_part_de_la_pointe_vers_l_avant_et_leche_le_sol() {
        let mesh = beam();
        let points = positions(&mesh);
        let tip = Vec3::from(points[0]);
        assert!(tip.length() < BEAM_TIP_RADIUS_M + 1e-5, "pointe à l'origine");
        let end_center = points[(BEAM_SEGMENTS as usize + 1)..]
            .iter()
            .fold(Vec3::ZERO, |sum, &p| sum + Vec3::from(p))
            / (BEAM_SEGMENTS as f32 + 1.0);
        assert!((end_center.length() - BEAM_LENGTH_M).abs() < 0.05, "{end_center}");
        assert!(end_center.z < -BEAM_LENGTH_M * 0.99, "vers l'avant (−Z) : {end_center}");
        assert!(end_center.y < 0.0, "vers le sol : {end_center}");
        assert!((end_center.y / BEAM_LENGTH_M + BEAM_DIP_RAD.sin()).abs() < 1e-3);
    }

    #[test]
    fn les_lumieres_ont_les_quatre_attributs_du_shader() {
        for mesh in [beam(), pool(7.0, 22.0, -1.0), pool(7.0, 6.0, 1.0), ground_quad(), halo_quad()] {
            for attribute in [
                Mesh::ATTRIBUTE_POSITION,
                Mesh::ATTRIBUTE_NORMAL,
                Mesh::ATTRIBUTE_UV_0,
                Mesh::ATTRIBUTE_COLOR,
            ] {
                assert!(mesh.attribute(attribute.id).is_some(), "{}", attribute.name);
            }
        }
    }

    #[test]
    fn la_flaque_s_etend_devant_ou_derriere_avec_uv_nul_contre_la_voiture() {
        let front = pool(7.0, 22.0, -1.0);
        assert!(positions(&front).iter().all(|p| p[2] <= 0.0));
        let rear = pool(7.0, 6.0, 1.0);
        assert!(positions(&rear).iter().all(|p| p[2] >= 0.0));
        let uvs = front
            .attribute(Mesh::ATTRIBUTE_UV_0)
            .and_then(|a| match a {
                bevy::mesh::VertexAttributeValues::Float32x2(v) => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_default();
        // Les sommets contre la voiture (z = 0) ont `uv.y` = 0.
        for (p, uv) in positions(&front).iter().zip(&uvs) {
            assert_eq!(p[2] == 0.0, uv[1] == 0.0);
        }
    }
}
