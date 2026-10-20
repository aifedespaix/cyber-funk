//! Les réglages visuels de la route : palette de la scène, lueur, freinage, météo.

use bevy::color::{LinearRgba, Mix, Srgba};
use bevy::prelude::*;

/// Les couleurs de la scène que lit la route : celles de `palette.ts` de Sillage, thème « Void Velvet » par défaut
/// (`packages/tokens/src/scene.css`). En lumière linéaire, comme les `Color` de three.js.
///
/// Provisoire : le jalon J4-08 apporte la palette vivante (six thèmes, fondu calé sur un temps fort) ; la route lira
/// alors celle-là, et ce type disparaîtra.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadPalette {
    /// `--scene-ink` : noir mat, l'asphalte et la brume.
    pub ink: LinearRgba,
    /// `--scene-mint` (cyan électrique) : tirets, lueur sous la voiture.
    pub mint: LinearRgba,
    /// `--scene-azure` : tirets, ambiance calme.
    pub azure: LinearRgba,
    /// `--scene-violet` : ambiance groove, lueur de départ.
    pub violet: LinearRgba,
    /// `--scene-rose` (corail) : feux arrière, ambiance sommet.
    pub rose: LinearRgba,
    /// `--scene-ice` : reflets.
    pub ice: LinearRgba,
}

impl RoadPalette {
    /// Thème « Void Velvet », le thème de base de Sillage.
    #[must_use]
    pub fn velvet() -> Self {
        let hex = |value: u32| {
            let [_, r, g, b] = value.to_be_bytes();
            LinearRgba::from(Srgba::rgb_u8(r, g, b))
        };
        Self {
            ink: hex(0x05_0508),
            mint: hex(0x00_e5ff),
            azure: hex(0x12_a8d4),
            violet: hex(0x6a_4cc2),
            rose: hex(0xff_5733),
            ice: hex(0xe6_f7ff),
        }
    }

    /// Teinte d'ambiance pour une intensité de 0 (calme) à 1 (sommet) : de l'azur (calme) au violet (groove) puis au
    /// corail (sommet), sans à-coup (`mood` de `SectionTracker`, `sections.ts`). Mélange en lumière linéaire, comme
    /// `Color.lerp` de three.js.
    #[must_use]
    pub fn mood(&self, intensity: f32) -> LinearRgba {
        let t = intensity.clamp(0.0, 1.0);
        if t < 0.5 {
            self.azure.mix(&self.violet, t * 2.0)
        } else {
            self.violet.mix(&self.rose, (t - 0.5) * 2.0)
        }
    }
}

impl Default for RoadPalette {
    fn default() -> Self {
        Self::velvet()
    }
}

/// Ce que la route montre en dehors de la musique : écrite par les systèmes du monde (voiture, météo), lue par le
/// système qui remplit le matériau.
///
/// Le pouls et les basses, eux, viennent directement de `AudioFeatures` (`road.update(.., b.pulse, b.bass, ..)`).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RoadLook {
    /// Couleurs de la scène.
    pub palette: RoadPalette,
    /// Lueur sous la voiture (`car.glow`, violet au départ) : la couleur des feux et du liseré de la voiture (J5-05).
    pub glow: LinearRgba,
    /// Freinage, 0 à 1 : les feux rouges se reflètent plus fort dans la chaussée mouillée (J5-05).
    pub brake: f32,
    /// Pré-intro : part de la route dessinée depuis la voiture, 0 à 1 (1 : tout, sans effet).
    pub draw: f32,
    /// Humidité de la chaussée, 0 à 1 (`uWet`, J5-04).
    pub wet: f32,
    /// Intensité de la pluie qui tombe, 0 à 1 (`uRain`, J5-04) : gouttes sur la route, brume plus proche.
    pub rain: f32,
    /// Temps imposé au shader, en s (tests et captures de référence). `None` : l'horloge réelle.
    pub clock_s: Option<f64>,
}

impl Default for RoadLook {
    fn default() -> Self {
        let palette = RoadPalette::velvet();
        Self {
            glow: palette.violet,
            palette,
            brake: 0.0,
            draw: 1.0,
            wet: 0.0,
            rain: 0.0,
            clock_s: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_palette_velvet_est_celle_de_scene_css() {
        let p = RoadPalette::velvet();
        // `#00e5ff` : rouge nul, vert 0xe5 = 229/255 sRGB → 0,784 linéaire, bleu plein.
        assert!(p.mint.red.abs() < 1e-6 && (p.mint.blue - 1.0).abs() < 1e-6);
        assert!((p.mint.green - 0.783_538).abs() < 1e-4, "{}", p.mint.green);
        assert!((p.ink.red - 0.001_518).abs() < 1e-4, "{}", p.ink.red);
        assert!(p.rose.red > 0.99);
    }

    #[test]
    fn la_teinte_glisse_de_l_azur_au_violet_puis_au_corail() {
        let p = RoadPalette::velvet();
        assert_eq!(p.mood(0.0), p.azure);
        assert_eq!(p.mood(0.5), p.violet);
        assert_eq!(p.mood(1.0), p.rose);
        assert_eq!(p.mood(-3.0), p.azure, "bornée");
        // À mi-chemin du calme et du groove : le mélange à parts égales.
        let mid = p.mood(0.25);
        assert!((mid.red - (p.azure.red + p.violet.red) / 2.0).abs() < 1e-6);
    }
}
