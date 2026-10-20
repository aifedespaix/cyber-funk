//! Culling : ne dessiner et ne calculer que ce qui se voit, en quatre niveaux.
//!
//! | Niveau | Où | Module | Ce que ça économise |
//! | ------ | -- | ------ | ------------------- |
//! | 1. Frustum | processeur, natif Bevy | [`bounds`] | les objets hors du champ de la caméra |
//! | 2. Occlusion | GPU, `OcclusionCulling` | [`occlusion`] | les objets cachés derrière d'autres |
//! | 3. Simulation | processeur | [`lod`] | l'animation des objets hors champ ou lointains |
//! | 4. Distance / LOD | processeur, `VisibilityRange` | [`range`] | les objets au-delà de la brume, les détails au loin |
//!
//! Le culling économise du **calcul** (processeur et GPU). La **mémoire** se tient ailleurs : le streaming du monde
//! et ses pools gardent le nombre d'entités et de buffers constant (voir [`crate::streaming`]).
//!
//! [`CullingPlugin`] installe le réglage [`CullingSettings`], l'occlusion sur la caméra du monde et la cadence des
//! objets [`SimulationLod`]. Les niveaux 1 et 4 n'ont rien à installer : ce sont des composants que les mondes posent
//! sur leurs pièces ([`until_fog`], [`follow_camera_bounds`]).
//!
//! # Ce que les mondes doivent faire
//!
//! - **Ne jamais poser `NoFrustumCulling`.** Un mesh dont l'`Aabb` automatique est faux (il suit la caméra, ou son
//!   vertex shader le déforme) reçoit un volume juste par [`follow_camera_bounds`] ou [`displaced_bounds`].
//! - Poser [`until_fog`] (ou une chaîne [`lod_ranges`]) sur les objets qui n'ont plus d'intérêt à la distance où la
//!   brume les a avalés.
//! - Poser [`SimulationLod`] sur les objets dont l'animation coûte, et faire consulter [`SimulationLod::due`] et
//!   [`SimulationLod::step_s`] par les systèmes d'animation, rangés `.after(CullingSet::Lod)`.
//! - Marquer [`CullingView`] une caméra qui doit porter l'occlusion (la caméra de la couche monde l'est d'office).
//!
//! # Mesures
//!
//! Les chiffres avant/après du jalon (J4-04) sont dans la description de la PR. Le processeur compte les entités
//! dessinables par `ViewVisibility` ; l'occlusion, qui se joue dans le GPU, n'y paraît pas.

pub mod bounds;
pub mod lod;
mod occlusion;
pub mod range;

use bevy::prelude::*;

pub use bounds::{aabb_encloses_mesh, displaced_bounds, follow_camera_bounds, mesh_aabb, padded};
pub use lod::{Cadence, LodBands, LodClock, SimulationLod};
pub use range::{FOG_FULL_M, lod_ranges, until_fog};

use crate::schedule::CfSet;
use crate::world::WorldSet;

/// Réglages du culling, modifiables en cours de show (panneau opérateur) : chaque changement est repris à l'image
/// suivante.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CullingSettings {
    /// Occlusion GPU sur les caméras [`CullingView`] (avec `DepthPrepass`). Voir le module `occlusion` pour son
    /// statut dans Bevy 0.20.
    pub occlusion: bool,
    /// Cadence de simulation par objet. À `false`, tous les [`SimulationLod`] restent à pleine cadence.
    pub simulation_lod: bool,
}

impl Default for CullingSettings {
    fn default() -> Self {
        Self {
            occlusion: true,
            simulation_lod: true,
        }
    }
}

/// Une caméra qui porte l'occlusion GPU et sert de point de vue pour la cadence de simulation.
///
/// Posée d'office sur la caméra de la couche monde du rig ([`crate::LayersPlugin`]). Hors du rig, la poser à la main
/// sur la caméra 3D qui filme le monde.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct CullingView;

/// Étapes du culling dans l'image.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CullingSet {
    /// `CfSet::Simulate`, après `WorldSet::Chunks` : la cadence de chaque [`SimulationLod`] est fixée. Les systèmes
    /// d'animation des mondes se placent après.
    Lod,
    /// `CfSet::Present` : la caméra du monde est adoptée, l'occlusion est mise d'accord avec [`CullingSettings`].
    Occlusion,
}

/// Installe le culling : réglages, occlusion sur la caméra du monde, cadence de simulation.
///
/// Ne dépend pas de la présence du rig de couches : sans caméra marquée [`CullingView`], l'occlusion n'a rien à
/// équiper et les [`SimulationLod`] restent à pleine cadence.
#[derive(Debug, Default)]
pub struct CullingPlugin {
    /// Réglages de départ.
    pub settings: CullingSettings,
}

impl Plugin for CullingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.settings)
            .init_resource::<LodClock>()
            .configure_sets(Update, CullingSet::Lod.in_set(CfSet::Simulate).after(WorldSet::Chunks))
            .configure_sets(Update, CullingSet::Occlusion.in_set(CfSet::Present))
            .add_systems(Update, lod::update_simulation_lod.in_set(CullingSet::Lod))
            .add_systems(
                Update,
                (occlusion::adopt_world_camera, occlusion::sync_occlusion)
                    .chain()
                    .in_set(CullingSet::Occlusion),
            );
    }
}
