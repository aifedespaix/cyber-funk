//! Les constructions annexes des vitrines (`build` de `FacadeSpec`) : le faisceau du phare, les ondes de la tour radio et
//! les blocs de la pagode. Maillages et constantes seulement ; la logique de pas est dans [`super::Showcase`].

use bevy::math::Vec3;

use super::super::part::MeshData;

/// Hauteur de la lanterne du phare, m.
pub const LAMP: f32 = 94.0;
/// Longueur du faisceau, m.
pub const BEAM: f32 = 150.0;
/// Largeur du faisceau, m.
pub const BEAM_WIDTH: f32 = 15.0;
/// Le faisceau penche vers le bas : à 60 m de la lampe il est encore au-dessus des gratte-ciel voisins.
pub const TILT: f32 = 0.1;
/// Portée de la lumière qui suit le bout du faisceau, m.
pub const BEAM_REACH: f32 = 55.0;
/// Hauteur de la pointe d'antenne de la tour radio, m.
pub const RADIO_TOP: f32 = 102.0;
/// Côté des ondes de la tour radio, m.
pub const WAVES: f32 = 90.0;

/// Étages de la pagode.
pub const TIERS: usize = 5;
/// Hauteur d'un étage, m.
pub const TIER_H: f32 = 14.0;
/// Altitude du pied du premier étage, m.
pub const BASE_Y: f32 = 4.0;
/// Largeur de chaque étage : le dernier est le plus étroit, m.
pub const WIDTHS: [f32; TIERS] = [30.0, 26.0, 22.0, 18.0, 14.0];
/// Débord du toit de chaque étage, de chaque côté, m.
pub const EAVE: f32 = 4.5;
/// Profondeur de la pagode, m.
pub const PAGODA_DEPTH: f32 = 26.0;
/// Nombre de blocs : trois par étage et la flèche.
pub const BLOCKS: usize = TIERS * 3 + 1;

/// Les deux plans en croix du faisceau : un couché, un debout, de [`BEAM`] × [`BEAM_WIDTH`] m, qui partent de l'origine le long
/// de +X. `uv.x` = distance le long du faisceau, `uv.y` = travers (le shader).
#[must_use]
pub fn beam_mesh() -> MeshData {
    let half = BEAM_WIDTH / 2.0;
    let mut mesh = MeshData::default();
    for flat in [true, false] {
        let first = mesh.positions.len() as u32;
        for (x, across, u, v) in [
            (0.0, -half, 0.0, 0.0),
            (BEAM, -half, 1.0, 0.0),
            (BEAM, half, 1.0, 1.0),
            (0.0, half, 0.0, 1.0),
        ] {
            mesh.positions
                .push(if flat { [x, 0.0, across] } else { [x, across, 0.0] });
            mesh.normals.push(if flat { [0.0, 1.0, 0.0] } else { [0.0, 0.0, 1.0] });
            mesh.uvs.push([u, v]);
        }
        mesh.indices
            .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    mesh
}

/// Ajoute un pavé : `x` et `z` au centre, `y` au pied (`BlockSet.set` de Sillage), arêtes `w` × `h` × `d`.
fn push_block(mesh: &mut MeshData, x: f32, y: f32, z: f32, size: Vec3) {
    let first = mesh.push_cube(Vec3::ZERO, 1.0) as usize;
    let center = Vec3::new(x, y + size.y / 2.0, z);
    for position in &mut mesh.positions[first..] {
        *position = (Vec3::from(*position) * size + center).to_array();
    }
}

/// Les seize blocs de la pagode (`build` de `pagodaSpec`) : par étage un volume plein, un toit large qui déborde et une dalle de
/// faîtage plus étroite ; une flèche au sommet. Le premier étage plonge de 8 m sous le sol, comme les gratte-ciel.
#[must_use]
pub fn pagoda_mesh() -> MeshData {
    let mut mesh = MeshData::default();
    let depth = PAGODA_DEPTH;
    for (t, width) in WIDTHS.iter().copied().enumerate() {
        let level = t as f32;
        let y = BASE_Y + level * TIER_H;
        let sink = if t == 0 { 8.0 } else { 0.0 };
        let narrowing = 2.0 * level;
        push_block(
            &mut mesh,
            0.0,
            y - sink,
            -depth / 2.0,
            Vec3::new(width, TIER_H - 2.0 + sink, depth - narrowing),
        );
        let roof_y = y + TIER_H - 2.0;
        push_block(
            &mut mesh,
            0.0,
            roof_y,
            -depth / 2.0 + EAVE * 0.2,
            Vec3::new(width + EAVE * 2.0, 1.6, depth - narrowing + EAVE * 1.2),
        );
        push_block(
            &mut mesh,
            0.0,
            roof_y + 1.6,
            -depth / 2.0,
            Vec3::new(width + EAVE * 0.6, 1.0, depth - narrowing),
        );
    }
    push_block(
        &mut mesh,
        0.0,
        BASE_Y + TIERS as f32 * TIER_H,
        -depth / 2.0,
        Vec3::new(2.4, 12.0, 2.4),
    );
    mesh
}
