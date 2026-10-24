//! Les maillages que le trafic crée lui-même : la traînée (deux plans en croix) et les deux maillages dynamiques, points
//! lumineux et murs de lumière, dont les sommets sont réécrits sur place à chaque image (capacité fixe, aucune
//! allocation).

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology, VertexAttributeValues};

use crate::traffic::glints::{GLINT_CAPACITY, Glint};
use crate::traffic::walls::{VERTEX_COUNT as WALL_VERTICES, WallVertices, wall_indices};

/// Sommets d'un point lumineux : un carré.
const GLINT_CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];

/// Deux plans en croix, de z = 0 (le véhicule) à z = 1 (le bout de la traînée) : l'un horizontal, l'autre vertical,
/// pour rester visibles sous tous les angles. Positions seules (le shader n'en lit pas d'autres).
#[must_use]
pub fn trail_mesh() -> Mesh {
    let positions: Vec<[f32; 3]> = vec![
        // Horizontal (plan XZ).
        [-0.5, 0.0, 0.0],
        [0.5, 0.0, 0.0],
        [0.5, 0.0, 1.0],
        [-0.5, 0.0, 1.0],
        // Vertical (plan YZ).
        [0.0, -0.5, 0.0],
        [0.0, 0.5, 0.0],
        [0.0, 0.5, 1.0],
        [0.0, -0.5, 1.0],
    ];
    let indices = vec![0_u32, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7];
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_indices(Indices::U32(indices))
}

/// Le maillage des points lumineux : quatre sommets par point, vides au départ (taille nulle).
#[must_use]
pub fn glint_mesh() -> Mesh {
    let corners: Vec<[f32; 2]> = (0..GLINT_CAPACITY).flat_map(|_| GLINT_CORNERS).collect();
    let mut indices = Vec::with_capacity(GLINT_CAPACITY * 6);
    for i in 0..GLINT_CAPACITY as u32 {
        let a = i * 4;
        indices.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; GLINT_CAPACITY * 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, corners)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; GLINT_CAPACITY * 4])
        .with_inserted_indices(Indices::U32(indices))
}

/// Écrit les points de l'image dans le maillage et rend le nombre de points écrits. Les points de l'image précédente
/// qui ne sont plus là sont éteints (taille nulle) ; `previous` est ce nombre.
pub fn write_glints(mesh: &mut Mesh, glints: &[Glint], previous: usize) -> usize {
    let n = glints.len().min(GLINT_CAPACITY);
    let touched = n.max(previous.min(GLINT_CAPACITY));
    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.try_attribute_mut(Mesh::ATTRIBUTE_POSITION).ok() {
        for (i, glint) in glints.iter().take(n).enumerate() {
            positions[i * 4..i * 4 + 4].fill(glint.offset.to_array());
        }
    }
    if let Some(VertexAttributeValues::Float32x4(colors)) = mesh.try_attribute_mut(Mesh::ATTRIBUTE_COLOR).ok() {
        for i in 0..touched {
            let value = glints.get(i).map_or([0.0; 4], |g| [g.color[0], g.color[1], g.color[2], g.size]);
            colors[i * 4..i * 4 + 4].fill(value);
        }
    }
    n
}

/// Le maillage des murs de lumière : une bande de triangles par ruban, indices fixes.
#[must_use]
pub fn wall_mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; WALL_VERTICES])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; WALL_VERTICES])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; WALL_VERTICES])
        .with_inserted_indices(Indices::U32(wall_indices()))
}

/// Les tableaux de sommets des murs, à remplir sur place (`None` si le maillage n'a pas la bonne forme).
pub fn wall_vertices(mesh: &mut Mesh) -> Option<WallVertices<'_>> {
    let (mut positions, mut fades, mut colors) = (None, None, None);
    for (attribute, values) in mesh.try_attributes_mut().ok()? {
        match values {
            VertexAttributeValues::Float32x3(v) if attribute.id == Mesh::ATTRIBUTE_POSITION.id => positions = Some(v),
            VertexAttributeValues::Float32x2(v) if attribute.id == Mesh::ATTRIBUTE_UV_0.id => fades = Some(v),
            VertexAttributeValues::Float32x4(v) if attribute.id == Mesh::ATTRIBUTE_COLOR.id => colors = Some(v),
            _ => {}
        }
    }
    Some(WallVertices {
        positions: positions?.as_mut_slice(),
        colors: colors?.as_mut_slice(),
        fades: fades?.as_mut_slice(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::math::Vec3;

    #[test]
    fn la_traine_va_de_zero_a_un_en_z_sur_deux_plans() {
        let mesh = trail_mesh();
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).expect("positions");
        assert_eq!(positions.len(), 8);
        assert!(positions.iter().all(|p| (0.0..=1.0).contains(&p[2])));
        assert_eq!(mesh.indices().map(Indices::len), Some(12));
    }

    #[test]
    fn les_points_s_ecrivent_sur_place_et_les_anciens_s_eteignent() {
        let mut mesh = glint_mesh();
        let glints = [
            Glint { offset: Vec3::new(1.0, 2.0, 3.0), color: [1.0, 0.5, 0.0], size: 0.8 },
            Glint { offset: Vec3::new(4.0, 5.0, 6.0), color: [0.0, 1.0, 0.0], size: 1.2 },
        ];
        assert_eq!(write_glints(&mut mesh, &glints, 0), 2);
        let colors = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_COLOR) {
            Some(VertexAttributeValues::Float32x4(v)) => v.clone(),
            _ => Vec::new(),
        };
        assert_eq!(colors(&mesh)[0], [1.0, 0.5, 0.0, 0.8]);
        assert_eq!(colors(&mesh)[7], [0.0, 1.0, 0.0, 1.2]);
        // L'image suivante n'a plus qu'un point : le second est éteint.
        assert_eq!(write_glints(&mut mesh, &glints[..1], 2), 1);
        assert_eq!(colors(&mesh)[4][3], 0.0, "taille nulle : rien à dessiner");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).and_then(|a| a.as_float3()).expect("positions");
        assert_eq!(positions[3], [1.0, 2.0, 3.0]);
    }

    #[test]
    fn les_murs_ont_leurs_trois_tableaux_de_la_bonne_taille() {
        let mut mesh = wall_mesh();
        let vertices = wall_vertices(&mut mesh).expect("attributs");
        assert_eq!(vertices.positions.len(), WALL_VERTICES);
        assert_eq!(vertices.colors.len(), WALL_VERTICES);
        assert_eq!(vertices.fades.len(), WALL_VERTICES);
    }
}
