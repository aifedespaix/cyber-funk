//! Niveau 2 du culling : l'occlusion par le GPU, sur la caméra du monde.
//!
//! # Statut dans Bevy 0.20
//!
//! - **API** : `bevy::render::occlusion_culling::OcclusionCulling`, un composant marqueur à poser sur une caméra.
//!   Elle vit dans `bevy_render::occlusion_culling`, hors de tout module `experimental` ; seul le générateur de pyramide de profondeur qu'elle
//!   utilise reste dans `bevy::core_pipeline::mip_generation::experimental` (la documentation de Bevy le dit
//!   expérimental, et l'exemple officiel `occlusion_culling` est étiqueté « Demonstration »). On la traite donc comme
//!   **stable d'API, jeune d'implémentation** : d'où le réglage pour la couper.
//! - **Désactivée par défaut** dans Bevy : elle a un coût fixe (pyramide de profondeur, test de chaque boîte
//!   englobante) qu'il faut gagner par des objets masqués.
//! - **Exige un `DepthPrepass`** sur la même caméra ; sans lui, Bevy ignore le composant (sa documentation le dit).
//!   Incompatible avec le rendu différé (non utilisé ici).
//! - **Exige le GPU preprocessing complet** (`GpuPreprocessingSupport::is_culling_supported` : compute shaders,
//!   dessin indirect). Sur un GPU qui ne l'a pas, le composant est ignoré : le rendu reste correct, sans le gain.
//!   Bevy le journalise (`GPU preprocessing is not supported` / `limited on this device`).
//! - **Deux phases** : les meshes visibles à l'image précédente sont dessinés d'abord pour produire une profondeur
//!   conservatrice, puis tous les autres sont testés contre elle et ceux qui passent sont dessinés dans la même
//!   image. Aucun objet n'est donc perdu en se découvrant ; en contrepartie, le coût GPU d'une image dépend de ce
//!   qui était visible à la précédente.
//! - **Invisible du processeur** : l'occlusion se joue dans le GPU. `ViewVisibility`, `VisibleEntities` et donc
//!   le nombre d'entités « visibles » du processeur ne la reflètent pas ; seule la lecture des paramètres de dessin
//!   indirect le fait (voir l'exemple `culling_city`).
//!
//! # Ce que fait ce module
//!
//! Les caméras marquées [`CullingView`] reçoivent `DepthPrepass` et `OcclusionCulling` quand
//! [`CullingSettings::occlusion`](super::CullingSettings::occlusion) est vrai, les perdent quand il passe à faux.
//! La caméra de la couche monde du rig ([`Layer::World`]) est marquée d'office ; les autres couches ne le sont pas
//! (le fond est une voûte opaque qui n'occulte rien, l'avant-plan est trop près et trop petit pour que la pyramide
//! de profondeur rapporte).

use bevy::core_pipeline::prepass::DepthPrepass;
use bevy::prelude::*;
use bevy::render::occlusion_culling::OcclusionCulling;

use super::{CullingSettings, CullingView};
use crate::layers::{Layer, LayerCamera};

/// Le `DepthPrepass` de cette caméra a été posé par le culling : il part avec l'occlusion. Un `DepthPrepass` qui était
/// là avant (pour une autre raison) n'est jamais retiré.
#[derive(Component, Debug, Default, Clone, Copy)]
pub(crate) struct OwnsDepthPrepass;

/// Caméras de couche qui viennent d'apparaître et ne sont pas encore des vues de culling.
type NewLayerCameras<'w, 's> =
    Query<'w, 's, (Entity, &'static LayerCamera), (Added<LayerCamera>, Without<CullingView>)>;

/// Vues de culling avec ce qu'elles portent déjà : occlusion, prépasse de profondeur, prépasse posé par le culling.
type CullingViews<'w, 's> =
    Query<'w, 's, (Entity, Has<OcclusionCulling>, Has<DepthPrepass>, Has<OwnsDepthPrepass>), With<CullingView>>;

/// Marque d'office la caméra de la couche monde comme vue de culling.
pub(crate) fn adopt_world_camera(mut commands: Commands, cameras: NewLayerCameras) {
    for (entity, LayerCamera(layer)) in &cameras {
        if *layer == Layer::World {
            commands.entity(entity).insert(CullingView);
        }
    }
}

/// Met les caméras de culling d'accord avec [`CullingSettings::occlusion`]. Ne travaille que quand le réglage change
/// ou qu'une caméra de culling vient d'apparaître : aucun coût en régime établi.
pub(crate) fn sync_occlusion(
    mut commands: Commands,
    settings: Res<CullingSettings>,
    views: CullingViews,
    fresh: Query<(), Added<CullingView>>,
) {
    if !settings.is_changed() && fresh.is_empty() {
        return;
    }
    for (entity, occlusion, prepass, owns_prepass) in &views {
        let mut camera = commands.entity(entity);
        if settings.occlusion {
            if !prepass {
                camera.insert((DepthPrepass, OwnsDepthPrepass));
            }
            if !occlusion {
                camera.insert(OcclusionCulling);
            }
        } else {
            if occlusion {
                camera.remove::<OcclusionCulling>();
            }
            if owns_prepass {
                camera.remove::<(DepthPrepass, OwnsDepthPrepass)>();
            }
        }
    }
}
