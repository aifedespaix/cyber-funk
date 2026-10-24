//! La ville de Neon City : gratte-ciel de chaque côté de la route (`scenery.ts`) et arches (`bridges.ts`).
//!
//! « Cité de gratte-ciel de chaque côté de la route. Tout est recalculé à partir de l'index de rangée (graine
//! déterministe) : le décor est infini et ne bouge que lorsqu'on franchit une rangée. » Ici une rangée est un tronçon du
//! streaming de `cf-render` (26 m, 15 derrière le point suivi et 18 devant, les bornes de Sillage) :
//!
//! - [`layout`] : la ville d'une rangée, fonction pure de son index ([`Row::generate`]), testée contre Sillage ;
//! - [`plan`] : la rangée devient un [`ChunkPlan`](cf_render::ChunkPlan) (pavés en repère de tronçon, tag pour le shader) ;
//! - [`query`] : collisions, toit, façade, répondues en régénérant les rangées voisines (pas d'état à tenir d'accord) ;
//! - [`arches`] : les deux arches affichées au plus, six entités préallouées posées d'après le plan de route.
//!
//! # Culling et coût
//!
//! Les immeubles sont le premier poste de coût : une rangée porte jusqu'à 36 pavés, 34 rangées vivent à la fois, soit
//! 1 224 entités préallouées (rien n'est créé ni détruit en régime établi).
//!
//! 1. **Frustum** : chaque pavé a l'`Aabb` exact du pavé unité, mis à l'échelle par son `Transform`. Pas de
//!    `NoFrustumCulling` (Sillage avait `frustumCulled = false` sur un seul `InstancedMesh`, ce qui dessinait toute la
//!    ville à chaque image).
//! 2. **Occlusion** : les façades écrivent la profondeur du passage de profondeur de la caméra du monde (même
//!    déplacement que le dessin, voir `facade_prepass.wesl`) ; les tours s'occultent entre elles.
//! 3. **Distance** : [`until_fog`] sur chaque pièce, mesurée à son `Aabb` ([`FOG_END_M`]) : au-delà de la brume
//!    (380 m, la constante de culling `FOG_FULL_M`) rien n'est testé ni dessiné.
//! 4. **Simulation** : rien à geler, les façades s'animent par le seul matériau (un buffer par image).
//! 5. **Qualité** : [`SceneryConfig::set_quality`] retire d'abord les couronnes lointaines (`FAR_BY_QUALITY`).
//!
//! # Couches
//!
//! Chaque pièce porte [`WorldLayer`](cf_render::WorldLayer) par le lot déclaré à `PieceRegistry::add` : Bevy ne propage
//! pas les `RenderLayers` aux enfants d'un tronçon.

pub mod arches;
pub mod layout;
mod plan;
pub mod query;

use bevy::camera::visibility::VisibilityRange;
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use cf_render::streaming::ArchetypeId;

pub use layout::{
    BASE_DEPTH_M, Clearing, FAR_BY_QUALITY, FLOOR_HEIGHT_M, Hitbox, MAX_PARTS, Part, Row, SPACING_M, SceneryConfig,
    SpotAnchor, TINT_CYCLE_LEN, Tier,
};
pub(crate) use plan::plan_row;

/// Distance de l'`Aabb` d'une pièce à la caméra au-delà de laquelle elle n'est plus dessinée : la distance où la brume a
/// tout avalé ([`cf_render::FOG_FULL_M`], `smoothstep(90, 380, d)` de `building-material.ts`).
pub const FOG_END_M: f32 = cf_render::FOG_FULL_M;

/// Plage de visibilité d'une pièce : abrupte à [`FOG_END_M`], mesurée à son `Aabb`.
///
/// `until_fog` mesure à l'origine de l'entité ; or un corps de tour a son origine 384 m sous la route (il plonge sous la
/// nappe de brouillard) : il serait coupé à 0 m de la caméra. La distance doit être celle du pavé, d'où `use_aabb`.
#[must_use]
pub fn fog_range() -> VisibilityRange {
    VisibilityRange { use_aabb: true, ..cf_render::until_fog(FOG_END_M) }
}

/// Ce que la ville partage : le pavé unité, le matériau de façade, la famille de pièces des tronçons. Créés une fois, dans
/// `World::setup`.
#[derive(Resource, Debug, Clone)]
pub struct SceneryAssets {
    /// Pavé unité, base à y = 0 (`BoxGeometry(1,1,1).translate(0, 0.5, 0)` de Sillage).
    pub mesh: Handle<Mesh>,
    /// Matériau de façade, unique.
    pub material: Handle<crate::facade::FacadeMaterial>,
    /// Famille des pavés des tronçons.
    pub pieces: ArchetypeId,
}

/// Le pavé unité de la ville : une boîte de 1 m d'arête dont la base est à y = 0.
#[must_use]
pub fn unit_block() -> Mesh {
    Mesh::from(Cuboid::new(1.0, 1.0, 1.0)).translated_by(Vec3::Y * 0.5)
}

/// Le `MeshTag` d'une pièce, pour les systèmes qui en posent un à la main (arches).
#[must_use]
pub(crate) fn mesh_tag(tag: u32) -> MeshTag {
    MeshTag::new(tag)
}
