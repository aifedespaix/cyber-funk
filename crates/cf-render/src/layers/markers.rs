//! Marqueurs de couche : placent une entité dans la couche voulue.
//!
//! Chacun exige le `RenderLayers` de sa couche : `commands.spawn((WorldLayer, Mesh3d(..)))` suffit. Bevy ne propage pas
//! les `RenderLayers` aux enfants : poser le marqueur sur chaque entité d'une hiérarchie (et sur les lumières).

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::{BACKGROUND_INDEX, FOREGROUND_INDEX, OVERLAY_INDEX, WORLD_INDEX};

/// Contenu de l'arrière-plan : ciel, étoiles, planètes (couche 0).
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(RenderLayers = RenderLayers::layer(BACKGROUND_INDEX))]
pub struct BackgroundLayer;

/// Contenu du monde : ville, route, voiture, trafic (couche 1).
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(RenderLayers = RenderLayers::layer(WORLD_INDEX))]
pub struct WorldLayer;

/// Contenu de l'avant-plan 3D, en repère caméra : logo du DJ, particules devant l'objectif (couche 2).
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(RenderLayers = RenderLayers::layer(FOREGROUND_INDEX))]
pub struct ForegroundLayer;

/// Contenu de la surcouche 2D : cartons-titres, nom du DJ (couche 3).
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(RenderLayers = RenderLayers::layer(OVERLAY_INDEX))]
pub struct OverlayLayer;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layers::Layer;

    #[test]
    fn un_marqueur_pose_les_render_layers_de_sa_couche() {
        let mut world = World::new();
        let cases = [
            (world.spawn(BackgroundLayer).id(), Layer::Background),
            (world.spawn(WorldLayer).id(), Layer::World),
            (world.spawn(ForegroundLayer).id(), Layer::Foreground),
            (world.spawn(OverlayLayer).id(), Layer::Overlay),
        ];
        for (entity, layer) in cases {
            assert_eq!(world.get::<RenderLayers>(entity), Some(&layer.render_layers()));
        }
    }
}
