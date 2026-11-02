//! Teinte de carrosserie des motos et de leurs pilotes : une extension de `StandardMaterial` qui remplace la couleur de la
//! peinture par une couleur visée, en gardant la luminance de la texture, sans toucher au métal.
//!
//! La formule du shader (`rider_tint.wesl`) est reprise ici ([`tinted`]) pour être testée sans GPU.

use bevy::app::{App, Plugin};
use bevy::asset::{Asset, AssetApp, Assets, Handle, uuid_handle};
use bevy::color::{Hsva, LinearRgba};
use bevy::image::Image;
use bevy::math::Vec4;
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin, PbrPlugin, StandardMaterial};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};
use cf_render::WarmupAppExt;

/// Shader de la teinte, embarqué (règle 8 de `CLAUDE.md` : rien ne se compile pendant un show).
const TINT_SHADER: Handle<Shader> = uuid_handle!("c2a85f17-3d06-4b9e-a471-5e8d0f2b6c93");
const TINT_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/rider_tint.wesl");
const TINT_PATH: &str = "cf-world-neon/rider_tint.wesl";

/// Luminosité de la couleur visée : une carrosserie de moto est sombre, la couleur pleine serait un néon.
pub const DEFAULT_BRIGHTNESS: f32 = 0.45;
/// Luminance moyenne de la peinture d'origine du manul (mesurée par `tools/char/make_mask.py`).
pub const MANUL_REF_LUMINANCE: f32 = 0.0908;
/// Luminance moyenne des carrosseries grises des motos (textures `Carcass1`, `przod`, `cor_base02`, à ±0,03).
pub const MOTO_REF_LUMINANCE: f32 = 0.07;

/// Les uniformes de la teinte.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct TintParams {
    /// `rgb` : couleur visée (lumière linéaire) ; `w` : masque présent (1) ou non (0).
    pub tint: Vec4,
    /// `x` : luminance moyenne de la peinture d'origine ; `y` : luminosité de la couleur visée ; `z` : force (0 : rien).
    pub params: Vec4,
}

/// Extension de `StandardMaterial` : la teinte de la carrosserie.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct RiderTintExt {
    /// Uniformes envoyés au shader.
    #[uniform(100)]
    pub params: TintParams,
    /// Masque de peinture (canal R) : seules ces zones sont teintées. Sans masque, tout le matériau l'est.
    #[texture(101)]
    #[sampler(102)]
    pub mask: Option<Handle<Image>>,
}

impl RiderTintExt {
    /// Une teinte `color` sur une peinture de luminance moyenne `ref_luminance`, dans `mask` s'il y en a un.
    #[must_use]
    pub fn new(color: LinearRgba, ref_luminance: f32, mask: Option<Handle<Image>>) -> Self {
        Self {
            params: TintParams {
                tint: Vec4::new(
                    color.red,
                    color.green,
                    color.blue,
                    if mask.is_some() { 1.0 } else { 0.0 },
                ),
                params: Vec4::new(ref_luminance, DEFAULT_BRIGHTNESS, 1.0, 0.0),
            },
            mask,
        }
    }

    /// Change la couleur visée (une moto qui naît reprend le même matériau avec une autre teinte).
    pub fn set_color(&mut self, color: LinearRgba) {
        self.params.tint.x = color.red;
        self.params.tint.y = color.green;
        self.params.tint.z = color.blue;
    }
}

impl MaterialExtension for RiderTintExt {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(TINT_SHADER)
    }
}

/// Le matériau teinté d'une carrosserie.
pub type RiderPaintMaterial = ExtendedMaterial<StandardMaterial, RiderTintExt>;

/// Luminance relative d'une couleur linéaire (les mêmes poids que le shader).
#[must_use]
pub fn luminance(color: LinearRgba) -> f32 {
    0.2126 * color.red + 0.7152 * color.green + 0.0722 * color.blue
}

/// La couleur finale d'un pixel de peinture : ce que calcule `rider_tint.wesl`. `mask` : valeur du masque (1 hors masque
/// de texture), `strength` : force du réglage (1 : teinte pleine).
#[must_use]
pub fn tinted(
    base: LinearRgba,
    target: LinearRgba,
    ref_luminance: f32,
    brightness: f32,
    strength: f32,
    mask: f32,
) -> LinearRgba {
    let scale = (luminance(base) / ref_luminance.max(0.0001)).clamp(0.0, 2.5) * brightness;
    let amount = (strength * mask).clamp(0.0, 1.0);
    let mix = |b: f32, t: f32| b + (t * scale - b) * amount;
    LinearRgba::new(
        mix(base.red, target.red),
        mix(base.green, target.green),
        mix(base.blue, target.blue),
        base.alpha,
    )
}

/// Une couleur de carrosserie tirée d'une graine : la teinte parcourt le cercle, saturée et assez claire pour que la
/// teinte se voie une fois multipliée par la luminance sombre de la texture. Déterministe.
#[must_use]
pub fn variant_color(seed: u32) -> LinearRgba {
    // Nombre d'or : deux graines voisines donnent des teintes éloignées.
    let hue = (seed as f32 * 0.618_034).fract() * 360.0;
    let saturation = 0.65 + 0.3 * ((seed >> 3) % 4) as f32 / 3.0;
    LinearRgba::from(Hsva::new(hue, saturation, 1.0, 1.0))
}

/// Enregistre le shader de la teinte dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(TINT_SHADER.id(), Shader::from_wesl(TINT_SOURCE, TINT_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, les carrosseries resteraient à leur couleur d'origine.
        tracing::error!(%error, path = TINT_PATH, "shader de teinte non enregistré");
    }
}

/// Installe le matériau de teinte et son shader.
#[derive(Debug, Default, Clone, Copy)]
pub struct RiderTintPlugin;

impl Plugin for RiderTintPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<PbrPlugin>() {
            app.add_plugins(MaterialPlugin::<RiderPaintMaterial>::default())
                .warm_material::<RiderPaintMaterial>("motos et pilotes : teinte");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<RiderPaintMaterial>();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: LinearRgba = LinearRgba::new(0.07, 0.07, 0.07, 1.0);

    #[test]
    fn sans_force_la_couleur_d_origine_reste() {
        let out = tinted(GREY, LinearRgba::RED, MOTO_REF_LUMINANCE, DEFAULT_BRIGHTNESS, 0.0, 1.0);
        assert!((out.red - GREY.red).abs() < 1e-6 && (out.green - GREY.green).abs() < 1e-6);
    }

    #[test]
    fn hors_du_masque_la_couleur_d_origine_reste() {
        let out = tinted(GREY, LinearRgba::RED, MOTO_REF_LUMINANCE, DEFAULT_BRIGHTNESS, 1.0, 0.0);
        assert!((out.red - GREY.red).abs() < 1e-6 && (out.blue - GREY.blue).abs() < 1e-6);
    }

    #[test]
    fn une_carrosserie_grise_prend_la_teinte_visee() {
        let out = tinted(GREY, LinearRgba::RED, MOTO_REF_LUMINANCE, DEFAULT_BRIGHTNESS, 1.0, 1.0);
        assert!(
            out.red > 0.3 && out.green < 1e-6 && out.blue < 1e-6,
            "rouge attendu : {out:?}"
        );
    }

    /// La texture garde ses reliefs : un pixel deux fois plus clair reste deux fois plus clair après teinte.
    #[test]
    fn la_luminance_de_la_texture_est_conservee() {
        let dark = tinted(GREY, LinearRgba::BLUE, MOTO_REF_LUMINANCE, DEFAULT_BRIGHTNESS, 1.0, 1.0);
        let light = tinted(
            GREY * 2.0,
            LinearRgba::BLUE,
            MOTO_REF_LUMINANCE,
            DEFAULT_BRIGHTNESS,
            1.0,
            1.0,
        );
        assert!((light.blue / dark.blue - 2.0).abs() < 1e-3);
    }

    #[test]
    fn une_tache_tres_claire_ne_depasse_pas_la_borne() {
        let out = tinted(
            LinearRgba::WHITE,
            LinearRgba::WHITE,
            MOTO_REF_LUMINANCE,
            DEFAULT_BRIGHTNESS,
            1.0,
            1.0,
        );
        assert!(out.red <= 2.5 * DEFAULT_BRIGHTNESS + 1e-6);
    }

    #[test]
    fn les_couleurs_de_variante_sont_deterministes_et_variees() {
        assert_eq!(variant_color(7), variant_color(7));
        let colors: Vec<_> = (0..8).map(variant_color).collect();
        for (i, a) in colors.iter().enumerate() {
            for b in &colors[i + 1..] {
                assert_ne!(a, b, "deux graines différentes donnent la même couleur");
            }
        }
    }

    #[test]
    fn le_masque_present_est_signale_au_shader() {
        let with = RiderTintExt::new(LinearRgba::RED, 0.1, Some(Handle::default()));
        let without = RiderTintExt::new(LinearRgba::RED, 0.1, None);
        assert!((with.params.tint.w - 1.0).abs() < f32::EPSILON);
        assert!(without.params.tint.w.abs() < f32::EPSILON);
    }
}
