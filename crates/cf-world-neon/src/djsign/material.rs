//! Le matériau d'une lettre de l'enseigne du DJ : un plan qui montre sa cellule de l'atlas de glyphes, additif, sans
//! écriture de profondeur (`dj-sign-material.ts`). Voir `assets/shaders/cf-world-neon/djsign_letter.wesl`.
//!
//! Un matériau par lettre du pool (vingt au plus) : les uniformes sont réécrits en place chaque image, aucune
//! allocation. L'atlas est partagé par toutes les lettres.

use bevy::asset::{Asset, Handle, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::Material;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::Image;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

/// Le shader des lettres, embarqué dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
pub(crate) const LETTER_SHADER: Handle<Shader> = uuid_handle!("d1a5c0de-7e21-4b6f-9a30-5c8e1f2b4a67");
/// Source du shader (`assets/shaders/cf-world-neon/djsign_letter.wesl`).
pub(crate) const LETTER_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/djsign_letter.wesl");
/// Chemin logique du shader.
pub(crate) const LETTER_SHADER_PATH: &str = "cf-world-neon/djsign_letter.wesl";

/// Les uniformes d'une lettre. Même ordre et mêmes types que la structure `Letter` de `djsign_letter.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct LetterParams {
    /// Rectangle de la lettre dans l'atlas : `(u0, v0, u1, v1)`.
    pub rect: Vec4,
    /// Teinte du halo, en lumière linéaire ; `w` : lumière de la lettre (0 éteinte, 1 allumée).
    pub hue: Vec4,
}

/// Le matériau d'une lettre.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct LetterMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: LetterParams,
    /// L'atlas des glyphes.
    #[texture(1)]
    #[sampler(2)]
    pub atlas: Option<Handle<Image>>,
}

impl Material for LetterMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(LETTER_SHADER)
    }

    /// Prémultiplié : `rgb + dst × (1 - a)`. Avec `a = 0`, c'est un mélange additif.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// Une lettre n'occulte rien et ne projette pas d'ombre : aucun pipeline inutile.
    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Les deux faces : la lettre tombe en basculant, on la voit aussi de côté.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Enregistre le shader dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut bevy::asset::Assets<Shader>) {
    if let Err(error) = shaders.insert(LETTER_SHADER.id(), Shader::from_wesl(LETTER_SOURCE, LETTER_SHADER_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, l'enseigne resterait invisible plutôt que de couper le
        // show.
        tracing::error!(%error, "shader de l'enseigne du DJ non enregistré");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les champs du shader sont ceux de `LetterParams`, dans le même ordre : une dérive fait lire les mauvais octets.
    #[test]
    fn le_shader_declare_les_memes_champs_que_les_uniformes() {
        let rect = LETTER_SOURCE.find("rect: vec4<f32>").expect("rect");
        let hue = LETTER_SOURCE.find("hue: vec4<f32>").expect("hue");
        assert!(rect < hue);
        assert!(
            LETTER_SOURCE.contains("@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> letter: Letter;")
        );
        assert!(LETTER_SOURCE.contains("@binding(1) var atlas: texture_2d<f32>;"));
        assert!(LETTER_SOURCE.contains("@binding(2) var atlas_sampler: sampler;"));
    }
}
