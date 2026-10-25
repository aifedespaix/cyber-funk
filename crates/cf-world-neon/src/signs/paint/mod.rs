//! La cuisson des textures 2D des enseignes (`painter.ts`, `art-*.ts`, `ticker.ts`).
//!
//! Sillage dessinait chaque enseigne dans un `<canvas>` repeint à chaque image. Ici tout est dessiné **une fois, à
//! l'ouverture**, sur CPU, avec `tiny-skia` (formes) et `cosmic-text` (texte) : jamais pendant un show (règle 8). Le
//! résultat est une cellule de 512 × 256 pixels en quatre canaux de couverture (voir [`canvas`]) ou, pour les
//! projections, un seul ; les bandeaux défilants sont des bandes d'une couleur, rejouées en boucle par le matériau.
//!
//! Le [`Painter`] ne connaît ni Bevy ni l'atlas : il rend des [`Bitmap`], que `atlas` range en pages.

// Code de dessin : les coordonnées x, y, w, h, les rayons et les couleurs sont des arguments naturels ; les tailles de
// canevas (au plus 4 096 px) sont très loin des limites des entiers signés.
#![allow(
    clippy::many_single_char_names,
    clippy::too_many_arguments,
    clippy::cast_possible_wrap,
    reason = "code de dessin : voir le commentaire ci-dessus"
)]

mod arts;
mod cam;
pub mod canvas;
mod icons;
mod neon;
mod street;
pub mod text;

pub use arts::Draw;
pub use cam::{BAND_Y, DIGIT_SLOT, DIGITS_H, DIGITS_X, DIGITS_Y, FEED, LABELS};
pub use neon::strip_aside;

use canvas::{Canvas, Ink, R};
use text::{Face, Fonts};

use super::formats::{CANVAS_H, CANVAS_W, TICKER_H};

/// Les pixels d'une cellule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pixels {
    /// Quatre canaux de couverture entrelacés (R, G, B, K).
    Rgba(Vec<u8>),
    /// Un seul canal.
    R8(Vec<u8>),
}

/// Une image cuite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    /// Largeur, en pixels.
    pub w: u32,
    /// Hauteur, en pixels.
    pub h: u32,
    /// Les pixels, `w × h` (par canal).
    pub pixels: Pixels,
}

/// Un bandeau défilant cuit : une bande d'une couleur, de largeur `period` (la boucle).
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    /// La bande (un canal), `ceil(period)` pixels de large sur [`TICKER_H`] de haut.
    pub bitmap: Bitmap,
    /// Longueur exacte de la boucle, en pixels (texte et intervalle compris).
    pub period: f32,
}

/// Intervalle entre deux passages du bandeau (`GAP` de `ticker.ts`).
const GAP: &str = "      ///      ";
/// Corps du texte du bandeau, en pixels (`FONT_SIZE`).
const TICKER_FONT: f32 = 24.0;
/// Largeur maximale d'une bande : la limite des textures de la plupart des GPU, avec de la marge.
pub const STRIP_MAX_W: f32 = 4000.0;
/// Nombre de slogans que le bandeau enchaîne au plus (`slice(0, 5)`).
pub const TICKER_SLOGANS: usize = 5;

/// Le peintre : les polices et un canevas réutilisé d'une cellule à l'autre.
pub struct Painter {
    fonts: Fonts,
    canvas: Canvas,
}

impl std::fmt::Debug for Painter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Painter").finish_non_exhaustive()
    }
}

impl Painter {
    /// Un peintre avec les polices du dépôt. `None` si le canevas ne peut pas être alloué.
    #[must_use]
    pub fn new() -> Option<Self> {
        Some(Self {
            fonts: Fonts::new(),
            canvas: Canvas::new(CANVAS_W, CANVAS_H)?,
        })
    }

    fn rgba(&self) -> Bitmap {
        Bitmap {
            w: CANVAS_W,
            h: CANVAS_H,
            pixels: Pixels::Rgba(self.canvas.pack_rgba()),
        }
    }

    /// Le visuel « produit » d'un panneau (hologramme ou caisson) : l'art de la publicité, sans cadre ni bandeau.
    pub fn product(&mut self, d: &Draw<'_>) -> Bitmap {
        self.canvas.clear();
        arts::draw(&mut self.canvas, &mut self.fonts, d);
        self.rgba()
    }

    /// L'enseigne en tubes de néon de `text`.
    pub fn neon(&mut self, text: &str, seed: f32) -> Bitmap {
        self.canvas.clear();
        neon::neon(&mut self.canvas, &mut self.fonts, text, seed);
        self.rgba()
    }

    /// La projection fantôme de `text` (un canal).
    pub fn ghost(&mut self, text: &str) -> Bitmap {
        self.canvas.clear();
        neon::ghost(&mut self.canvas, &mut self.fonts, text);
        Bitmap {
            w: CANVAS_W,
            h: CANVAS_H,
            pixels: Pixels::R8(self.canvas.pack_r8()),
        }
    }

    /// L'habillage d'un écran-caméra ; `live` : (nom, événement) d'un écran géant du DJ.
    pub fn cam(&mut self, live: Option<(&str, &str)>) -> Bitmap {
        self.canvas.clear();
        cam::cam(&mut self.canvas, &mut self.fonts, live);
        self.rgba()
    }

    /// Le bandeau défilant qui enchaîne `slogans` (au plus [`TICKER_SLOGANS`], et pas plus large que [`STRIP_MAX_W`]).
    pub fn strip(&mut self, slogans: &[String]) -> Option<Strip> {
        let mut text = String::new();
        for (i, slogan) in slogans.iter().take(TICKER_SLOGANS).enumerate() {
            let next = if i == 0 {
                slogan.clone()
            } else {
                format!("{text} /// {slogan}")
            };
            let width = self.fonts.width(&format!("{next}{GAP}"), Face::Mono, TICKER_FONT);
            if i > 0 && width > STRIP_MAX_W {
                break;
            }
            text = next;
        }
        text.push_str(GAP);
        let period = self.fonts.width(&text, Face::Mono, TICKER_FONT).max(1.0);
        let width = period.ceil() as u32 + 2;
        let mut canvas = Canvas::new(width, TICKER_H)?;
        let run = self.fonts.layout(&text, Face::Mono, TICKER_FONT);
        let baseline = TICKER_H as f32 / 2.0 + 1.0 + TICKER_FONT * 0.34;
        icons::draw_run(&mut canvas, Ink::new(R).glow(8.0), &run, 0.0, baseline, None);
        Some(Strip {
            bitmap: Bitmap {
                w: width,
                h: TICKER_H,
                pixels: Pixels::R8(canvas.pack_r8()),
            },
            period,
        })
    }
}

#[cfg(test)]
mod tests;
