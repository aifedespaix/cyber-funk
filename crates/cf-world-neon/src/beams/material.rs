//! Le matériau des projecteurs (`ShaderMaterial` de `beams.ts`) et son shader WESL (`assets/shaders/cf-world-neon/beam.wesl`),
//! embarqué dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
//!
//! Additif, double face, sans écriture de profondeur (`depthWrite: false` de Sillage), sans lumière de la scène ni ombre.
//! Deux instances servent : une pour tous les cônes, une pour toutes les taches au sol ; le type est un champ de l'uniforme.

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

const BEAM: Handle<Shader> = uuid_handle!("6d1c8a30-7b34-4e59-8f0a-2c9e5d7b1a50");
const BEAM_SRC: &str = include_str!("../../../../assets/shaders/cf-world-neon/beam.wesl");
const BEAM_PATH: &str = "cf-world-neon/beam.wesl";

/// Enregistre le shader des projecteurs dans `Assets<Shader>`.
pub(crate) fn embed_shaders(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(BEAM.id(), Shader::from_wesl(BEAM_SRC, BEAM_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, les projecteurs resteraient invisibles plutôt que de
        // couper le show.
        tracing::error!(%error, BEAM_PATH, "shader des projecteurs non enregistré");
    }
}

/// Le type de pièce que dessine un [`BeamMaterial`] (champ `y` de l'uniforme).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamKind {
    /// Le faisceau : un cône évasé.
    Cone = 0,
    /// La tache posée sur la route.
    Pool = 1,
}

/// Uniformes d'un projecteur. Même ordre que la structure `Beam` de `beam.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct BeamParams {
    /// `x` : niveau d'éclat accordé, `y` : type ([`BeamKind`]).
    pub p: Vec4,
}

impl BeamParams {
    /// Les uniformes d'une pièce de type `kind` au niveau `level`.
    #[must_use]
    pub fn new(kind: BeamKind, level: f32) -> Self {
        Self {
            p: Vec4::new(level, f32::from(kind as u8), 0.0, 0.0),
        }
    }
}

/// Lumière additive d'un projecteur : cône ou tache au sol.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct BeamMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: BeamParams,
}

impl Material for BeamMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(BEAM)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(BEAM)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

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
        // `DoubleSide` : un cône se voit de dedans comme de dehors ; la tache, de dessus comme de dessous.
        descriptor.primitive.cull_mode = None;
        // `depthWrite: false` : une lumière additive ne masque rien derrière elle.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}
