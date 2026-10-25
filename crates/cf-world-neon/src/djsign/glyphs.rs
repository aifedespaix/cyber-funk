//! Les glyphes de l'enseigne du DJ, cuits une fois à l'ouverture (`dj-sign-glyph.ts`).
//!
//! Sillage dessinait une texture de `<canvas>` par lettre à chaque changement de nom. Ici (règle 8 de `CLAUDE.md`) toutes
//! les lettres qu'un nom peut demander sont cuites **avant le show**, dans un atlas unique : les noms de la feuille de
//! conduite, plus un alphabet de secours (lettres accentuées, chiffres, ponctuation courante). Une lettre hors de ce jeu
//! (un nom saisi à la main pendant le show) s'affiche vide plutôt que de déclencher une cuisson.
//!
//! Une cellule porte trois canaux : `R` le cœur de la lettre, `G` son halo proche, `B` sa lueur large. Le matériau les
//! colore (cœur presque blanc, halo à la teinte de l'enseigne). Le halo est l'ombre seule du texte, comme dans Sillage
//! (`shadowBlur` avec un décalage hors cadre) : à l'intérieur de la lettre il reste donc plus faible que le cœur.

use std::collections::{BTreeSet, HashMap};

use tiny_skia::PathBuilder;

use super::layout::normalize_name;
use super::sign::{PAD_BOTTOM, PAD_TOP, PAD_X};
use crate::signs::paint::canvas::{B, Canvas, G, Ink, R};
use crate::signs::paint::text::{Face, Fonts};

/// Hauteur de capitale dans la texture, en pixels (200 dans Sillage ; 128 suffit à l'écran et tient l'atlas sous 20 Mo).
pub const CAP_PX: f32 = 128.0;
/// Rapport hauteur de capitale / corps de la police (`DejaVu` Sans Bold) : les glyphes sont mis à l'échelle pour que la
/// capitale fasse [`CAP_PX`].
const CAP_RATIO: f32 = 0.729;
/// Largeur d'avance bornée, comme dans la mise en page (`MIN_ADVANCE` et `MAX_ADVANCE`).
const MIN_ADVANCE: f32 = 0.3;
const MAX_ADVANCE: f32 = 1.3;
/// Avance d'un glyphe inconnu (`measure` sans canevas dans Sillage).
pub const UNKNOWN_ADVANCE: f32 = 0.7;
/// Alphabet de secours cuit en plus des noms du show.
const BASE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ÀÂÄÇÉÈÊËÎÏÔÖÙÛÜŸŒÆ.-_!?&'+:/#@*()$%,";

/// Où est une lettre dans l'atlas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlyphCell {
    /// Avance du glyphe, en hauteurs de capitale (bornée).
    pub advance: f32,
    /// Rectangle de la cellule dans l'atlas, `(u0, v0, u1, v1)`, v vers le bas comme l'image.
    pub rect: [f32; 4],
}

/// L'atlas cuit, côté processeur.
#[derive(Debug, Clone)]
pub struct GlyphBake {
    /// Largeur de l'atlas, px.
    pub width: u32,
    /// Hauteur de l'atlas, px.
    pub height: u32,
    /// Pixels RGBA 8 bits (R, G, B : couvertures ; A inutilisé).
    pub rgba: Vec<u8>,
    /// Les cellules par caractère.
    pub cells: HashMap<char, GlyphCell>,
}

impl GlyphBake {
    /// Avance d'un glyphe en hauteurs de capitale ; [`UNKNOWN_ADVANCE`] s'il n'a pas été cuit.
    #[must_use]
    pub fn advance(&self, ch: char) -> f32 {
        self.cells.get(&ch).map_or(UNKNOWN_ADVANCE, |c| c.advance)
    }

    /// La mesure que la mise en page appelle, détachée de l'atlas (elle voyage dans la ressource de l'enseigne).
    pub fn measure(&self) -> super::sign::Measure {
        let advances: HashMap<char, f32> = self.cells.iter().map(|(c, cell)| (*c, cell.advance)).collect();
        Box::new(move |ch| advances.get(&ch).copied().unwrap_or(UNKNOWN_ADVANCE))
    }

    /// Mémoire de l'atlas, en octets.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.rgba.len()
    }
}

/// Les caractères à cuire : l'alphabet de secours et ceux des noms (nettoyés comme l'enseigne les nettoie).
#[must_use]
pub fn charset<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeSet<char> {
    let mut set: BTreeSet<char> = BASE.chars().collect();
    for name in names {
        set.extend(
            normalize_name(name, super::layout::MAX_SIGN_CHARS)
                .chars()
                .filter(|c| *c != ' '),
        );
    }
    set
}

fn cell_size() -> (u32, u32) {
    let w = ((MAX_ADVANCE + PAD_X * 2.0) * CAP_PX).ceil() as u32;
    let h = ((1.0 + PAD_TOP + PAD_BOTTOM) * CAP_PX).ceil() as u32;
    (w, h)
}

/// Cuit l'atlas de `chars`. `None` si le canevas ne peut pas être alloué ou si le jeu est vide.
#[must_use]
pub fn bake(chars: &BTreeSet<char>) -> Option<GlyphBake> {
    if chars.is_empty() {
        return None;
    }
    let (cw, ch) = cell_size();
    let columns = (chars.len() as f32).sqrt().ceil() as u32;
    let rows = (chars.len() as u32).div_ceil(columns);
    let (width, height) = (columns * cw, rows * ch);
    let mut rgba = vec![0_u8; width as usize * height as usize * 4];
    let mut canvas = Canvas::new(cw, ch)?;
    let mut fonts = Fonts::new();
    let font_px = CAP_PX / CAP_RATIO;
    let plane_h = (1.0 + PAD_TOP + PAD_BOTTOM) * CAP_PX;
    let mut cells = HashMap::with_capacity(chars.len());

    for (i, &c) in chars.iter().enumerate() {
        let (col, row) = (i as u32 % columns, i as u32 / columns);
        let run = fonts.layout(&c.to_string(), Face::Sans, font_px);
        let raw = run.width / CAP_PX;
        let advance = if raw.is_finite() {
            raw.clamp(MIN_ADVANCE, MAX_ADVANCE)
        } else {
            UNKNOWN_ADVANCE
        };
        let plane_w = (advance + PAD_X * 2.0) * CAP_PX;

        canvas.clear();
        let mut builder = PathBuilder::new();
        for glyph in &run.glyphs {
            builder.push_path(glyph);
        }
        if let Some(path) = builder.finish() {
            // Le texte est centré dans son plan, sa ligne de base sous la marge du haut et la capitale.
            canvas.xf = tiny_skia::Transform::from_translate((plane_w - run.width) / 2.0, (PAD_TOP + 1.0) * CAP_PX);
            // Halo large (B, deux passes) et proche (G, trois passes), puis le cœur (R) : l'ordre de Sillage.
            for (channel, blur, passes) in [(B, CAP_PX * 0.3, 2), (G, CAP_PX * 0.1, 3)] {
                for _ in 0..passes {
                    canvas.glow_only(Ink::new(channel).glow(blur), &path);
                }
            }
            canvas.fill(Ink::new(R), &path);
        }
        let packed = canvas.pack_rgba();
        for y in 0..ch {
            let src = y as usize * cw as usize * 4;
            let dst = ((row * ch + y) as usize * width as usize + (col * cw) as usize) * 4;
            rgba[dst..dst + cw as usize * 4].copy_from_slice(&packed[src..src + cw as usize * 4]);
        }
        let (x0, y0) = ((col * cw) as f32, (row * ch) as f32);
        cells.insert(
            c,
            GlyphCell {
                advance,
                rect: [
                    x0 / width as f32,
                    y0 / height as f32,
                    (x0 + plane_w) / width as f32,
                    (y0 + plane_h) / height as f32,
                ],
            },
        );
    }
    Some(GlyphBake {
        width,
        height,
        rgba,
        cells,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bake_of(text: &str) -> GlyphBake {
        let set: BTreeSet<char> = text.chars().collect();
        bake(&set).expect("cuisson")
    }

    fn lit(bake: &GlyphBake, ch: char, channel: usize) -> usize {
        let cell = bake.cells[&ch];
        let (u0, v0) = (cell.rect[0] * bake.width as f32, cell.rect[1] * bake.height as f32);
        let (u1, v1) = (cell.rect[2] * bake.width as f32, cell.rect[3] * bake.height as f32);
        let mut n = 0;
        for y in v0 as u32..v1 as u32 {
            for x in u0 as u32..u1 as u32 {
                if bake.rgba[(y as usize * bake.width as usize + x as usize) * 4 + channel] > 8 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn le_jeu_de_caracteres_contient_les_noms_nettoyes_et_l_alphabet() {
        let set = charset(["  dj  ñandú "]);
        assert!(set.contains(&'D') && set.contains(&'J') && set.contains(&'Ñ') && set.contains(&'Ú'));
        assert!(set.contains(&'Z') && set.contains(&'7') && set.contains(&'É'));
        assert!(!set.contains(&' '));
    }

    #[test]
    fn un_jeu_vide_ne_cuit_rien() {
        assert!(bake(&BTreeSet::new()).is_none());
    }

    #[test]
    fn le_coeur_est_dessine_et_le_halo_deborde() {
        let b = bake_of("AIW");
        for c in ['A', 'I', 'W'] {
            let core = lit(&b, c, 0);
            assert!(core > 200, "{c} : {core} pixels de cœur");
            // Le halo proche puis la lueur large débordent du cœur.
            assert!(lit(&b, c, 1) > core, "{c} : halo proche");
            assert!(lit(&b, c, 2) > lit(&b, c, 1), "{c} : lueur large");
        }
    }

    #[test]
    fn les_avances_suivent_la_police() {
        let b = bake_of("IWM");
        assert!(b.advance('I') < b.advance('M'));
        assert!(b.advance('M') <= b.advance('W') + 0.3);
        assert!(b.advance('I') >= MIN_ADVANCE && b.advance('W') <= MAX_ADVANCE);
        assert!((b.advance('€') - UNKNOWN_ADVANCE).abs() < f32::EPSILON);
    }

    #[test]
    fn les_cellules_tiennent_dans_l_atlas_et_ne_se_chevauchent_pas() {
        let b = bake_of("ABCDEFGHIJKL");
        let mut rects: Vec<_> = b.cells.values().map(|c| c.rect).collect();
        assert!(
            rects
                .iter()
                .all(|r| r.iter().all(|v| (0.0..=1.0).contains(v)) && r[0] < r[2] && r[1] < r[3])
        );
        rects.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
        for (i, a) in rects.iter().enumerate() {
            for other in &rects[i + 1..] {
                let apart = a[2] <= other[0] || other[2] <= a[0] || a[3] <= other[1] || other[3] <= a[1];
                assert!(apart, "{a:?} chevauche {other:?}");
            }
        }
    }

    #[test]
    fn l_atlas_complet_reste_raisonnable() {
        let b = bake(&charset(["AIFE", "KIRA DJ"])).expect("cuisson");
        assert!(b.bytes() < 24_000_000, "{} Mo", b.bytes() / 1_000_000);
        assert!(b.width <= 4096 && b.height <= 4096);
    }

    #[test]
    fn la_mesure_detachee_donne_les_memes_avances() {
        let b = bake_of("AI");
        let measure = b.measure();
        assert!((measure('A') - b.advance('A')).abs() < f32::EPSILON);
        assert!((measure('?') - UNKNOWN_ADVANCE).abs() < f32::EPSILON);
    }
}
