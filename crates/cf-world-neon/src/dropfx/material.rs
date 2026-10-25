//! Les deux matériaux du paquet du drop (`fx.ts`) et leurs shaders WESL (`assets/shaders/cf-world-neon/dropfx_*.wesl`),
//! embarqués dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
//!
//! | Matériau | Rendu | Sillage |
//! | --- | --- | --- |
//! | [`DropRingMaterial`] | additif, double face ; un anneau au sol qui s'élargit | `MeshBasicMaterial` + `RingGeometry` |
//! | [`DropSparkMaterial`] | additif, carrés tournés vers la caméra, couleur par sommet | `PointsMaterial` |
//!
//! Aucun ne reçoit de lumière de la scène ni d'ombre, ni ne passe par le passage de profondeur ; aucun n'écrit la profondeur.

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

const RING: Handle<Shader> = uuid_handle!("6d1c8a40-7b34-4e59-8f0a-2c9e5d7b1a60");
const RING_SRC: &str = include_str!("../../../../assets/shaders/cf-world-neon/dropfx_ring.wesl");
const RING_PATH: &str = "cf-world-neon/dropfx_ring.wesl";
const SPARK: Handle<Shader> = uuid_handle!("6d1c8a41-7b34-4e59-8f0a-2c9e5d7b1a61");
const SPARK_SRC: &str = include_str!("../../../../assets/shaders/cf-world-neon/dropfx_spark.wesl");
const SPARK_PATH: &str = "cf-world-neon/dropfx_spark.wesl";

/// Demi-côté du quad d'une onde (m) : l'anneau de Sillage grandit jusqu'à 17 × 0,9 m de rayon, soit 15,3 m.
pub const RING_HALF_M: f32 = 16.0;

/// Enregistre les shaders du paquet du drop dans `Assets<Shader>`.
pub(crate) fn embed_shaders(shaders: &mut Assets<Shader>) {
    for (handle, source, path) in [(&RING, RING_SRC, RING_PATH), (&SPARK, SPARK_SRC, SPARK_PATH)] {
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path)) {
            // Impossible avec un identifiant UUID ; si cela arrivait, le paquet resterait invisible plutôt que de couper
            // le show.
            tracing::error!(%error, path, "shader du paquet du drop non enregistré");
        }
    }
}

/// Les deux faces sont dessinées, la profondeur n'est pas écrite (`depthWrite: false` de Sillage).
fn additive_double_sided(descriptor: &mut RenderPipelineDescriptor) {
    descriptor.primitive.cull_mode = None;
    if let Some(depth) = descriptor.depth_stencil.as_mut() {
        depth.depth_write_enabled = false.into();
    }
}

/// Uniformes d'une onde. Même ordre que la structure `Ring` de `dropfx_ring.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct RingParams {
    /// Teinte (RVB linéaire).
    pub tint: Vec4,
    /// `x` : niveau accordé, `y` : rayon du centre de l'anneau, `z` : demi-épaisseur (fractions du demi-côté du quad).
    pub p: Vec4,
}

impl RingParams {
    /// Les uniformes d'une onde de teinte `tint`, au niveau `level`, d'échelle `scale` (1 → 17, voir `Ring::scale`).
    #[must_use]
    pub fn new(tint: [f32; 3], level: f32, scale: f32) -> Self {
        // `RingGeometry(0.7, 0.9)` : centre à 0,8, demi-épaisseur 0,1, le tout multiplié par l'échelle.
        Self {
            tint: Vec4::new(tint[0], tint[1], tint[2], 1.0),
            p: Vec4::new(level, 0.8 * scale / RING_HALF_M, 0.1 * scale / RING_HALF_M, 0.0),
        }
    }
}

/// Onde de choc du drop : un anneau additif posé au sol.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct DropRingMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: RingParams,
}

impl Material for DropRingMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(RING)
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
        additive_double_sided(descriptor);
        Ok(())
    }
}

/// Gerbe d'étincelles du drop : carrés additifs tournés vers la caméra, couleur par sommet. Sans uniforme.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct DropSparkMaterial {}

impl Material for DropSparkMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(SPARK)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SPARK)
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
        additive_double_sided(descriptor);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l_anneau_de_depart_et_d_arrivee_tient_dans_le_quad() {
        for scale in [1.0, 17.0] {
            let p = RingParams::new([1.0; 3], 0.4, scale).p;
            assert!(
                p.y + p.z < 1.0,
                "bord extérieur {} du quad à l'échelle {scale}",
                p.y + p.z
            );
        }
        // À l'échelle 1 : centre 0,8 m, demi-épaisseur 0,1 m (`RingGeometry(0.7, 0.9)`).
        let p = RingParams::new([1.0; 3], 0.4, 1.0).p;
        assert!((p.y * RING_HALF_M - 0.8).abs() < 1e-5 && (p.z * RING_HALF_M - 0.1).abs() < 1e-5);
    }
}
