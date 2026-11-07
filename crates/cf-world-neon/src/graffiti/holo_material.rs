//! Le matériau d'un projecteur holographique : un shader WESL (`assets/shaders/cf-world-neon/graffiti_holo.wesl`), additif et
//! sans écriture de profondeur, pour la barre comme pour le texte. Voir le shader pour la règle 7 (limiteur).
//!
//! Un matériau par pièce du pool (douze projecteurs, deux pièces) : les uniformes sont réécrits en place, aucune allocation.
//! L'atlas holographique est partagé.

use bevy::asset::{Asset, Handle, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::Image;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};

/// Le shader des projecteurs, embarqué dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
pub(crate) const HOLO_SHADER: Handle<Shader> = uuid_handle!("6f3a9c52-1d84-4e07-b5a8-92c0e4d7f1b3");
/// Source du shader.
pub(crate) const HOLO_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/graffiti_holo.wesl");
/// Chemin logique du shader.
pub(crate) const HOLO_SHADER_PATH: &str = "cf-world-neon/graffiti_holo.wesl";

/// Les uniformes d'une pièce. Même ordre et mêmes types que la structure `Holo` de `graffiti_holo.wesl` : tout en `Vec4`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct HoloParams {
    /// `xyz` : teinte (lumière linéaire) ; `w` : 1 pour la barre, 0 pour le panneau du texte.
    pub tint: Vec4,
    /// `x` : apparition 0..1, `y` : niveau accordé par le limiteur de flashs 0..1, `z` : temps (s), `w` : graine 0..1.
    pub state: Vec4,
    /// `x` : part de la hauteur du panneau portée par la cellule (en haut), `y` : largeur de la barre / largeur du panneau,
    /// `z` : force du halo, `w` : force du faisceau.
    pub shape: Vec4,
    /// Rectangle du masque net dans l'atlas, en 0..1 : `(u0, v0, largeur, hauteur)` ; `w` : couche.
    pub sharp: Vec4,
    /// Idem pour le halo flou.
    pub halo: Vec4,
}

/// Le matériau d'une pièce de projecteur.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct HoloMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: HoloParams,
    /// L'atlas holographique (tableau de couches) ; `None` tant qu'il n'est pas cuit.
    #[texture(1, dimension = "2d_array")]
    #[sampler(2)]
    pub atlas: Option<Handle<Image>>,
}

impl Material for HoloMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(HOLO_SHADER)
    }

    /// Prémultiplié : `rgb + dst × (1 - a)`. Avec `a = 0`, c'est un mélange additif.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// Un hologramme n'occulte rien et ne projette pas d'ombre : aucun pipeline inutile.
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
        // Les deux faces : la barre se voit de tous côtés, le texte aussi quand la caméra passe derrière.
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Enregistre le shader dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut bevy::asset::Assets<Shader>) {
    if let Err(error) = shaders.insert(HOLO_SHADER.id(), Shader::from_wesl(HOLO_SOURCE, HOLO_SHADER_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, les projecteurs resteraient invisibles plutôt que de
        // couper le show.
        tracing::error!(%error, "shader des projecteurs holographiques non enregistré");
    }
}
