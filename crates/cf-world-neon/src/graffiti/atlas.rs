//! Les atlas des graffitis, cuits une fois à l'ouverture (règle 8 de `CLAUDE.md`) :
//!
//! - **mural** : une cellule RGBA de 512 × 128 par motif mural, en couleur (teintes du mot, liseré sombre), alpha
//!   prémultiplié, sRVB ; la cellule `k − 1` est le motif `k` du tag de façade ;
//! - **hologramme** : deux cellules d'un canal de 1 024 × 256 par mot holographique, le masque net et sa version floue
//!   (le halo, précuit : pas de second passage plein écran). La couleur vient de la palette, au rendu.
//!
//! Chaque atlas est une texture en **tableau de couches** (une seule `Handle<Image>`) : si les cellules dépassent une
//! couche (4 096 px de haut au plus), la suivante prend la relève ; une cellule dit sa couche.
//!
//! **Alignement** : un mot sauté (police absente, sans ligne de licence, signe manquant) est retiré **avant** de
//! construire la [`GraffitiLibrary`] ; la bibliothèque et l'atlas comptent donc les mêmes mots dans le même ordre, et un
//! index de tag ou de projecteur désigne toujours la bonne cellule. Rien ici ne touche à Bevy : `bake` envoie les octets
//! au GPU.

use std::collections::HashMap;
use std::path::Path;

use tiny_skia::{Color, FillRule, Paint, Pixmap, Transform};

use super::fonts::{LoadedFont, Shape, load_font};
use super::raster::{Mask, Strokes, paint_wall, styled};
use super::{GraffitiWord, Placement, licensed_fonts, load_placements, load_words};
use crate::scenery::graffiti_plan::GraffitiLibrary;

/// Cellule murale, en pixels.
const WALL_CELL: (u32, u32) = (512, 128);
/// Marge autour des lettres murales : liseré et extrusion.
const WALL_PAD: u32 = 12;
const WALL_STROKES: Strokes = Strokes {
    rim: 2,
    ring: 5,
    depth: 7,
};
/// Largeur et hauteur maximale d'une couche murale.
const WALL_PAGE: (u32, u32) = (2048, 2048);

/// Cellule holographique, en pixels.
const HOLO_CELL: (u32, u32) = (1024, 256);
/// Rayon du flou du halo (trois passes : il s'étale sur environ trois rayons, d'où la marge).
const HALO_R: usize = 8;
const HOLO_PAD: u32 = 3 * HALO_R as u32;
const HOLO_STROKES: Strokes = Strokes {
    rim: 0,
    ring: 8,
    depth: 10,
};
/// Largeur et hauteur maximale d'une couche holographique.
const HOLO_PAGE: (u32, u32) = (4096, 4096);

/// Où lire un motif : le rectangle (lettres et marge) dans une couche de l'atlas, en pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellRect {
    /// Couche du tableau.
    pub layer: u16,
    /// Coin haut gauche.
    pub x: u16,
    /// Coin haut gauche.
    pub y: u16,
    /// Largeur.
    pub w: u16,
    /// Hauteur.
    pub h: u16,
    /// Part de la hauteur occupée par les lettres : le panneau mesure `hauteur des lettres / letters`.
    pub letters: f32,
}

/// Un mot holographique : le masque net et le halo, de même taille et même disposition (même UV locale).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoloCell {
    /// Le masque net.
    pub sharp: CellRect,
    /// Sa version floue.
    pub halo: CellRect,
}

/// Format des pixels d'un atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerFormat {
    /// Couleur et alpha (prémultiplié), sRVB.
    Rgba,
    /// Un canal de couverture.
    R8,
}

impl LayerFormat {
    const fn bpp(self) -> usize {
        match self {
            Self::Rgba => 4,
            Self::R8 => 1,
        }
    }
}

/// Les couches d'un atlas, prêtes à partir au GPU (`count` couches de `w × h`, à la suite).
#[derive(Debug, Clone)]
pub struct Layers {
    /// Format des pixels.
    pub format: LayerFormat,
    /// Largeur d'une couche.
    pub w: u32,
    /// Hauteur d'une couche.
    pub h: u32,
    /// Nombre de couches (au moins une, même vide : une texture n'est jamais de taille nulle).
    pub count: u32,
    /// Les pixels.
    pub data: Vec<u8>,
}

impl Layers {
    /// Range des cellules de `cell` pixels, en grille, sur des couches de `page` pixels au plus. Rend aussi la place de
    /// chacune (couche, x, y).
    fn pack(
        format: LayerFormat,
        cell: (u32, u32),
        page: (u32, u32),
        cells: &[Vec<u8>],
    ) -> (Self, Vec<(u16, u32, u32)>) {
        let bpp = format.bpp();
        let total = cells.len() as u32;
        if total == 0 {
            let data = vec![0; bpp];
            return (
                Self {
                    format,
                    w: 1,
                    h: 1,
                    count: 1,
                    data,
                },
                Vec::new(),
            );
        }
        let cols = (page.0 / cell.0).max(1);
        let rows_max = (page.1 / cell.1).max(1);
        let per_layer = cols * rows_max;
        let count = total.div_ceil(per_layer);
        // Une seule couche : juste les rangées utiles ; plusieurs : toutes pleines hauteur (même taille pour le tableau).
        let rows = if count == 1 { total.div_ceil(cols) } else { rows_max };
        let (width, height) = (cols * cell.0, rows * cell.1);
        let layer_bytes = (width * height) as usize * bpp;
        let mut data = vec![0; layer_bytes * count as usize];
        let mut places = Vec::with_capacity(cells.len());
        let row_bytes = cell.0 as usize * bpp;
        for (i, pixels) in cells.iter().enumerate() {
            let i = i as u32;
            let (layer, slot) = (i / per_layer, i % per_layer);
            let (x, y) = ((slot % cols) * cell.0, (slot / cols) * cell.1);
            for row in 0..cell.1 as usize {
                let from = row * row_bytes;
                let to = layer as usize * layer_bytes + ((y as usize + row) * width as usize + x as usize) * bpp;
                if let (Some(src), Some(dst)) = (pixels.get(from..from + row_bytes), data.get_mut(to..to + row_bytes)) {
                    dst.copy_from_slice(src);
                }
            }
            places.push((layer as u16, x, y));
        }
        let layers = Self {
            format,
            w: width,
            h: height,
            count,
            data,
        };
        (layers, places)
    }

    /// Les pixels d'un rectangle, rangée par rangée (tests, diagnostics).
    #[must_use]
    pub fn crop(&self, r: CellRect) -> Vec<u8> {
        let bpp = self.format.bpp();
        let layer_bytes = (self.w * self.h) as usize * bpp;
        let row_bytes = usize::from(r.w) * bpp;
        let mut out = Vec::with_capacity(row_bytes * usize::from(r.h));
        for row in 0..usize::from(r.h) {
            let at = usize::from(r.layer) * layer_bytes
                + ((usize::from(r.y) + row) * self.w as usize + usize::from(r.x)) * bpp;
            out.extend_from_slice(self.data.get(at..at + row_bytes).unwrap_or_default());
        }
        out
    }
}

/// L'index des cellules, dans l'ordre de la [`GraffitiLibrary`].
#[derive(Debug, Clone, Default)]
pub struct AtlasCells {
    wall: Vec<CellRect>,
    holo: Vec<HoloCell>,
}

impl AtlasCells {
    /// La cellule du motif mural `index` (celui du tag : `1..=31`) ; `None` pour 0 ou un motif inconnu.
    #[must_use]
    pub fn wall_cell(&self, index: u8) -> Option<CellRect> {
        self.wall.get(usize::from(index).checked_sub(1)?).copied()
    }

    /// Les cellules du mot holographique `word` (rang dans [`GraffitiLibrary::holo_words`]).
    #[must_use]
    pub fn holo_cell(&self, word: u16) -> Option<HoloCell> {
        self.holo.get(usize::from(word)).copied()
    }
}

/// Le résultat de la cuisson : la bibliothèque (seulement les mots cuits), les deux atlas et ce qui a été sauté.
#[derive(Debug, Clone)]
pub struct BakedGraffiti {
    /// Les mots gardés et les placements valides, alignés sur les cellules.
    pub library: GraffitiLibrary,
    /// L'atlas mural.
    pub wall: Layers,
    /// L'atlas holographique.
    pub holo: Layers,
    /// L'index des cellules.
    pub cells: AtlasCells,
    /// Mots sautés, avec la raison.
    pub skipped: Vec<String>,
    /// Avertissements des fichiers (entrées ignorées, placements invalides…).
    pub warnings: Vec<String>,
}

impl BakedGraffiti {
    /// Mémoire des deux atlas, en octets.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.wall.data.len() + self.holo.data.len()
    }
}

/// Lit `graffiti/words.ron`, `graffiti/placements.ron` et `fonts/LICENCES.md` dans `assets_dir`, puis cuit. Un fichier
/// absent est un avertissement, jamais une erreur : sans rien, la ville est celle d'avant les graffitis.
#[must_use]
pub fn bake_assets(assets_dir: &Path) -> BakedGraffiti {
    let mut warnings = Vec::new();
    let mut read = |rel: &str| {
        std::fs::read_to_string(assets_dir.join(rel)).unwrap_or_else(|e| {
            warnings.push(format!("{rel} illisible ({e})"));
            String::new()
        })
    };
    let (words_ron, placements_ron, licences) = (
        read("graffiti/words.ron"),
        read("graffiti/placements.ron"),
        read("fonts/LICENCES.md"),
    );
    let (words, w) = load_words(&words_ron);
    let (placements, p) = load_placements(&placements_ron);
    warnings.extend(w.into_iter().chain(p));
    let mut baked = bake(&words, &placements, &licences, &assets_dir.join("fonts"));
    warnings.append(&mut baked.warnings);
    baked.warnings = warnings;
    baked
}

/// Cuit les deux atlas : écarte les mots dont la police manque, n'est pas licenciée ou n'a pas tous les signes,
/// construit la bibliothèque avec les autres, puis dessine une cellule par mot gardé, dans l'ordre de la bibliothèque.
#[must_use]
pub fn bake(words: &[GraffitiWord], placements: &[Placement], licences_md: &str, fonts_dir: &Path) -> BakedGraffiti {
    let licensed = licensed_fonts(licences_md);
    let mut fonts: HashMap<&str, Result<LoadedFont, String>> = HashMap::new();
    let mut kept: Vec<(GraffitiWord, Shape)> = Vec::new();
    let mut skipped = Vec::new();
    for word in words.iter().filter(|w| w.wall || w.holo) {
        let shape = if licensed.contains(&word.font) {
            match fonts
                .entry(word.font.as_str())
                .or_insert_with(|| load_font(fonts_dir, &word.font))
            {
                Ok(font) => font.shape(&word.text),
                Err(why) => Err(format!("police {why}")),
            }
        } else {
            Err("police sans ligne de licence remplie dans assets/fonts/LICENCES.md".to_owned())
        };
        match shape {
            Ok(shape) => kept.push((word.clone(), shape)),
            Err(why) => skipped.push(format!("« {} » sauté (police « {} ») : {why}", word.text, word.font)),
        }
    }
    let kept_words: Vec<GraffitiWord> = kept.iter().map(|(w, _)| w.clone()).collect();
    let (library, warnings) = GraffitiLibrary::new(&kept_words, placements);

    // Même filtre et même ordre que `GraffitiLibrary::new`, tronqués comme elle : cellule i = mot i.
    let wall_words = kept.iter().filter(|(w, _)| w.wall).take(library.wall_words().len());
    let mut wall_pixels = Vec::new();
    let mut wall_rects = Vec::new();
    for (word, shape) in wall_words {
        let (body, rect) = body_mask(shape, WALL_CELL, WALL_PAD);
        wall_pixels.push(paint_wall(&body, word.style, &word.hues, WALL_STROKES));
        wall_rects.push(rect);
    }
    let holo_words = kept.iter().filter(|(w, _)| w.holo).take(library.holo_words().len());
    let mut holo_pixels = Vec::new();
    let mut holo_rects = Vec::new();
    for (word, shape) in holo_words {
        let (body, rect) = body_mask(shape, HOLO_CELL, HOLO_PAD);
        let sharp = styled(&body, word.style, HOLO_STROKES);
        holo_pixels.push(sharp.to_r8());
        holo_pixels.push(sharp.blur(HALO_R).to_r8());
        holo_rects.push(rect);
    }

    let (wall, wall_places) = Layers::pack(LayerFormat::Rgba, WALL_CELL, WALL_PAGE, &wall_pixels);
    let (holo, holo_places) = Layers::pack(LayerFormat::R8, HOLO_CELL, HOLO_PAGE, &holo_pixels);
    let place = |rect: CellRect, (layer, x, y): (u16, u32, u32)| CellRect {
        layer,
        x: rect.x + x as u16,
        y: rect.y + y as u16,
        ..rect
    };
    let cells = AtlasCells {
        wall: wall_rects.iter().zip(wall_places).map(|(r, p)| place(*r, p)).collect(),
        holo: holo_rects
            .iter()
            .zip(holo_places.chunks(2))
            .filter_map(|(r, pair)| match pair {
                [sharp, halo] => Some(HoloCell {
                    sharp: place(*r, *sharp),
                    halo: place(*r, *halo),
                }),
                _ => None,
            })
            .collect(),
    };
    BakedGraffiti {
        library,
        wall,
        holo,
        cells,
        skipped,
        warnings,
    }
}

/// Dessine les lettres de `shape` dans une cellule de `cell` pixels, à la plus grande échelle qui laisse `pad` pixels
/// de marge, centrées. Rend le masque et le rectangle utile (lettres + marge), relatif à la cellule.
fn body_mask(shape: &Shape, cell: (u32, u32), pad: u32) -> (Mask, CellRect) {
    let (cw, ch) = (cell.0 as f32, cell.1 as f32);
    let (avail_w, avail_h) = (cw - 2.0 * pad as f32, ch - 2.0 * pad as f32);
    let b = shape.bounds;
    let scale = (avail_w / b.width().max(1.0)).min(avail_h / b.height().max(1.0));
    let (lw, lh) = (b.width() * scale, b.height() * scale);
    let (lx, ly) = (pad as f32 + (avail_w - lw) / 2.0, pad as f32 + (avail_h - lh) / 2.0);
    let xf = Transform::from_row(scale, 0.0, 0.0, scale, lx - b.left() * scale, ly - b.top() * scale);
    let (w, h) = (cell.0 as usize, cell.1 as usize);
    let mut a = vec![0.0; w * h];
    if let Some(mut pixmap) = Pixmap::new(cell.0, cell.1) {
        let mut paint = Paint::default();
        paint.set_color(Color::WHITE);
        paint.anti_alias = true;
        for glyph in &shape.glyphs {
            pixmap.fill_path(glyph, &paint, FillRule::Winding, xf, None);
        }
        for (v, px) in a.iter_mut().zip(pixmap.data().as_chunks::<4>().0) {
            *v = f32::from(px[3]) / 255.0;
        }
    }
    let x0 = (lx - pad as f32).floor().max(0.0);
    let y0 = (ly - pad as f32).floor().max(0.0);
    let x1 = (lx + lw + pad as f32).ceil().min(cw);
    let y1 = (ly + lh + pad as f32).ceil().min(ch);
    let rect = CellRect {
        layer: 0,
        x: x0 as u16,
        y: y0 as u16,
        w: (x1 - x0) as u16,
        h: (y1 - y0) as u16,
        letters: lh / (y1 - y0).max(1.0),
    };
    (Mask { w, h, a }, rect)
}
