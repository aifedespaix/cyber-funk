//! Le champ d'étoiles : tirage déterministe (`stars` de `sky.ts`) et maillage.
//!
//! Sillage dessinait trois `Points` de pixels carrés. Bevy n'a pas de sprite de point : chaque étoile est un quad de quatre
//! sommets, tous au centre de l'étoile, que le vertex shader élargit en pixels à l'écran (même effet qu'un `gl_PointSize`).
//! Les trois couches tiennent dans **un seul mesh**, donc un seul dessin pour 6 370 étoiles.
//!
//! Les teintes ne sont pas cuites dans les sommets comme dans Sillage (qui devait les refaire à la fin de chaque fondu de
//! palette, `watchPalette`) : le sommet porte l'indice de la teinte, le shader la lit dans la palette courante.

use bevy::asset::RenderAssetUsages;
use bevy::math::Vec3;
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};

use super::spec::{RADIUS_M, STAR_LAYERS, StarLayer, galaxy_axis};
use crate::rng::hash;

/// Nombre de teintes d'étoiles (les six rôles de `tints` de `sky.ts`).
pub const TINT_COUNT: usize = 6;

/// Une étoile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Star {
    /// Position dans le repère du ciel, en m.
    pub position: Vec3,
    /// Taille à l'écran, en pixels (pour une image de 1 080 px de haut).
    pub size_px: f32,
    /// Phase du scintillement, 0..1.
    pub phase: f32,
    /// Indice dans le tableau des teintes : glace, glace, azur, menthe, ambre, corail.
    pub tint: u8,
    /// Atténuation de la couche (la poussière lointaine est un peu plus terne).
    pub dim: f32,
}

/// Tire toutes les étoiles : direction uniforme sur la sphère, dont une part est ramenée vers le plan de la Voie lactée.
///
/// Mêmes graines et mêmes sels que `Sky.stars` de Sillage : le ciel est le même d'une exécution à l'autre.
#[must_use]
pub fn star_field() -> Vec<Star> {
    let total: u32 = STAR_LAYERS.iter().map(|l| l.count).sum();
    let mut stars = Vec::with_capacity(total as usize);
    for (layer_index, layer) in STAR_LAYERS.iter().enumerate() {
        stars.extend((0..layer.count).map(|i| star(layer, layer_index, i)));
    }
    stars
}

fn star(layer: &StarLayer, layer_index: usize, i: u32) -> Star {
    let axis = galaxy_axis();
    let seed = f64::from(i);
    let salt = layer_index as f64 * 100.0;
    let h = |n: u8| hash(seed, salt + f64::from(n)) as f32;

    // Direction uniforme sur la sphère…
    let z = h(1) * 2.0 - 1.0;
    let a = h(2) * std::f32::consts::TAU;
    let r = (1.0 - z * z).sqrt();
    let mut dir = Vec3::new(r * a.cos(), z, r * a.sin());
    // …puis une partie est ramenée vers le plan de la Voie lactée.
    if h(3) < layer.galactic {
        let k = dir.dot(axis);
        let squash = 0.82 + h(4) * 0.12;
        dir -= axis * (k * squash);
        let norm = dir.length();
        dir /= if norm > 0.0 { norm } else { 1.0 };
    }
    let radius = RADIUS_M * (0.78 + h(5) * 0.15);
    Star {
        position: dir * radius,
        size_px: layer.size_px[0] + (layer.size_px[1] - layer.size_px[0]) * h(6).powf(layer.power),
        phase: h(7),
        tint: ((h(8) * TINT_COUNT as f32).floor() as usize).min(TINT_COUNT - 1) as u8,
        dim: if layer_index == 0 { 0.9 } else { 1.0 },
    }
}

/// Le mesh du champ : quatre sommets par étoile, tous au centre.
///
/// - `POSITION` : le centre de l'étoile ;
/// - `UV_0` : le coin du quad (0 ou 1), pour le vertex shader (décalage) et le fragment (forme) ;
/// - `UV_1` : taille en pixels et phase de scintillement ;
/// - `COLOR` : indice de teinte, atténuation (le reste est libre).
///
/// `Aabb` : celui que Bevy calcule des positions est juste (les centres sont tous dans la voûte ; l'élargissement du quad
/// se fait en pixels, jamais en mètres).
#[must_use]
pub fn star_mesh(stars: &[Star]) -> Mesh {
    let n = stars.len();
    let mut positions = Vec::with_capacity(n * 4);
    let mut corners = Vec::with_capacity(n * 4);
    let mut sizes = Vec::with_capacity(n * 4);
    let mut attrs = Vec::with_capacity(n * 4);
    let mut indices = Vec::with_capacity(n * 6);
    for (i, star) in stars.iter().enumerate() {
        for corner in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            positions.push(star.position.to_array());
            corners.push(corner);
            sizes.push([star.size_px, star.phase]);
            attrs.push([f32::from(star.tint), star.dim, 0.0, 0.0]);
        }
        let base = (i * 4) as u32;
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, corners)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, sizes)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, attrs)
        .with_inserted_indices(Indices::U32(indices))
}

#[cfg(test)]
mod tests {
    use bevy::mesh::VertexAttributeValues;

    use super::*;

    #[test]
    fn le_champ_a_les_trois_couches_au_complet() {
        let stars = star_field();
        assert_eq!(stars.len(), 5200 + 1100 + 70);
    }

    #[test]
    fn le_tirage_est_deterministe() {
        assert_eq!(star_field(), star_field());
    }

    #[test]
    fn rayon_taille_et_teinte_restent_dans_les_bornes_de_leur_couche() {
        let stars = star_field();
        let mut start = 0;
        for (index, layer) in STAR_LAYERS.iter().enumerate() {
            let end = start + layer.count as usize;
            for star in &stars[start..end] {
                let r = star.position.length();
                assert!(
                    (RADIUS_M * 0.78 - 0.1..=RADIUS_M * 0.93 + 0.1).contains(&r),
                    "rayon {r} (couche {index})"
                );
                assert!(
                    (layer.size_px[0]..=layer.size_px[1] + 1e-4).contains(&star.size_px),
                    "taille {}",
                    star.size_px
                );
                assert!((0.0..1.0).contains(&star.phase));
                assert!(usize::from(star.tint) < TINT_COUNT);
            }
            start = end;
        }
    }

    #[test]
    fn les_etoiles_galactiques_se_resserrent_autour_de_la_voie_lactee() {
        let axis = galaxy_axis();
        let stars = star_field();
        let layer = &STAR_LAYERS[0];
        let near_plane = |s: &Star| s.position.normalize().dot(axis).abs() < 0.2;
        let dusty = &stars[..layer.count as usize];
        let share = dusty.iter().filter(|s| near_plane(s)).count() as f32 / dusty.len() as f32;
        // Une direction uniforme tombe à moins de 0,2 du plan dans 20 % des cas ; avec 55 % ramenées vers lui, c'est ~ 55 %.
        assert!(share > 0.45, "{share}");
        let bright = &stars[stars.len() - STAR_LAYERS[2].count as usize..];
        let share = bright.iter().filter(|s| near_plane(s)).count() as f32 / bright.len() as f32;
        assert!(share < 0.45, "la couche brillante reste uniforme : {share}");
    }

    #[test]
    fn le_mesh_a_quatre_sommets_par_etoile_et_un_aabb_qui_les_contient() {
        let stars = star_field();
        let mesh = star_mesh(&stars);
        assert_eq!(mesh.count_vertices(), stars.len() * 4);
        assert_eq!(mesh.indices().map(Indices::len), Some(stars.len() * 6));
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(VertexAttributeValues::as_float3)
            .expect("positions");
        let aabb = cf_render::mesh_aabb(&mesh).expect("positions");
        assert!(cf_render::aabb_encloses_mesh(&aabb, &mesh, 1e-3));
        assert!(positions.iter().all(|p| Vec3::from(*p).length() <= RADIUS_M));
    }
}
