//! La teinte d'ambiance (`SectionTracker.mood` de `engine/sections.ts`) : elle glisse du cyan (calme) au violet
//! (groove) puis au corail (sommet) avec l'intensité du morceau.
//!
//! Sillage calculait l'intensité lissée dans `SectionTracker`. Ici elle vient déjà de `cf-analysis`
//! (`AudioFeatures::intensity`, 0 calme → 1 sommet, même formule) : ce module n'en garde que la couleur, qui suit la
//! palette à l'écran (elle bouge donc aussi pendant un fondu de thème, comme les `followed` de Sillage).

use bevy::prelude::Resource;

use super::color::LinearRgb;
use super::scene::{SceneColor, ScenePalette};

/// Intensité de départ de Sillage (« ambiance neutre » : le groove).
pub const NEUTRAL_INTENSITY: f32 = 0.4;

/// La teinte ambiante courante, à injecter dans la brume et les bords de route. Écrite par le seul système du module.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct SceneMood {
    /// Teinte (lumière linéaire).
    pub color: LinearRgb,
    /// Intensité 0..1 d'où elle vient.
    pub intensity: f32,
}

impl Default for SceneMood {
    fn default() -> Self {
        Self {
            color: LinearRgb::default(),
            intensity: NEUTRAL_INTENSITY,
        }
    }
}

/// La teinte d'ambiance : `azure` en dessous de 0,5 (jusqu'à `violet`), puis `violet` jusqu'à `rose`.
#[must_use]
pub fn mood_color(palette: &ScenePalette, intensity: f32) -> LinearRgb {
    let t = if intensity.is_finite() {
        intensity.clamp(0.0, 1.0)
    } else {
        NEUTRAL_INTENSITY
    };
    let calm = palette.get(SceneColor::Azure);
    let groove = palette.get(SceneColor::Violet);
    let peak = palette.get(SceneColor::Rose);
    if t < 0.5 {
        calm.lerp(groove, t * 2.0)
    } else {
        groove.lerp(peak, (t - 0.5) * 2.0)
    }
}

#[cfg(test)]
mod tests {
    use cf_show::model::PresetTheme;

    use super::*;
    use crate::palette::ThemeLibrary;

    #[test]
    fn calme_groove_sommet() {
        let library = ThemeLibrary::builtin().unwrap();
        let palette = library.preset(PresetTheme::Velvet);
        let bytes = |intensity| mood_color(palette, intensity).to_srgb8();
        assert_eq!(bytes(0.0), palette.get(SceneColor::Azure).to_srgb8());
        assert_eq!(bytes(0.5), palette.get(SceneColor::Violet).to_srgb8());
        assert_eq!(bytes(1.0), palette.get(SceneColor::Rose).to_srgb8());
    }

    #[test]
    fn continue_entre_les_sections() {
        let library = ThemeLibrary::builtin().unwrap();
        let palette = library.preset(PresetTheme::Velvet);
        let a = mood_color(palette, 0.499).to_srgb8();
        let b = mood_color(palette, 0.501).to_srgb8();
        assert!(a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 1));
    }

    #[test]
    fn intensite_hors_borne_ou_non_finie_reste_sure() {
        let library = ThemeLibrary::builtin().unwrap();
        let palette = library.preset(PresetTheme::Gold);
        assert_eq!(
            mood_color(palette, 7.0).to_srgb8(),
            palette.get(SceneColor::Rose).to_srgb8()
        );
        assert_eq!(mood_color(palette, f32::NAN), mood_color(palette, NEUTRAL_INTENSITY));
    }
}
