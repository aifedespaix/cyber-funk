//! Les matériaux des corps : surface d'astre, anneau, étoile filante.

use bevy::asset::{Asset, Handle};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use super::{PLANET, RING, STREAK, SkyParams, double_sided};

// ---------------------------------------------------------------------------------------------------------------------
// Planètes

/// Uniformes de la surface d'un astre.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct PlanetParams {
    /// `xyz` : direction du soleil hors champ (`uLight`).
    pub light: Vec4,
    /// Teinte de l'atmosphère (`uAtmo`).
    pub atmo: Vec4,
    /// `x` : pouls (`uPulse`), `y` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Surface d'un astre : texture, éclairage diurne, liseré de Fresnel et lueur au terminateur.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct PlanetMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: PlanetParams,
    /// La texture de surface (`uMap`), avec ses mips.
    #[texture(1)]
    #[sampler(2)]
    pub map: Option<Handle<Image>>,
}
sky_params!(PlanetMaterial, PlanetParams);

impl Material for PlanetMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PLANET)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Uniformes d'un anneau.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct RingParams {
    /// Teinte de l'anneau (`uTint`).
    pub tint: Vec4,
    /// `x` : rayon extérieur en m (`uOuter`), `y` : pouls (`uPulse`), `z` : fondu du ciel (`uReveal`).
    pub geo: Vec4,
}

/// Anneaux concentriques de densités variées, avec une division nette.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct RingMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: RingParams,
}
sky_params!(RingMaterial, RingParams);

impl Material for RingMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(RING)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(RING)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
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
        double_sided(descriptor);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Étoiles filantes

/// Uniformes d'un trait.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct StreakParams {
    /// Teinte (`uColor`).
    pub color: Vec4,
    /// `x` : opacité (`uAlpha`), `y` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Un trait à tête brillante qui glisse sur la voûte.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct StreakMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: StreakParams,
}
sky_params!(StreakMaterial, StreakParams);

impl Material for StreakMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(STREAK)
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
        double_sided(descriptor);
        Ok(())
    }
}
