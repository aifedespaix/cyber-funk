//! Teinte de la peinture : de la couleur du thème à l'échelle de teinte, de saturation et de valeur du shader
//! (`PaintSystem.setColor` de `CyberCarPaint.ts`).
//!
//! La peinture d'origine du modèle est violette (teinte 0,76, saturation 0,44, valeur 0,49, mesurées dans la texture) :
//! le shader déplace cette teinte vers celle visée et met saturation et valeur à l'échelle, le reste de la texture
//! (usure, reflets) est conservé.

use bevy::color::{LinearRgba, Srgba};

/// Teinte de la peinture d'origine (`PAINT_REF.h`).
pub const PAINT_REF_HUE: f32 = 0.76;
/// Saturation de la peinture d'origine (`PAINT_REF.s`).
pub const PAINT_REF_SAT: f32 = 0.44;
/// Valeur de la peinture d'origine (`PAINT_REF.v`).
pub const PAINT_REF_VAL: f32 = 0.49;

/// Teinte (0 à 1), saturation et valeur (0 à 1) d'un RVB.
fn rgb_to_hsv([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let mut h = 0.0;
    if d > 1e-6 {
        h = if (max - r).abs() < f32::EPSILON {
            ((g - b) / d + 6.0) % 6.0
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        h /= 6.0;
    }
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}

/// Réglage du shader pour peindre à `color` (lumière linéaire) : `(teinte visée, échelle de saturation, échelle de
/// valeur)`. Les échelles sont bornées à 2,2 comme dans Sillage.
#[must_use]
pub fn paint_scales(color: LinearRgba) -> (f32, f32, f32) {
    // Sillage convertit en sRGB (`convertLinearToSRGB`) avant de lire la teinte : le shader travaille en « gamma ».
    let srgb = Srgba::from(color);
    let [h, s, v] = rgb_to_hsv([srgb.red, srgb.green, srgb.blue]);
    (h, (s / PAINT_REF_SAT).clamp(0.0, 2.2), (v / PAINT_REF_VAL).clamp(0.0, 2.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_primaires_ont_leur_teinte() {
        assert!(rgb_to_hsv([1.0, 0.0, 0.0])[0].abs() < 1e-6);
        assert!((rgb_to_hsv([0.0, 1.0, 0.0])[0] - 1.0 / 3.0).abs() < 1e-6);
        assert!((rgb_to_hsv([0.0, 0.0, 1.0])[0] - 2.0 / 3.0).abs() < 1e-6);
        assert_eq!(rgb_to_hsv([0.0, 0.0, 0.0]), [0.0, 0.0, 0.0]);
    }

    /// Le violet du thème Velvet (`#6a4cc2`, teinte 0,71) est proche de la peinture d'origine (0,76) : le shader ne
    /// décale la texture que d'un peu moins d'un dixième de tour.
    #[test]
    fn le_violet_du_theme_velvet_ressemble_a_la_peinture_d_origine() {
        let violet = LinearRgba::from(Srgba::rgb_u8(0x6a, 0x4c, 0xc2));
        let (hue, sat, val) = paint_scales(violet);
        assert!((hue - PAINT_REF_HUE).abs() < 0.06, "teinte {hue}");
        assert!((0.8..2.0).contains(&sat), "saturation {sat}");
        assert!((0.8..2.2).contains(&val), "valeur {val}");
    }

    #[test]
    fn les_echelles_sont_bornees() {
        let (_, sat, val) = paint_scales(LinearRgba::rgb(1.0, 0.0, 0.0));
        // Saturation 1 / 0,44 = 2,27 : bornée à 2,2 ; valeur 1 / 0,49 = 2,04 : sous la borne.
        assert!((sat - 2.2).abs() < 1e-6, "{sat}");
        assert!((val - 1.0 / PAINT_REF_VAL).abs() < 1e-5, "{val}");
    }
}
