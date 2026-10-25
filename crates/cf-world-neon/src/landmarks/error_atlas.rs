//! L'atlas du panneau ERROR (`glyph-atlas.ts`) : une rangée par mot, blanc sur noir, chaque mot centré dans sa rangée.
//!
//! Sillage le dessinait dans un `<canvas>` au premier affichage de l'immeuble. Ici il est **cuit à l'ouverture** (règle 8 de
//! `CLAUDE.md`) avec `tiny-skia` et `cosmic-text` (la police à pas fixe du dépôt, comme les enseignes), puis envoyé dans une
//! texture à un canal. Le panneau et le décalque de la route lisent la même texture.
//!
//! Police à pas fixe pour que l'indice d'une lettre se déduise de sa position, sans table de largeurs : chaque lettre est
//! centrée dans une cellule de largeur [`ADVANCE`] × la taille de la police, quel que soit le dessin de la lettre. Les
//! positions des mots ([`spans`]) sont donc **analytiques**, connues sans cuire l'atlas.

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Transform};

use crate::signs::paint::text::{Face, Fonts};

/// Les quatre textes du panneau, dans l'ordre des rangées de l'atlas.
pub const ERROR_WORDS: [&str; 4] = ["ERROR", "CORRUPTED", "SYSTEM FAILURE", "OVERFLOW"];

/// Largeur de l'atlas, en pixels.
pub const WIDTH: u32 = 1024;
/// Hauteur d'une rangée.
pub const ROW: u32 = 256;
/// Chasse d'une police à pas fixe, en fraction de la taille.
pub const ADVANCE: f32 = 0.6;

/// Où est un mot dans sa rangée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordSpan {
    /// Début du texte dans la rangée, en fraction de la largeur.
    pub from: f32,
    /// Fin du texte.
    pub to: f32,
    /// Nombre de lettres.
    pub letters: usize,
}

/// La taille de police d'un mot : elle tient à la fois dans la largeur (96 %) et dans la hauteur (78 %) de la rangée.
fn font_size(letters: usize) -> f32 {
    (WIDTH as f32 * 0.96 / (letters as f32 * ADVANCE)).min(ROW as f32 * 0.78)
}

/// Les positions des quatre mots (`spans` de `createErrorAtlas`).
#[must_use]
pub fn spans() -> [WordSpan; 4] {
    ERROR_WORDS.map(|word| {
        let letters = word.chars().count();
        let width = ADVANCE * font_size(letters) * letters as f32;
        let x0 = (WIDTH as f32 - width) / 2.0;
        WordSpan {
            from: x0 / WIDTH as f32,
            to: (x0 + width) / WIDTH as f32,
            letters,
        }
    })
}

/// Cuit l'atlas : `WIDTH × (ROW × 4)` octets, un par pixel (la couverture). `None` si la taille est invalide.
#[must_use]
pub fn bake(fonts: &mut Fonts) -> Option<Vec<u8>> {
    let mut pixmap = Pixmap::new(WIDTH, ROW * ERROR_WORDS.len() as u32)?;
    let mut paint = Paint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = true;
    for (row, (word, span)) in ERROR_WORDS.iter().zip(spans()).enumerate() {
        let size = font_size(span.letters);
        let cell = ADVANCE * size;
        let x0 = span.from * WIDTH as f32;
        // Ligne de base : le centre de la rangée (0,52 de sa hauteur, comme Sillage) plus un tiers de la taille de police.
        let baseline = ROW as f32 * (row as f32 + 0.52) + size * 0.34;
        for (i, letter) in word.chars().enumerate() {
            let run = fonts.layout(&letter.to_string(), Face::Mono, size);
            let mut builder = PathBuilder::new();
            for glyph in &run.glyphs {
                builder.push_path(glyph);
            }
            let Some(path) = builder.finish() else { continue };
            let x = x0 + cell * (i as f32 + 0.5) - run.width / 2.0;
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::Winding,
                Transform::from_translate(x, baseline),
                None,
            );
        }
    }
    Some(pixmap.data().as_chunks::<4>().0.iter().map(|px| px[3]).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_mots_sont_centres_et_tiennent_dans_la_rangee() {
        for span in spans() {
            assert!(span.from > 0.0 && span.to < 1.0 && span.from < span.to);
            assert!(((span.from + span.to) / 2.0 - 0.5).abs() < 1e-5, "centré");
        }
        // « SYSTEM FAILURE » (14 lettres) remplit 96 % de la largeur.
        let long = spans()[2];
        assert!((long.to - long.from - 0.96).abs() < 1e-3);
        assert_eq!(long.letters, 14);
    }

    #[test]
    fn l_atlas_cuit_porte_de_l_encre_dans_chaque_rangee_et_dans_la_boite_du_mot() {
        let mut fonts = Fonts::new();
        let Some(pixels) = bake(&mut fonts) else {
            panic!("l'atlas doit se cuire");
        };
        assert_eq!(pixels.len(), (WIDTH * ROW * 4) as usize);
        for (row, span) in spans().iter().enumerate() {
            let (mut min_x, mut max_x, mut ink) = (WIDTH, 0, 0_usize);
            for y in 0..ROW {
                for x in 0..WIDTH {
                    if pixels[((row as u32 * ROW + y) * WIDTH + x) as usize] > 128 {
                        min_x = min_x.min(x);
                        max_x = max_x.max(x);
                        ink += 1;
                    }
                }
            }
            assert!(ink > 1000, "rangée {row} : {ink} pixels");
            // L'encre reste dans la boîte du mot (à quelques pixels de marge de dessin près).
            assert!(
                min_x as f32 >= span.from * WIDTH as f32 - 12.0,
                "rangée {row} déborde à gauche"
            );
            assert!(
                max_x as f32 <= span.to * WIDTH as f32 + 12.0,
                "rangée {row} déborde à droite"
            );
        }
    }
}
