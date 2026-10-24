//! Les matériaux du fond : dôme, étoiles, aurores.

use bevy::asset::Asset;
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;

use super::{AURORA, DOME, STARS, SkyParams, double_sided};
use crate::sky::stars::TINT_COUNT;

// ---------------------------------------------------------------------------------------------------------------------
// Dôme

/// Uniformes du dôme. Mêmes champs que la structure `Dome` de `sky_dome.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct DomeParams {
    /// Encre (`uInk`).
    pub ink: Vec4,
    /// Violet (`uViolet`).
    pub violet: Vec4,
    /// Azur (`uAzure`).
    pub azure: Vec4,
    /// Corail (`uRose`).
    pub rose: Vec4,
    /// `xyz` : axe de la Voie lactée (`uAxis`).
    pub axis: Vec4,
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Fond de l'espace : noir profond, nébuleuses diffuses et bande de Voie lactée.
///
/// Le shader lit la direction dans la position monde du fragment : l'entité reste à l'origine du repère du ciel (c'est la
/// caméra de fond qui se déplace, voir `cf_render::layers`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct DomeMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: DomeParams,
}
sky_params!(DomeMaterial, DomeParams);

impl Material for DomeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(DOME)
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
        // `depthWrite: false` de Sillage : le fond est derrière tout, il ne doit rien masquer.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Étoiles

/// Uniformes du champ d'étoiles.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct StarsParams {
    /// Les six teintes (glace, glace, azur, menthe, ambre, corail), que l'indice du sommet choisit.
    pub tints: [Vec4; TINT_COUNT],
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Champ d'étoiles : taille en pixels, scintillement, éclat additionnel sur les kicks.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct StarsMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: StarsParams,
}
sky_params!(StarsMaterial, StarsParams);

impl Material for StarsMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(STARS)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(STARS)
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
        // Les quads sont toujours tournés vers la caméra mais leur enroulement dépend du signe de `w` : on ne cull rien.
        double_sided(descriptor);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Aurore

/// Uniformes d'un rideau d'aurore.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct AuroraParams {
    /// Teinte du bas du rideau (`uA`).
    pub a: Vec4,
    /// Teinte du milieu (`uB`).
    pub b: Vec4,
    /// Teinte du haut (`uC`).
    pub c: Vec4,
    /// `x` : temps (s), `y` : pouls, `z` : graine du rideau (`uSeed`), `w` : mouvement (`uMotion`, 0,3 en mode calme).
    pub frame: Vec4,
    /// `x` : fondu du ciel (`uReveal`).
    pub look: Vec4,
}

/// Rideau d'aurore boréale : un plan vertical très lointain, en bruit animé.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct AuroraMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: AuroraParams,
}
sky_params!(AuroraMaterial, AuroraParams);

impl Material for AuroraMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(AURORA)
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
