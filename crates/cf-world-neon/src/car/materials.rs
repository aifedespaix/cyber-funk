//! Les deux matériaux sur mesure de la voiture : la peinture (extension de `StandardMaterial`) et les lumières additives
//! (volumes de phares, flaques, halos, fumée, traînée). Leurs shaders sont embarqués dans le binaire (règle 8 de
//! `CLAUDE.md` : rien ne se charge pendant un show).

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::image::Image;
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, Material, MaterialExtension, MaterialPipeline, MaterialPipelineKey, StandardMaterial,
};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

/// Shader des lumières additives, embarqué.
const LIGHT_SHADER: Handle<Shader> = uuid_handle!("3b7e5a14-6c2d-4f08-b9a1-2e4d8c7f1a60");
const LIGHT_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/car_light.wesl");
const LIGHT_PATH: &str = "cf-world-neon/car_light.wesl";
/// Shader de la peinture, embarqué.
const PAINT_SHADER: Handle<Shader> = uuid_handle!("9d41c6e8-0a5b-4e73-8f2c-7b1e3a5d9c04");
const PAINT_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/car_paint.wesl");
const PAINT_PATH: &str = "cf-world-neon/car_paint.wesl";

/// Enregistre les shaders de la voiture dans `Assets<Shader>`.
pub(crate) fn embed_shaders(shaders: &mut Assets<Shader>) {
    for (handle, source, path) in [
        (&LIGHT_SHADER, LIGHT_SOURCE, LIGHT_PATH),
        (&PAINT_SHADER, PAINT_SOURCE, PAINT_PATH),
    ] {
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path)) {
            // Impossible avec un identifiant UUID ; si cela arrivait, la voiture resterait sans ce shader plutôt que de
            // couper le show.
            tracing::error!(%error, path, "shader de la voiture non enregistré");
        }
    }
}

/// Le motif qu'un [`CarLightMaterial`] dessine (`params.x` du shader).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightKind {
    /// Volume lumineux d'un phare (cône).
    Beam = 0,
    /// Deux flaques de lumière côte à côte sur la route.
    Pool = 1,
    /// Halo autour d'une optique : un carré toujours face à la caméra.
    Halo = 2,
    /// Lueur ronde posée sur la route (sous la caisse, clignotants).
    Glow = 3,
    /// Fumée : disques flous, couleur et opacité par sommet.
    Smoke = 4,
    /// Bande de la traînée : la couleur du sommet porte l'opacité.
    Ribbon = 5,
}

/// Les uniformes du shader des lumières : tout en `Vec4`, la disposition mémoire ne dépend d'aucune règle d'alignement.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct LightParams {
    /// `rgb` : teinte (lumière linéaire, au-delà de 1 le bloom s'en empare) ; `w` : opacité (`uK`).
    pub color: Vec4,
    /// `x` : motif ([`LightKind`]), `y` : écart des lobes de la flaque (`uSplit`).
    pub params: Vec4,
}

/// Matériau additif des lumières de la voiture, sans éclairage ni ombre : comme les `ShaderMaterial` de Sillage.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq)]
pub struct CarLightMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: LightParams,
    /// Biais de profondeur (`polygonOffset` de Sillage) : les flaques passent au-dessus de la route sans scintiller.
    pub depth_bias: f32,
}

impl CarLightMaterial {
    /// Un matériau de ce motif, éteint.
    #[must_use]
    pub fn new(kind: LightKind, depth_bias: f32) -> Self {
        Self {
            params: LightParams {
                color: Vec4::ZERO,
                params: Vec4::new(kind as u32 as f32, 0.42, 0.0, 0.0),
            },
            depth_bias,
        }
    }

    /// Teinte et opacité de l'image.
    pub fn set(&mut self, color: bevy::color::LinearRgba, k: f32) {
        self.params.color = Vec4::new(color.red, color.green, color.blue, k);
    }
}

impl Material for CarLightMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(LIGHT_SHADER)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(LIGHT_SHADER)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn depth_bias(&self) -> f32 {
        self.depth_bias
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// Deux faces : les volumes se voient de l'intérieur, les bandes et les quadrilatères de n'importe quel côté
    /// (`DoubleSide` de Sillage).
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Les uniformes de la peinture.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct PaintParams {
    /// `x` : teinte visée (0 à 1), `y` : échelle de saturation, `z` : échelle de valeur, `w` : masque présent (1).
    pub paint: Vec4,
    /// `x` : rugosité, `y` : métal, `z` : paillettes, `w` : force du liseré.
    pub finish: Vec4,
    /// Couleur néon du liseré, côté gauche de l'écran (lumière linéaire).
    pub rim_a: Vec4,
    /// Couleur néon du liseré, côté droit de l'écran.
    pub rim_b: Vec4,
}

/// Extension de `StandardMaterial` : la peinture de la carrosserie (`CyberCarPaint.ts`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CarPaintExt {
    /// Uniformes envoyés au shader.
    #[uniform(100)]
    pub params: PaintParams,
    /// Masque de peinture (canal R) : seules ces zones sont repeintes.
    #[texture(101)]
    #[sampler(102)]
    pub mask: Option<Handle<Image>>,
}

impl MaterialExtension for CarPaintExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PAINT_SHADER)
    }
}

/// Le matériau de la carrosserie.
pub type CarPaintMaterial = ExtendedMaterial<StandardMaterial, CarPaintExt>;
