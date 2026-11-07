//! Les graffitis muraux côté matériau : la table qui dit au shader (`facade_paint.wesl`) où lire chaque motif dans
//! l'atlas, et le système qui la branche une fois l'atlas cuit.

use bevy::math::Vec4;
use bevy::prelude::*;
use bevy::render::render_resource::ShaderType;

use super::FacadeMaterial;
use crate::graffiti::GraffitiAtlas;
use crate::scenery::SceneryAssets;

/// Nombre d'entrées de la table : le tag réserve 5 bits au motif (0 : aucun), donc 31 motifs.
pub const WALL_SLOTS: usize = 32;

/// Où lire chaque motif mural. Même ordre et mêmes types que la structure `Wall` de `facade.wesl` ; l'entrée `k` est le
/// motif `k` du tag (l'entrée 0 reste vide). Table vide par défaut : aucun motif n'existe, la façade est celle d'avant.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct WallCells {
    /// `x` : largeur d'une couche de l'atlas (px), `y` : hauteur, `z` : nombre de couches.
    pub atlas: Vec4,
    /// Par motif : `x`, `y`, largeur, hauteur du rectangle utile dans la couche (px).
    pub rects: [Vec4; WALL_SLOTS],
    /// Par motif : `x` couche, `y` part de la hauteur occupée par les lettres, `w` 1 si le motif existe.
    pub info: [Vec4; WALL_SLOTS],
}

impl WallCells {
    /// La table d'un atlas cuit.
    #[must_use]
    pub fn from_atlas(atlas: &GraffitiAtlas) -> Self {
        let mut cells = Self {
            atlas: Vec4::new(
                atlas.wall_size.x as f32,
                atlas.wall_size.y as f32,
                atlas.wall_size.z as f32,
                0.0,
            ),
            ..Self::default()
        };
        for k in 1..WALL_SLOTS {
            let Some(cell) = u8::try_from(k).ok().and_then(|k| atlas.wall_cell(k)) else {
                continue;
            };
            cells.rects[k] = Vec4::new(
                f32::from(cell.x),
                f32::from(cell.y),
                f32::from(cell.w),
                f32::from(cell.h),
            );
            cells.info[k] = Vec4::new(f32::from(cell.layer), cell.letters, 0.0, 1.0);
        }
        cells
    }
}

/// `Update` : relie l'atlas mural au matériau de façade dès que les deux existent (une seule fois : le matériau est créé
/// au `setup` du monde, qui peut venir après la fin de la cuisson).
pub(super) fn bind_wall_atlas(
    atlas: Res<GraffitiAtlas>,
    scenery: Option<Res<SceneryAssets>>,
    materials: Option<ResMut<Assets<FacadeMaterial>>>,
) {
    let (Some(scenery), Some(mut materials)) = (scenery, materials) else {
        return;
    };
    if materials.get(&scenery.material).is_none_or(|m| m.wall.is_some()) {
        return;
    }
    let Some(mut material) = materials.get_mut(&scenery.material) else {
        return;
    };
    material.wall = Some(atlas.wall.clone());
    material.wall_cells = WallCells::from_atlas(&atlas);
}
