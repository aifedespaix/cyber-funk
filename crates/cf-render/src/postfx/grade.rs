//! Réglages de la passe d'étalonnage (les uniformes de `postfx-grade.ts`), portés par la caméra du compositeur.

use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;

/// Grain de pellicule (`uGrain` de Sillage), en lumière linéaire ajoutée crête à crête.
pub const GRAIN: f32 = 0.035;
/// Rapport largeur / hauteur tant que la taille de l'image n'est pas connue (`uAspect` de Sillage).
const DEFAULT_ASPECT: f32 = 1.78;

/// Uniformes de la passe d'étalonnage, un par caméra qui la porte (le [`Compositor`](crate::Compositor)).
///
/// Écrits une fois par image par le seul système du post-process ; extraits vers le monde de rendu par
/// `ExtractComponent`, téléversés par `UniformComponentPlugin`. L'ordre des champs est celui de la structure `Grade`
/// de `grade.wgsl`.
#[derive(Component, ExtractComponent, ShaderType, Debug, Clone, Copy, PartialEq)]
#[extract_app(RenderApp)]
pub struct GradeUniforms {
    /// Aberration chromatique (`uAberration`) : décalage des voies rouge et bleue, en fraction d'écran.
    pub aberration: f32,
    /// Partie fractionnaire de l'horloge (s), pour faire bouiller le grain (`fract(uTime)` de Sillage).
    pub time: f32,
    /// Amplitude du grain (`uGrain`).
    pub grain: f32,
    /// Quantité de gouttes sur l'objectif, 0 → 1 (`uLens`).
    pub lens: f32,
    /// Horloge des gouttes (`uRainTime`), s.
    pub rain_time: f32,
    /// Rapport largeur / hauteur de l'image (`uAspect`).
    pub aspect: f32,
    /// Lumière ajoutée par le flash, **sortie du limiteur** (`uFlash`).
    pub flash: f32,
    /// Intensité de la vague de lumière, **sortie du limiteur** (`uSweep`).
    pub sweep: f32,
    /// Position de la vague, en largeur d'écran (`uSweepPos`).
    pub sweep_pos: f32,
    /// Contraste de l'étalonnage, ≥ 1 (`uContrast`).
    pub contrast: f32,
    /// Saturation de l'étalonnage, ≥ 1 (`uSaturation`).
    pub saturation: f32,
    /// Bullet-time, 0 → 1 (`uSlow`).
    pub slow: f32,
    /// Couleur du flash, linéaire, sans rouge saturé (`uFlashColor`).
    pub flash_color: Vec3,
    /// Flou radial du bullet-time, 0 → 1 (`uBlur`).
    pub blur: f32,
    /// Couleur de la vague, linéaire, sans rouge saturé (`uSweepColor`).
    pub sweep_color: Vec3,
}

impl Default for GradeUniforms {
    /// Valeurs de départ de Sillage : image neutre, sans flash ni vague.
    fn default() -> Self {
        Self {
            aberration: 0.0,
            time: 0.0,
            grain: GRAIN,
            lens: 0.0,
            rain_time: 0.0,
            aspect: DEFAULT_ASPECT,
            flash: 0.0,
            sweep: 0.0,
            sweep_pos: -1.0,
            contrast: 1.0,
            saturation: 1.0,
            slow: 0.0,
            flash_color: Vec3::ZERO,
            blur: 0.0,
            sweep_color: Vec3::ZERO,
        }
    }
}
