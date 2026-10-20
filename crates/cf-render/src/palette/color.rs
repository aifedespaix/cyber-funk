//! Couleurs de la palette : stockage en lumière **linéaire** (comme les `Color` de three.js, qui linéarise les valeurs
//! sRGB lues dans `scene.css`), mélange linéaire, et passage par la teinte (HSL calculée en sRGB, comme
//! `getHSL(SRGBColorSpace)`).

// r, g, b, h, s, l, p, q, t : les noms de la conversion RVB/TSL, tels que dans three.js.
#![allow(clippy::many_single_char_names)]

use bevy::color::LinearRgba;

/// Couleur en lumière linéaire, composantes 0..1.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LinearRgb {
    /// Rouge.
    pub r: f32,
    /// Vert.
    pub g: f32,
    /// Bleu.
    pub b: f32,
}

/// Teinte, saturation, luminosité : `h`, `s`, `l` dans 0..1, calculées sur les valeurs **sRGB**.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Hsl {
    /// Teinte, 0..1 (un tour complet).
    pub h: f32,
    /// Saturation, 0..1.
    pub s: f32,
    /// Luminosité, 0..1.
    pub l: f32,
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Valeur d'un chiffre hexadécimal ASCII.
fn nibble(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).and_then(|d| u8::try_from(d).ok())
}

/// Les trois octets sRGB d'un `#rrggbb` (minuscules ou majuscules), `None` si le format est autre.
#[must_use]
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let bytes = text.as_bytes();
    if bytes.len() != 7 || bytes[0] != b'#' {
        return None;
    }
    let mut out = [0u8; 3];
    for (i, channel) in out.iter_mut().enumerate() {
        *channel = nibble(bytes[1 + 2 * i])? * 16 + nibble(bytes[2 + 2 * i])?;
    }
    Some(out)
}

/// Mélange deux couleurs sRGB 8 bits : 0 donne `a`, 1 donne `b` (`mixHex` de `settings.ts`, arrondi par canal).
#[must_use]
pub fn mix_srgb8(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    let mut out = [0u8; 3];
    for (i, channel) in out.iter_mut().enumerate() {
        let from = f64::from(a[i]);
        let to = f64::from(b[i]);
        // Arrondi vers le haut à la moitié, comme `Math.round` ; les valeurs sont positives.
        *channel = (from + (to - from) * t + 0.5).floor().clamp(0.0, 255.0) as u8;
    }
    out
}

impl LinearRgb {
    /// Depuis des octets sRGB.
    #[must_use]
    pub fn from_srgb8(bytes: [u8; 3]) -> Self {
        let c = |v: u8| srgb_to_linear(f32::from(v) / 255.0);
        Self {
            r: c(bytes[0]),
            g: c(bytes[1]),
            b: c(bytes[2]),
        }
    }

    /// Depuis un `#rrggbb`, `None` si le format est invalide.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        parse_hex(text).map(Self::from_srgb8)
    }

    /// Les octets sRGB, arrondis (pour comparer à `scene.css` et pour les journaux).
    #[must_use]
    pub fn to_srgb8(self) -> [u8; 3] {
        let c = |v: f32| (linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
        [c(self.r), c(self.g), c(self.b)]
    }

    /// Mélange linéaire : 0 donne `self`, 1 donne `other` (`Color.lerp` de three.js).
    #[must_use]
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
        }
    }

    /// Teinte, saturation et luminosité (calculées en sRGB, `getHSL(SRGBColorSpace)`).
    #[must_use]
    pub fn to_hsl(self) -> Hsl {
        let r = linear_to_srgb(self.r);
        let g = linear_to_srgb(self.g);
        let b = linear_to_srgb(self.b);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let l = min.midpoint(max);
        let delta = max - min;
        if delta < f32::EPSILON {
            return Hsl { h: 0.0, s: 0.0, l };
        }
        let s = if l <= 0.5 {
            delta / (max + min)
        } else {
            delta / (2.0 - max - min)
        };
        let h = if max <= r {
            (g - b) / delta + if g < b { 6.0 } else { 0.0 }
        } else if max <= g {
            (b - r) / delta + 2.0
        } else {
            (r - g) / delta + 4.0
        };
        Hsl { h: h / 6.0, s, l }
    }

    /// Depuis une teinte, saturation, luminosité sRGB (`setHSL(h, s, l, SRGBColorSpace)`).
    #[must_use]
    pub fn from_hsl(hsl: Hsl) -> Self {
        let h = hsl.h.rem_euclid(1.0);
        let (s, l) = (hsl.s.clamp(0.0, 1.0), hsl.l.clamp(0.0, 1.0));
        let p = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let q = 2.0 * l - p;
        let channel = |t: f32| {
            let t = t.rem_euclid(1.0);
            let v = if t < 1.0 / 6.0 {
                q + (p - q) * 6.0 * t
            } else if t < 0.5 {
                p
            } else if t < 2.0 / 3.0 {
                q + (p - q) * 6.0 * (2.0 / 3.0 - t)
            } else {
                q
            };
            srgb_to_linear(v)
        };
        Self {
            r: channel(h + 1.0 / 3.0),
            g: channel(h),
            b: channel(h - 1.0 / 3.0),
        }
    }

    /// Le même point pour le rendu Bevy.
    #[must_use]
    pub fn to_bevy(self) -> LinearRgba {
        LinearRgba::rgb(self.r, self.g, self.b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_aller_retour_sans_perte() {
        for hex in ["#050508", "#00e5ff", "#ff5733", "#ffffff", "#39ff6a"] {
            let bytes = parse_hex(hex).unwrap();
            assert_eq!(LinearRgb::from_srgb8(bytes).to_srgb8(), bytes, "{hex}");
        }
    }

    #[test]
    fn hex_invalide_refuse() {
        for bad in ["", "#fff", "00e5ff", "#00e5fg", "#00e5ff0"] {
            assert!(parse_hex(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn hsl_aller_retour() {
        for hex in ["#00e5ff", "#ff5733", "#6a4cc2", "#e6f7ff"] {
            let c = LinearRgb::from_hex(hex).unwrap();
            let back = LinearRgb::from_hsl(c.to_hsl());
            assert_eq!(back.to_srgb8(), c.to_srgb8(), "{hex}");
        }
    }

    #[test]
    fn mix_arrondit_comme_math_round() {
        // 0 + (255 - 0) * 0,5 = 127,5 -> 128 en JavaScript.
        assert_eq!(mix_srgb8([0, 0, 0], [255, 255, 255], 0.5), [128, 128, 128]);
        assert_eq!(mix_srgb8([10, 20, 30], [10, 20, 30], 0.3), [10, 20, 30]);
    }
}
