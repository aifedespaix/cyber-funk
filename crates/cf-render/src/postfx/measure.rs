//! Outil de mesure sur capture (DA §1.1 et §3.1) : il sert de test aux chantiers C2, C3 et C4.
//!
//! Il lit une capture d'écran (RGBA sRGB 8 bits) et en tire :
//!
//! - `Ȳ`, la luminance relative moyenne **affichée** (`Y = 0.2126·R + 0.7152·G + 0.0722·B` en lumière linéaire) ;
//! - la part de pixels « émissifs » (`Y > 0.2`) et, parmi eux, la part dans la dominante ou l'accent (à moins de 25° de
//!   teinte), le critère de palette de DA §1.1 (au moins 70 %) ;
//! - la part de `ice` (`Y > 0.8`, moins de 2 % hors drop) ;
//! - un histogramme de teintes (12 cases de 30°) des pixels émissifs ;
//! - la part du bloom, par différence entre deux captures du même état, avec et sans bloom.
//!
//! Sans Bevy ni GPU : on l'appelle depuis un test ou depuis l'exemple `postfx_demo`.

/// Seuil de luminance d'un pixel émissif.
pub const EMISSIVE_Y: f32 = 0.2;
/// Seuil de luminance de `ice`.
pub const ICE_Y: f32 = 0.8;
/// Tolérance de teinte autour de la dominante et de l'accent (degrés).
pub const HUE_TOLERANCE: f32 = 25.0;
/// Cases de l'histogramme de teintes.
pub const HUE_BINS: usize = 12;

/// Teintes (degrés, 0..360) de la dominante et de l'accent de la scène.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaletteHues {
    pub dominant: f32,
    pub accent: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// `Ȳ`.
    pub mean_y: f32,
    /// Part des pixels émissifs (`Y > 0.2`).
    pub emissive_share: f32,
    /// Part des émissifs à moins de 25° de la dominante ou de l'accent (1 s'il n'y a aucun émissif).
    pub emissive_in_palette: f32,
    /// Part de l'image en `ice` (`Y > 0.8`).
    pub ice_share: f32,
    /// Part des émissifs dans chaque case de teinte de 30°.
    pub hue_histogram: [f32; HUE_BINS],
}

fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Luminance relative d'un pixel linéaire.
#[must_use]
pub fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

/// Teinte en degrés (0..360) d'un pixel ; `None` pour un gris (aucune teinte).
#[must_use]
pub fn hue_degrees(rgb: [f32; 3]) -> Option<f32> {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let d = max - min;
    if d < 1e-4 {
        return None;
    }
    let h = if (max - rgb[0]).abs() < f32::EPSILON {
        ((rgb[1] - rgb[2]) / d).rem_euclid(6.0)
    } else if (max - rgb[1]).abs() < f32::EPSILON {
        (rgb[2] - rgb[0]) / d + 2.0
    } else {
        (rgb[0] - rgb[1]) / d + 4.0
    };
    Some(h * 60.0)
}

/// Écart angulaire minimal entre deux teintes (degrés).
#[must_use]
pub fn hue_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d)
}

/// Mesure une capture RGBA 8 bits (`rgba.len() == 4·w·h`). `region` restreint la mesure à un rectangle
/// `[x0, x1) × [y0, y1)` (par exemple pour exclure une charte de couleurs de la scène de référence).
#[must_use]
pub fn measure(rgba: &[u8], width: u32, palette: PaletteHues, region: Option<((u32, u32), (u32, u32))>) -> Report {
    let width = width.max(1) as usize;
    let height = rgba.len() / 4 / width;
    let ((x0, y0), (x1, y1)) = region.unwrap_or(((0, 0), (width as u32, height as u32)));
    let (mut sum, mut n, mut emissive, mut in_palette, mut ice) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let mut bins = [0.0_f32; HUE_BINS];
    for y in y0 as usize..(y1 as usize).min(height) {
        for x in x0 as usize..(x1 as usize).min(width) {
            let i = 4 * (y * width + x);
            let rgb = [
                srgb_to_linear(rgba[i]),
                srgb_to_linear(rgba[i + 1]),
                srgb_to_linear(rgba[i + 2]),
            ];
            let luma = luminance(rgb);
            sum += f64::from(luma);
            n += 1.0;
            if luma > ICE_Y {
                ice += 1.0;
            }
            if luma > EMISSIVE_Y {
                emissive += 1.0;
                if let Some(hue) = hue_degrees(rgb) {
                    bins[((hue / 30.0) as usize).min(HUE_BINS - 1)] += 1.0;
                    if hue_distance(hue, palette.dominant) <= HUE_TOLERANCE
                        || hue_distance(hue, palette.accent) <= HUE_TOLERANCE
                    {
                        in_palette += 1.0;
                    }
                }
            }
        }
    }
    let n = n.max(1.0);
    let hist_total = bins.iter().sum::<f32>().max(1.0);
    Report {
        mean_y: (sum / n) as f32,
        emissive_share: (emissive / n) as f32,
        // Un pixel émissif sans teinte (blanc pur) ne compte pas dans la palette : il est de l'`ice`.
        emissive_in_palette: if emissive > 0.0 {
            (in_palette / emissive) as f32
        } else {
            1.0
        },
        ice_share: (ice / n) as f32,
        hue_histogram: bins.map(|b| b / hist_total),
    }
}

/// Part du bloom dans `Ȳ` : `(Ȳ avec − Ȳ sans) / Ȳ avec`, pour deux captures du même état.
#[must_use]
pub fn bloom_share(with_bloom: &Report, without_bloom: &Report) -> f32 {
    if with_bloom.mean_y <= 1e-6 {
        return 0.0;
    }
    ((with_bloom.mean_y - without_bloom.mean_y) / with_bloom.mean_y).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: PaletteHues = PaletteHues {
        dominant: 170.0,
        accent: 330.0,
    };

    fn uniform(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect()
    }

    #[test]
    fn une_image_noire_est_noire() {
        let r = measure(&uniform(8, 8, [0, 0, 0]), 8, PALETTE, None);
        assert_eq!(r.mean_y, 0.0);
        assert_eq!(r.emissive_share, 0.0);
        assert_eq!(r.ice_share, 0.0);
    }

    #[test]
    fn la_moyenne_est_en_lumiere_lineaire() {
        // sRGB 188 ≈ 0,5 linéaire ; gris : Y = valeur linéaire.
        let r = measure(&uniform(4, 4, [188, 188, 188]), 4, PALETTE, None);
        assert!((r.mean_y - 0.5).abs() < 0.01, "{}", r.mean_y);
    }

    #[test]
    fn la_teinte_et_la_distance_angulaire() {
        assert!((hue_degrees([1.0, 0.0, 0.0]).unwrap() - 0.0).abs() < 1e-3);
        assert!((hue_degrees([0.0, 1.0, 0.0]).unwrap() - 120.0).abs() < 1e-3);
        assert!((hue_degrees([0.0, 0.0, 1.0]).unwrap() - 240.0).abs() < 1e-3);
        assert!(hue_degrees([0.5, 0.5, 0.5]).is_none());
        assert!((hue_distance(350.0, 10.0) - 20.0).abs() < 1e-4);
    }

    #[test]
    fn la_part_de_palette_compte_les_emissifs_proches_de_la_dominante_ou_de_l_accent() {
        // Moitié cyan (170°, dans la palette), moitié vert (120° : 50° d'écart, hors palette).
        let (w, h) = (8_u32, 8_u32);
        let mut img = Vec::new();
        for i in 0..w * h {
            let px = if i % 2 == 0 {
                [0, 255, 200, 255]
            } else {
                [0, 255, 0, 255]
            };
            img.extend_from_slice(&px);
        }
        let r = measure(&img, w, PALETTE, None);
        assert!((r.emissive_share - 1.0).abs() < 1e-6);
        assert!((r.emissive_in_palette - 0.5).abs() < 1e-6, "{}", r.emissive_in_palette);
        assert!((r.hue_histogram.iter().sum::<f32>() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn l_ice_est_la_part_des_pixels_tres_clairs() {
        let mut img = uniform(10, 10, [10, 10, 20]);
        for px in img.chunks_mut(4).take(3) {
            px[..3].copy_from_slice(&[255, 255, 255]);
        }
        let r = measure(&img, 10, PALETTE, None);
        assert!((r.ice_share - 0.03).abs() < 1e-6);
    }

    #[test]
    fn la_region_exclut_ce_qui_est_hors_du_cadre_mesure() {
        let mut img = uniform(10, 10, [0, 0, 0]);
        for px in img.chunks_mut(4).take(10) {
            px[..3].copy_from_slice(&[255, 255, 255]);
        }
        let full = measure(&img, 10, PALETTE, None);
        let crop = measure(&img, 10, PALETTE, Some(((0, 1), (10, 10))));
        assert!(full.mean_y > 0.0 && crop.mean_y == 0.0);
    }

    #[test]
    fn la_part_du_bloom_est_l_ecart_relatif() {
        let with = Report {
            mean_y: 0.08,
            ..measure(&uniform(1, 1, [0, 0, 0]), 1, PALETTE, None)
        };
        let without = Report {
            mean_y: 0.06,
            ..with.clone()
        };
        assert!((bloom_share(&with, &without) - 0.25).abs() < 1e-6);
        assert_eq!(bloom_share(&without, &with), 0.0);
    }
}
