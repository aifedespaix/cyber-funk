//! Post-process : HDR, bloom, étalonnage et effets du spectacle, puis tonemapping (portage de `engine/postfx.ts`,
//! `engine/postfx-grade.ts` et `engine/show-fx.ts` de Sillage).
//!
//! # La chaîne
//!
//! Sur la caméra du [`Compositor`](crate::Compositor), qui rend l'image composée en HDR (`Rgba16Float`) :
//!
//! ```text
//! couches 3D composées (HDR) → bloom (Bevy) → étalonnage cf_grade (WGSL) → tonemapping KhronosPbrNeutral → dither
//!                                                                                   ↓
//!                       surcouche 2D posée par l'OverlayCompositor, sans aucun effet → fenêtre
//! ```
//!
//! - **Bloom** : celui de Bevy, réglé d'après l'`UnrealBloomPass` de Sillage ([`bloom`]) ; il respire sur le kick.
//! - **Étalonnage** (`cf_grade`) : le shader de `postfx-grade.ts` porté en WGSL (`assets/shaders/cf-render/grade.wgsl`) :
//!   aberration chromatique (séparation RVB), gouttes sur l'objectif, flou radial et désaturation du bullet-time,
//!   contraste et saturation par chaleur, vignette, lignes de balayage, vague de lumière, flash, halation, rayons.
//! - **Persistance** (avant le bloom) et **grain** (après le tonemapping), voir [`persistence`] et [`finish`].
//! - **Tonemapping** : `Tonemapping::KhronosPbrNeutral`, le même opérateur que le `NeutralToneMapping` de three.js
//!   (« Khronos PBR Neutral »), exposition 1 comme dans Sillage.
//!
//! # Le flash ne passe que par le limiteur
//!
//! Les uniformes `flash` et `sweep` ne sont écrits qu'à partir de [`ShowFx::flash`] et [`ShowFx::sweep`], qui lisent le
//! [`FlashLimiter`](cf_show::limiter::FlashLimiter) de `cf-show` ; leurs couleurs ne sont que des
//! [`SafeColor`](cf_show::limiter::SafeColor). Aucun autre chemin n'écrit de lumière plein écran dans cette passe.
//!
//! # Qui écrit quoi
//!
//! - [`CurrentShowFx`] : le pont du spectacle (copie de [`ShowFxInput::from_show`] à chaque image). Absente : pas de
//!   spectacle, donc ni flash, ni vague, ni étalonnage de chaleur.
//! - [`PostFxSettings`] : la qualité adaptative (J4-06) — bloom allumé, diviseur du bloom, niveau.
//! - [`LensWeather`] : la météo du monde (averse, vitesse) pour les gouttes.
//! - [`PostFxState`] : le seul système du post-process ; il porte les effets du spectacle et leur limiteur.

mod bloom;
pub mod finish;
mod grade;
mod grain;
mod lens;
mod node;
mod persist_node;
pub mod persistence;
mod show_fx;
mod systems;

use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponentPlugin, UniformComponentPlugin};
use bevy::shader::Shader;

pub use bloom::{
    BLOOM_PULSE, BLOOM_STRENGTH, BLOOM_THRESHOLD, DEFAULT_BLOOM_DIVISOR, bloom_intensity, bloom_mip, bloom_settings,
    bloom_strength, bloom_tail,
};
pub use grade::GradeUniforms;
pub use grain::GrainUniforms;
pub use lens::Lens;
pub use node::{GRADE_PASS_LABEL, GRAIN_PASS_LABEL};
pub use persist_node::{PERSIST_PASS_LABEL, PersistUniforms};
pub use show_fx::{FlashPalette, ShowFx, ShowFxInput};

use crate::CfSet;

/// Réglages du post-process, écrits par la qualité adaptative (J4-06) ou l'opérateur.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PostFxSettings {
    /// Niveau de qualité adaptative, 0 = plein. Le flou radial du bullet-time se coupe dès le cran 2.
    pub quality: u8,
    /// Bloom allumé. Éteint, le composant est retiré de la caméra : ses passes ne coûtent plus rien.
    pub bloom: bool,
    /// Le bloom se calcule à 1/`bloom_divisor` de la hauteur de l'image : plus grand, moins cher et plus flou.
    pub bloom_divisor: u32,
    /// Gain du bloom, 0 → 1, appliqué à son intensité. La qualité adaptative l'estompe avant de retirer le bloom (et le
    /// rétablit après l'avoir remis) pour qu'aucun palier ne soit un saut de luminance.
    pub bloom_gain: f32,
    /// Densité de brume (0..1) : allonge la queue du bloom (DA §3.1).
    pub mist: f32,
}

impl Default for PostFxSettings {
    fn default() -> Self {
        Self {
            quality: 0,
            bloom: true,
            bloom_divisor: DEFAULT_BLOOM_DIVISOR,
            bloom_gain: 1.0,
            mist: 0.0,
        }
    }
}

/// La météo vue par l'objectif. Écrite par le monde ; au repos, objectif sec.
/// Compteur de coupes de plan, incrémenté par le réalisateur : chaque incrément vide l'historique de persistance.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShotCuts(pub u64);

#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct LensWeather {
    /// Averse, 0 → 1.
    pub rain: f32,
    /// Vitesse rapportée à la vitesse de croisière, 0 → 1 : les gouttes glissent plus vite.
    pub speed_ratio: f32,
}

/// L'extrait du spectacle de l'image pour les effets. Écrite par un seul système, le pont du spectacle.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Deref)]
pub struct CurrentShowFx(pub ShowFxInput);

/// État du post-process : effets du spectacle (et leur limiteur), gouttes. Écrit par le seul système du post-process.
#[derive(Resource, Debug, Clone)]
pub struct PostFxState {
    show_fx: ShowFx,
    lens: Lens,
    persistence: persistence::Persistence,
    transient: cf_analysis::filters::AttackRelease,
    peak_mix: f32,
    last_time: Option<f64>,
}

impl Default for PostFxState {
    fn default() -> Self {
        Self {
            show_fx: ShowFx::default(),
            lens: Lens::default(),
            persistence: persistence::Persistence::default(),
            // `env(TR ; 2 ms, 90 ms)` de l'aberration ciblée.
            transient: cf_analysis::filters::AttackRelease::new(0.002, 0.09),
            peak_mix: 0.0,
            last_time: None,
        }
    }
}

impl PostFxState {
    /// Les effets du spectacle (niveaux de flash et de vague du limiteur, étalonnage…).
    #[must_use]
    pub fn show_fx(&self) -> &ShowFx {
        &self.show_fx
    }

    /// Les gouttes sur l'objectif.
    #[must_use]
    pub fn lens(&self) -> Lens {
        self.lens
    }

    /// Change les couleurs de flash et de vague (changement de thème, J4-08).
    pub fn set_palette(&mut self, palette: FlashPalette) {
        self.show_fx.set_palette(palette);
    }
}

/// Installe le post-process sur le [`Compositor`](crate::Compositor) du [`LayersPlugin`](crate::LayersPlugin).
///
/// À ajouter après les plugins de rendu de Bevy (`DefaultPlugins`) : le shader est embarqué dans `Assets<Shader>` à la
/// construction. Sans monde de rendu (tests), seule la partie logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct PostFxPlugin;

impl Plugin for PostFxPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<GradeUniforms>::default(),
            UniformComponentPlugin::<GradeUniforms>::default(),
            ExtractComponentPlugin::<GrainUniforms>::default(),
            UniformComponentPlugin::<GrainUniforms>::default(),
            ExtractComponentPlugin::<PersistUniforms>::default(),
            UniformComponentPlugin::<PersistUniforms>::default(),
        ));
        if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
            let shader = Shader::from_wgsl(node::GRADE_SOURCE, "cf-render/grade.wgsl");
            let persist = Shader::from_wgsl(persist_node::PERSIST_SOURCE, "cf-render/persist.wgsl");
            if let Err(error) = shaders.insert(persist_node::PERSIST_SHADER.id(), persist) {
                tracing::error!(%error, "shader de persistance non enregistré");
            }
            let grain = Shader::from_wgsl(node::GRAIN_SOURCE, "cf-render/grain.wgsl");
            if let Err(error) = shaders.insert(node::GRAIN_SHADER.id(), grain) {
                tracing::error!(%error, "shader de grain non enregistré");
            }
            if let Err(error) = shaders.insert(node::GRADE_SHADER.id(), shader) {
                // Impossible avec un identifiant UUID ; si cela arrivait, l'image sortirait sans étalonnage (la passe
                // attend son pipeline) plutôt que de couper le show.
                tracing::error!(%error, "shader d'étalonnage non enregistré");
            }
        }
        node::build_render(app);
        persist_node::build_render(app);
        app.init_resource::<PostFxSettings>()
            .init_resource::<LensWeather>()
            .init_resource::<PostFxState>()
            .add_systems(
                Update,
                (systems::equip_compositor, systems::update_postfx)
                    .chain()
                    .in_set(CfSet::Present),
            );
    }
}
