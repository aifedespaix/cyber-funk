//! Les trois matériaux des astres (`meteor-materials.ts`) et leurs shaders WESL
//! (`assets/shaders/cf-world-neon/meteor_*.wesl`), embarqués dans le binaire : aucun fichier lu pendant un show
//! (règle 8 de `CLAUDE.md`).
//!
//! | Matériau | Rendu | Sillage |
//! | --- | --- | --- |
//! | [`MeteorSurfaceMaterial`] | opaque, texturé, vertex bosselé | `surfaceMaterial` |
//! | [`MeteorFxMaterial`] | additif, double face ; queue, halo, anneau de brume, éclair, onde | `trail/glow/splash/flash/shockMaterial` |
//! | [`MeteorDebrisMaterial`] | additif, carrés tournés vers la caméra, couleur par sommet | `PointsMaterial` de `meteor-blast.ts` |
//!
//! Les cinq lumières additives partagent un shader et un pipeline : le type de lumière est un champ de l'uniforme.
//! Aucun matériau ne reçoit de lumière de la scène ni d'ombre, ni ne passe par le passage de profondeur.

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

const SURFACE: Handle<Shader> = uuid_handle!("6d1c8a20-7b34-4e59-8f0a-2c9e5d7b1a40");
const SURFACE_SRC: &str = include_str!("../../../../assets/shaders/cf-world-neon/meteor_surface.wesl");
const SURFACE_PATH: &str = "cf-world-neon/meteor_surface.wesl";
const FX: Handle<Shader> = uuid_handle!("6d1c8a21-7b34-4e59-8f0a-2c9e5d7b1a41");
const FX_SRC: &str = include_str!("../../../../assets/shaders/cf-world-neon/meteor_fx.wesl");
const FX_PATH: &str = "cf-world-neon/meteor_fx.wesl";
const DEBRIS: Handle<Shader> = uuid_handle!("6d1c8a22-7b34-4e59-8f0a-2c9e5d7b1a42");
const DEBRIS_SRC: &str = include_str!("../../../../assets/shaders/cf-world-neon/meteor_debris.wesl");
const DEBRIS_PATH: &str = "cf-world-neon/meteor_debris.wesl";

/// Enregistre les shaders des astres dans `Assets<Shader>`.
pub(crate) fn embed_shaders(shaders: &mut Assets<Shader>) {
    for (handle, source, path) in [
        (&SURFACE, SURFACE_SRC, SURFACE_PATH),
        (&FX, FX_SRC, FX_PATH),
        (&DEBRIS, DEBRIS_SRC, DEBRIS_PATH),
    ] {
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path)) {
            // Impossible avec un identifiant UUID ; si cela arrivait, les astres resteraient invisibles plutôt que de
            // couper le show.
            tracing::error!(%error, path, "shader des astres non enregistré");
        }
    }
}

/// Les deux faces sont dessinées : un ruban ou un anneau se voit de n'importe quel côté (`DoubleSide` de Sillage).
fn double_sided(descriptor: &mut RenderPipelineDescriptor) {
    descriptor.primitive.cull_mode = None;
}

/// Uniformes de la surface d'un astre. Même ordre que la structure `Surface` de `meteor_surface.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct SurfaceParams {
    /// Teinte de chaleur (`uTint`).
    pub tint: Vec4,
    /// `xyz` : direction du vol, unitaire (`uDir`).
    pub dir: Vec4,
    /// `x` : graine du bosselage (`uSeed`).
    pub seed: Vec4,
}

/// Surface d'un astre : texture de planète, jour et nuit, face chauffée, liseré de Fresnel.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct MeteorSurfaceMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: SurfaceParams,
    /// La texture de surface (`uMap`).
    #[texture(1)]
    #[sampler(2)]
    pub map: Option<Handle<Image>>,
}

impl Material for MeteorSurfaceMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(SURFACE)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SURFACE)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Le type de lumière additive que dessine un [`MeteorFxMaterial`] (champ `kind` de l'uniforme).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FxKind {
    /// Queue : un ruban qui s'affine en s'éloignant de l'astre.
    Trail = 0,
    /// Halo de chaleur centré sur l'astre.
    Glow = 1,
    /// Anneau de brume à l'impact dans la nappe de brouillard.
    Splash = 2,
    /// Éclair de l'explosion : cœur blanc, halo teinté.
    Flash = 3,
    /// Onde de choc : un anneau qui s'élargit.
    Shock = 4,
}

/// Uniformes d'une lumière additive. Même ordre que la structure `Fx` de `meteor_fx.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct FxParams {
    /// Teinte de chaleur.
    pub tint: Vec4,
    /// `x` : niveau accordé (`uFade`, `uK`), `y` : rayon de l'onde (`uR`), `z` : type ([`FxKind`]).
    pub p: Vec4,
}

impl FxParams {
    /// Les uniformes d'une lumière de type `kind`, de teinte `tint` (linéaire), au niveau `level`, d'onde de rayon `r`.
    #[must_use]
    pub fn new(kind: FxKind, tint: [f32; 3], level: f32, r: f32) -> Self {
        Self {
            tint: Vec4::new(tint[0], tint[1], tint[2], 1.0),
            p: Vec4::new(level, r, f32::from(kind as u8), 0.0),
        }
    }
}

/// Lumière additive d'un astre ou de son explosion : queue, halo, anneau de brume, éclair, onde de choc.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct MeteorFxMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: FxParams,
}

impl Material for MeteorFxMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(FX)
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
        // `depthWrite: false` de Sillage : une lumière additive ne masque rien derrière elle.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}

/// Gerbe de débris : carrés additifs tournés vers la caméra, couleur par sommet. Sans uniforme.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct MeteorDebrisMaterial {}

impl Material for MeteorDebrisMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(DEBRIS)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(DEBRIS)
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
        // Les quatre sommets sont au même point : l'enroulement dépend de la vue, on ne cull rien.
        double_sided(descriptor);
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}
