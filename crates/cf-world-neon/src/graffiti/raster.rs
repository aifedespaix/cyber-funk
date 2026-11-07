//! Les styles des graffitis, sur processeur, à partir du masque des lettres : contour par dilatation et érosion,
//! extrusion par copies décalées empilées, halo par flou. Rien de plus qu'un masque de flottants : le dessin des lettres
//! (`tiny-skia`) est fait une fois par `atlas`, tout le reste en découle.

use super::GraffitiStyle;
use crate::signs::paint::canvas::box_pass;

/// Épaisseurs d'un style, en pixels de cellule.
#[derive(Debug, Clone, Copy)]
pub(super) struct Strokes {
    /// Liseré sombre autour des lettres murales (0 : aucun).
    pub rim: usize,
    /// Épaisseur du trait du style `Outline`.
    pub ring: usize,
    /// Profondeur de l'extrusion (copies décalées d'un pixel en diagonale).
    pub depth: usize,
}

/// Un masque de couverture `w × h`, de 0 à 1.
#[derive(Debug, Clone)]
pub(super) struct Mask {
    pub w: usize,
    pub h: usize,
    pub a: Vec<f32>,
}

impl Mask {
    /// Filtre de rang séparable sur une fenêtre carrée de rayon `r` ; hors du masque, la couverture vaut 0.
    fn rank(&self, radius: usize, pick: fn(f32, f32) -> f32) -> Self {
        if radius == 0 {
            return self.clone();
        }
        let (w, h) = (self.w, self.h);
        let mut tmp = vec![0.0; self.a.len()];
        let mut out = vec![0.0; self.a.len()];
        let ri = radius.cast_signed();
        for y in 0..h {
            for x in 0..w {
                let mut v = self.a[y * w + x];
                for dx in -ri..=ri {
                    let sx = x.cast_signed() + dx;
                    let sample = if (0..w.cast_signed()).contains(&sx) {
                        self.a[y * w + sx as usize]
                    } else {
                        0.0
                    };
                    v = pick(v, sample);
                }
                tmp[y * w + x] = v;
            }
        }
        for y in 0..h {
            for x in 0..w {
                let mut v = tmp[y * w + x];
                for dy in -ri..=ri {
                    let sy = y.cast_signed() + dy;
                    let sample = if (0..h.cast_signed()).contains(&sy) {
                        tmp[sy as usize * w + x]
                    } else {
                        0.0
                    };
                    v = pick(v, sample);
                }
                out[y * w + x] = v;
            }
        }
        Self { w, h, a: out }
    }

    /// Les lettres grossies de `r` pixels.
    pub fn dilate(&self, r: usize) -> Self {
        self.rank(r, f32::max)
    }

    /// Les lettres amincies de `r` pixels.
    pub fn erode(&self, r: usize) -> Self {
        self.rank(r, f32::min)
    }

    /// Flou de boîte en trois passes (presque gaussien), rayon `r` pixels : le flou des enseignes.
    pub fn blur(&self, r: usize) -> Self {
        let mut a = self.a.clone();
        let mut tmp = vec![0.0; a.len()];
        for _ in 0..3 {
            box_pass(&a, &mut tmp, self.w, self.h, r, true);
            box_pass(&tmp, &mut a, self.w, self.h, r, false);
        }
        Self {
            w: self.w,
            h: self.h,
            a,
        }
    }

    /// L'extrusion : le maximum des copies décalées de 1 à `depth` pixels vers le bas et la droite.
    pub fn trail(&self, depth: usize) -> Self {
        let (w, h) = (self.w, self.h);
        let mut out = vec![0.0_f32; self.a.len()];
        for k in 1..=depth {
            for y in k..h {
                for x in k..w {
                    let i = y * w + x;
                    out[i] = out[i].max(self.a[i - k * w - k]);
                }
            }
        }
        Self { w, h, a: out }
    }

    /// `self − other`, borné à 0.
    pub fn minus(&self, other: &Self) -> Self {
        let a = self.a.iter().zip(&other.a).map(|(a, b)| (a - b).max(0.0)).collect();
        Self {
            w: self.w,
            h: self.h,
            a,
        }
    }

    /// `max(self, other × k)`.
    pub fn union(&self, other: &Self, k: f32) -> Self {
        let a = self.a.iter().zip(&other.a).map(|(a, b)| a.max(b * k)).collect();
        Self {
            w: self.w,
            h: self.h,
            a,
        }
    }

    /// Le masque en octets (un canal).
    pub fn to_r8(&self) -> Vec<u8> {
        self.a.iter().map(|v| to_byte(*v)).collect()
    }
}

fn to_byte(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Une teinte (degrés), une saturation et une valeur en RVB (`[0, 1]`).
fn hsv(hue_deg: f32, saturation: f32, value: f32) -> [f32; 3] {
    let sector = hue_deg.rem_euclid(360.0) / 60.0;
    let chroma = value * saturation;
    let second = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let rgb = match sector as u32 {
        0 => [chroma, second, 0.0],
        1 => [second, chroma, 0.0],
        2 => [0.0, chroma, second],
        3 => [0.0, second, chroma],
        4 => [second, 0.0, chroma],
        _ => [chroma, 0.0, second],
    };
    let floor = value - chroma;
    rgb.map(|channel| channel + floor)
}

/// Encre d'une couche : une couleur unie, ou un dégradé vertical (haut, bas).
#[derive(Clone, Copy)]
enum Paint {
    Solid([f32; 3]),
    Gradient([f32; 3], [f32; 3]),
}

/// Liseré des lettres murales : presque noir, ce qui détache le graffiti du mur.
const RIM: [f32; 3] = [0.03, 0.02, 0.05];

/// Le masque de la forme du style, sans couleur : ce que projette un hologramme.
pub(super) fn styled(body: &Mask, style: GraffitiStyle, s: Strokes) -> Mask {
    match style {
        GraffitiStyle::Fill => body.clone(),
        GraffitiStyle::Outline => body.minus(&body.erode(s.ring)),
        // La face extrudée, plus terne que la face avant : on lit encore le relief en monochrome.
        GraffitiStyle::Extrude => body.union(&body.trail(s.depth), 0.45),
    }
}

/// La cellule murale : couleurs des teintes du mot, liseré sombre, alpha **prémultiplié**, en sRVB. Rend des octets RGBA.
pub(super) fn paint_wall(body: &Mask, style: GraffitiStyle, hues: &[f32], strokes: Strokes) -> Vec<u8> {
    let h0 = hues.first().copied().unwrap_or(300.0);
    let h1 = hues.get(1).copied().unwrap_or(h0 + 40.0);
    let face = Paint::Gradient(hsv(h0, 0.8, 1.0), hsv(h1, 0.9, 0.8));
    let layers: Vec<(Mask, Paint)> = match style {
        GraffitiStyle::Fill => vec![(body.dilate(strokes.rim), Paint::Solid(RIM)), (body.clone(), face)],
        GraffitiStyle::Outline => {
            let ring = body.minus(&body.erode(strokes.ring));
            vec![(ring.dilate(strokes.rim), Paint::Solid(RIM)), (ring, face)]
        }
        GraffitiStyle::Extrude => {
            let trail = body.trail(strokes.depth);
            vec![
                (body.union(&trail, 1.0).dilate(strokes.rim), Paint::Solid(RIM)),
                (trail, Paint::Solid(hsv(h0, 0.9, 0.3))),
                (body.clone(), face),
            ]
        }
    };
    let (w, h) = (body.w, body.h);
    let mut out = vec![[0.0_f32; 4]; w * h];
    for (mask, paint) in &layers {
        for (i, px) in out.iter_mut().enumerate() {
            let a = mask.a[i];
            if a <= 0.0 {
                continue;
            }
            let color = match paint {
                Paint::Solid(solid) => *solid,
                Paint::Gradient(top, bottom) => {
                    let t = (i / w) as f32 / h.max(1) as f32;
                    [0, 1, 2].map(|k| top[k] + (bottom[k] - top[k]) * t)
                }
            };
            // « Par-dessus », en alpha prémultiplié.
            for k in 0..3 {
                px[k] = color[k] * a + px[k] * (1.0 - a);
            }
            px[3] = a + px[3] * (1.0 - a);
        }
    }
    out.iter().flat_map(|px| px.map(to_byte)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Mask {
        let (w, h) = (20, 20);
        let a = (0..w * h)
            .map(|i| {
                if (5..15).contains(&(i % w)) && (5..15).contains(&(i / w)) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect();
        Mask { w, h, a }
    }

    #[test]
    fn dilater_grossit_eroder_amincit_et_l_extrusion_suit_la_diagonale() {
        let m = square();
        let count = |m: &Mask| m.a.iter().filter(|v| **v > 0.5).count();
        assert_eq!(count(&m), 100);
        assert_eq!(count(&m.dilate(1)), 144);
        assert_eq!(count(&m.erode(1)), 64);
        let t = m.trail(3);
        assert!(t.a[17 * 20 + 17] > 0.5 && t.a[3 * 20 + 3] == 0.0);
    }

    #[test]
    fn le_contour_est_creux() {
        let ring = styled(
            &square(),
            GraffitiStyle::Outline,
            Strokes {
                rim: 0,
                ring: 2,
                depth: 0,
            },
        );
        assert_eq!(ring.a[10 * 20 + 10], 0.0, "le centre est vide");
        assert_eq!(ring.a[5 * 20 + 10], 1.0, "le bord est tracé");
    }

    #[test]
    fn la_peinture_murale_est_premultipliee() {
        let rgba = paint_wall(
            &square(),
            GraffitiStyle::Fill,
            &[120.0],
            Strokes {
                rim: 1,
                ring: 2,
                depth: 2,
            },
        );
        for px in rgba.chunks(4) {
            assert!(px[0] <= px[3] && px[1] <= px[3] && px[2] <= px[3], "{px:?}");
        }
        let center = &rgba[(10 * 20 + 10) * 4..(10 * 20 + 10) * 4 + 4];
        assert_eq!(center[3], 255);
        assert!(
            center[1] > center[0] && center[1] > center[2],
            "vert dominant : {center:?}"
        );
    }
}
