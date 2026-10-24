//! Enseigne en tubes de néon et projection fantôme (`art-neon.ts`).
//!
//! Le néon : des lettres creuses (un trait épais lumineux, un cœur clair plus fin), un cadre arrondi interrompu. Les
//! tubes « morts » de Sillage (un par enseigne, qui grésillent) sont cuits sur le canal `B` : le matériau les fait
//! respirer lentement, jamais en stroboscope (règle 7).
//!
//! La projection : le slogan en très grosses lettres, sans fond ni cadre, une seule couleur (canal `R`).

use super::canvas::{B, Canvas, G, Ink, R};
use super::icons::draw_run;
use super::text::{Face, Fonts};
use crate::rng::hash;
use crate::signs::formats::{CANVAS_H, CANVAS_W};

const W: f32 = CANVAS_W as f32;
const H: f32 = CANVAS_H as f32;
/// Luminosité de repos des tubes (`0.75 + pulse * 0.25` avec `pulse = 0`).
const BRIGHT: f32 = 0.75;

/// Taille de police et lignes du texte d'une enseigne en tubes : la taille baisse jusqu'à tenir en 2 lignes.
fn tube_lines(fonts: &mut Fonts, text: &str) -> (f32, Vec<String>) {
    let room = W - 90.0;
    let mut size = 96.0_f32;
    let mut lines = fonts.wrap(text, Face::Sans, size, room);
    while (lines.len() > 2 || lines.iter().any(|l| fonts.width(l, Face::Sans, size) > room)) && size > 36.0 {
        size -= 6.0;
        lines = fonts.wrap(text, Face::Sans, size, room);
    }
    (size, lines)
}

/// Dessine l'enseigne de `text` ; `seed` tire les tubes morts et la coupure du cadre.
pub fn neon(canvas: &mut Canvas, fonts: &mut Fonts, text: &str, seed: f32) {
    let (size, lines) = tube_lines(fonts, text);
    let lead = size * 1.12;
    let top = H / 2.0 - (lines.len() as f32 - 1.0) * lead / 2.0 + size * 0.33;
    let mut index = 0_u32;
    for (row, line) in lines.iter().enumerate() {
        let run = fonts.layout(line, Face::Sans, size);
        let x0 = (W - run.width) / 2.0;
        let mut glyph = 0;
        for letter in line.chars() {
            if letter.is_whitespace() {
                index += 1;
                continue;
            }
            let dead = hash(f64::from(seed) + f64::from(index), 7.0) > 0.93;
            if let Some(path) = run.glyphs.get(glyph) {
                let one = super::text::Run {
                    glyphs: vec![path.clone()],
                    width: 0.0,
                    cells: Vec::new(),
                };
                tube(canvas, &one, x0, top + row as f32 * lead, dead);
            }
            glyph += 1;
            index += 1;
        }
    }
    // Cadre : un rectangle arrondi dont un côté est coupé.
    let gap = 38.0 + ((seed * 10.0) % 3.0).floor() * 26.0;
    let Some(path) = Canvas::rounded_path(18.0, 18.0, W - 36.0, H - 36.0, 34.0) else { return };
    let stroke = Canvas::dashed_stroke(7.0, W * 2.0 - gap * 2.0, gap, -seed * 40.0);
    canvas.stroke(Ink::new(R).alpha(BRIGHT).glow(22.0), &path, &stroke);
}

/// Un tube : halo épais, corps, cœur clair. Un tube mort vit sur le canal `B`, plus pâle.
fn tube(canvas: &mut Canvas, letter: &super::text::Run, x: f32, y: f32, dead: bool) {
    if dead {
        draw_run(canvas, Ink::new(B).alpha(BRIGHT).glow(26.0), letter, x, y, Some(6.0));
        return;
    }
    draw_run(canvas, Ink::new(R).alpha(BRIGHT * 0.55).glow(26.0), letter, x, y, Some(13.0));
    draw_run(canvas, Ink::new(R).alpha(BRIGHT), letter, x, y, Some(6.0));
    draw_run(canvas, Ink::new(G).alpha(BRIGHT * 0.9), letter, x, y, Some(2.0));
}

/// Dessine la projection de `text` : très grosses lettres centrées, en une couleur (canal `R`).
pub fn ghost(canvas: &mut Canvas, fonts: &mut Fonts, text: &str) {
    let text = strip_aside(text);
    let mut size = 76.0_f32;
    let mut lines = fonts.wrap(&text, Face::Sans, size, W - 40.0);
    while lines.len() as f32 * size * 1.08 > H - 24.0 && size > 28.0 {
        size -= 4.0;
        lines = fonts.wrap(&text, Face::Sans, size, W - 40.0);
    }
    let lead = size * 1.08;
    let top = H / 2.0 - (lines.len() as f32 - 1.0) * lead / 2.0;
    for (i, line) in lines.iter().enumerate() {
        let run = fonts.layout(line, Face::Sans, size);
        draw_run(canvas, Ink::new(R).glow(30.0), &run, (W - run.width) / 2.0, top + i as f32 * lead + size * 0.34, None);
    }
}

/// Retire la parenthèse finale d'un slogan (`replace(/\s*\(.*\)$/, "")`) : « (PAS BESOIN DE LIRE) » ne se projette pas.
#[must_use]
pub fn strip_aside(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.ends_with(')')
        && let Some(open) = trimmed.find('(')
    {
        return trimmed[..open].trim_end().to_owned();
    }
    text.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_parenthese_finale_est_retiree() {
        assert_eq!(strip_aside("AJOUTE AU PANIER. (PAS BESOIN DE LIRE)"), "AJOUTE AU PANIER.");
        assert_eq!(strip_aside("PROMO"), "PROMO");
    }

    #[test]
    fn un_slogan_neon_tient_en_deux_lignes() {
        let mut fonts = Fonts::new();
        for text in ["AUJOURD'HUI SEULEMENT", "LIVRAISON 5 MIN", "REVENDEZ-VOUS", "OUVERT 24H/24"] {
            let (size, lines) = tube_lines(&mut fonts, text);
            assert!(lines.len() <= 2 && size >= 36.0, "{text} : {size} {lines:?}");
        }
    }
}
