//! Uniformes de la passe de grain (`grain.wgsl`), portés par la caméra du compositeur.

use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;

use super::finish::GRAIN_CALM;

#[derive(Component, ExtractComponent, ShaderType, Debug, Clone, Copy, PartialEq)]
#[extract_app(RenderApp)]
pub struct GrainUniforms {
    /// Force `G` (0,06 au calme, 0,03 au sommet).
    pub gain: f32,
    /// Numéro de l'image de grain (24 par seconde).
    pub frame: u32,
    pub pad0: f32,
    pub pad1: f32,
}

impl Default for GrainUniforms {
    fn default() -> Self {
        Self {
            gain: GRAIN_CALM,
            frame: 0,
            pad0: 0.0,
            pad1: 0.0,
        }
    }
}
