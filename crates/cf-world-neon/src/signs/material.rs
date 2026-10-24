//! Le matériau d'une enseigne : un plan qui montre une cellule de l'atlas, son bandeau défilant et le décor que le shader
//! dessine lui-même (cadre, trame, flux d'une caméra). Voir `assets/shaders/cf-world-neon/sign.wesl`.
//!
//! - **Un pipeline pour tout** : mélange « alpha prémultiplié » ; un rendu additif (hologramme, projection, néon) est un
//!   alpha nul. Les deux faces sont dessinées ; le shader écarte lui-même le dos d'un panneau à caisson. Un seul
//!   pipeline à préchauffer, quel que soit le format que prend une unité du pool.
//! - **Une couleur par canal** : les cellules sont des couvertures (voir `paint::canvas`) ; les couleurs viennent de la
//!   palette vivante à chaque image, fondus de thème compris.
//! - **Pas d'allocation** : les uniformes sont réécrits en place, deux textures (art, bandeaux) changent de page par
//!   simple échange de poignée.

use bevy::asset::{Asset, Handle, uuid_handle};
use bevy::math::Vec4;
use bevy::material::AlphaMode;
use bevy::pbr::Material;
use bevy::prelude::Image;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};

use super::paint::{BAND_Y, DIGIT_SLOT, DIGITS_H, DIGITS_X, DIGITS_Y, FEED, LABELS};

/// Le shader des enseignes, embarqué dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
pub(crate) const SIGN_SHADER: Handle<Shader> = uuid_handle!("6e2a9b41-3c58-4d17-b0a6-81f4d2c75e93");
/// Source du shader (`assets/shaders/cf-world-neon/sign.wesl`).
pub(crate) const SIGN_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/sign.wesl");
/// Chemin logique du shader.
pub(crate) const SIGN_SHADER_PATH: &str = "cf-world-neon/sign.wesl";

/// Les uniformes d'une enseigne. Même ordre et mêmes types que la structure `Sign` de `sign.wesl` : tout en `Vec4`.
///
/// Les couleurs sont en lumière linéaire (le `w` porte autre chose, voir chaque champ).
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct SignParams {
    /// `xy` : coin de la cellule dans la page d'art (px) ; `zw` : taille de la page (px).
    pub cell: Vec4,
    /// `x` : haut du bandeau dans sa page (px) ; `y` : largeur de la page de bandeaux ; `z` : longueur de la boucle (px) ;
    /// `w` : position de lecture (px).
    pub strip: Vec4,
    /// `x` : hauteur de la page de bandeaux (px) ; `y` : 1 s'il y a un bandeau ; `z` : 1 si la cellule n'a qu'un canal ;
    /// `w` : le rendu ([`LOOK_HOLO`] à [`LOOK_CAMERA`]).
    pub layout: Vec4,
    /// Accent de la publicité ; `w` : opacité (fondu, affichage).
    pub accent: Vec4,
    /// Glace.
    pub ice: Vec4,
    /// Couleur du canal `B` ; `w` : drapeaux de la cellule (`FLAG_*`).
    pub second: Vec4,
    /// Encre.
    pub ink: Vec4,
    /// Violet nuit (fond du flux vidéo) ; `w` : graine du panneau.
    pub velvet: Vec4,
    /// `x` : pouls limité (0..1) ; `y` : flash de ce panneau (radar, verdict), limité ; `z` : niveau du canal `R` soumis
    /// au limiteur (matraquage) ; `w` : horloge (s).
    pub state: Vec4,
    /// `x` : vitesse (km/h) ; `y` : numéro de la caméra (10..99) ; `z` : plan (0 large, 1 face, 2 profil) ; `w` : 1 si la
    /// caméra a un flux.
    pub feed: Vec4,
    /// `x` : 1 si les deux faces se voient (formats sans caisson) ; `y` : surcroît de lumière par flash ; `z` : 1 en mode
    /// calme ; `w` : étoile de la qualité (réservé).
    pub misc: Vec4,
}

/// `layout.w` : hologramme cadré, additif.
pub const LOOK_HOLO: f32 = 0.0;
/// `layout.w` : caisson opaque.
pub const LOOK_METAL: f32 = 1.0;
/// `layout.w` : projection, additive.
pub const LOOK_GHOST: f32 = 2.0;
/// `layout.w` : tubes de néon, additif.
pub const LOOK_NEON: f32 = 3.0;
/// `layout.w` : écran vidéo.
pub const LOOK_CAMERA: f32 = 4.0;

/// Le matériau d'une enseigne.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct SignMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: SignParams,
    /// La page de l'atlas qui porte la cellule.
    #[texture(1)]
    #[sampler(2)]
    pub art: Option<Handle<Image>>,
    /// La page de bandeaux qui porte le bandeau.
    #[texture(3)]
    #[sampler(4)]
    pub strip: Option<Handle<Image>>,
}

impl Material for SignMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Default
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SIGN_SHADER)
    }

    /// Prémultiplié : `rgb + dst × (1 - a)`. Avec `a = 0`, c'est le mélange additif des hologrammes.
    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// Une enseigne n'occulte rien et ne projette pas d'ombre : aucun pipeline inutile.
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
        // Les deux faces : le shader écarte le dos des panneaux à caisson (`misc.x`).
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Enregistre le shader dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut bevy::asset::Assets<Shader>) {
    if let Err(error) = shaders.insert(SIGN_SHADER.id(), Shader::from_wesl(SIGN_SOURCE, SIGN_SHADER_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, les enseignes resteraient invisibles plutôt que de
        // couper le show.
        tracing::error!(%error, "shader des enseignes non enregistré");
    }
}

/// Les constantes de mise en page que le shader recopie. Une dérive entre les deux fait lire les mauvais pixels : un test
/// compare ce tableau au texte du shader.
#[must_use]
pub fn shader_constants() -> Vec<(&'static str, f32)> {
    let (fx, fy, fw, fh) = FEED;
    let mut out = vec![
        ("FEED_X", fx),
        ("FEED_Y", fy),
        ("FEED_W", fw),
        ("FEED_H", fh),
        ("BAND_Y", BAND_Y),
        ("DIGITS_X", DIGITS_X),
        ("DIGITS_Y", DIGITS_Y),
        ("DIGITS_H", DIGITS_H),
        ("DIGIT_SLOT", DIGIT_SLOT),
    ];
    for (i, (_, x, w)) in LABELS.iter().enumerate() {
        out.push((["LABEL_X0", "LABEL_X1", "LABEL_X2"][i], *x));
        out.push((["LABEL_W0", "LABEL_W1", "LABEL_W2"][i], *w));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Chaque constante du shader porte la valeur de `paint` : le shader pioche les étiquettes et les chiffres aux
    /// endroits où la cuisson les a posés.
    #[test]
    fn le_shader_lit_la_cellule_la_ou_la_cuisson_l_a_ecrite() {
        for (name, value) in shader_constants() {
            let needle = format!("const {name}: f32 = {value:?};");
            let alt = format!("const {name}: f32 = {value:.1};");
            assert!(
                SIGN_SOURCE.contains(&needle) || SIGN_SOURCE.contains(&alt),
                "sign.wesl : `{needle}` introuvable"
            );
        }
    }
}
