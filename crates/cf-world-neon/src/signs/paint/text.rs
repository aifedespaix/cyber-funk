//! Le texte des enseignes : `cosmic-text` pour la mise en forme, les contours des lettres pour le dessin.
//!
//! Les polices sont celles du dépôt (`assets/fonts/`, compilées dans le binaire : règle 8, rien n'est lu pendant un
//! show) et jamais celles de la machine : la même enseigne se dessine pareil partout. Chaque lettre devient un
//! [`Path`] `tiny-skia` ; on peut donc la remplir, la tracer (tubes de néon) ou la déformer comme n'importe quelle forme.
//!
//! Sillage demandait « Courier New » (bandeau, paragraphes), « Impact / Arial Black » (gros titres) et « Trebuchet MS »
//! (néon) : ici une police à pas fixe ([`Face::Mono`]) et une sans empattement grasse ([`Face::Sans`]).

use cosmic_text::{Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, SwashCache, Weight, fontdb};
use tiny_skia::{Path, PathBuilder};

/// DejaVu Sans Mono Bold (licence Bitstream Vera, `assets/fonts/LICENCE.md`).
const MONO: &[u8] = include_bytes!("../../../../../assets/fonts/DejaVuSansMono-Bold.ttf");
/// DejaVu Sans Bold.
const SANS: &[u8] = include_bytes!("../../../../../assets/fonts/DejaVuSans-Bold.ttf");

/// Les deux polices disponibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    /// Pas fixe : bandeaux, paragraphes, tempo.
    Mono,
    /// Sans empattement gras : titres, néons, projections.
    Sans,
}

impl Face {
    const fn family(self) -> &'static str {
        match self {
            Self::Mono => "DejaVu Sans Mono",
            Self::Sans => "DejaVu Sans",
        }
    }
}

/// Une ligne de texte mise en forme : les contours des lettres, origine à gauche de la ligne de base (y vers le bas).
pub struct Run {
    /// Un contour par lettre visible, déjà placé en x.
    pub glyphs: Vec<Path>,
    /// Largeur de la ligne, en pixels.
    pub width: f32,
    /// Position (x, largeur) de chaque lettre, y compris les espaces, dans l'ordre du texte.
    pub cells: Vec<(f32, f32)>,
}

impl std::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run").field("glyphs", &self.glyphs.len()).field("width", &self.width).finish()
    }
}

/// Le moteur de texte : base de polices, cache de contours.
pub struct Fonts {
    system: FontSystem,
    swash: SwashCache,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fonts").finish_non_exhaustive()
    }
}

impl Default for Fonts {
    fn default() -> Self {
        Self::new()
    }
}

impl Fonts {
    /// Charge les deux polices du dépôt, rien d'autre : pas de recherche dans les polices du système.
    #[must_use]
    pub fn new() -> Self {
        let mut db = fontdb::Database::new();
        db.load_font_data(MONO.to_vec());
        db.load_font_data(SANS.to_vec());
        Self {
            system: FontSystem::new_with_locale_and_db("fr-FR".to_owned(), db),
            swash: SwashCache::new(),
        }
    }

    /// Met `text` en forme à `size` pixels. Un texte vide donne une ligne vide.
    pub fn layout(&mut self, text: &str, face: Face, size: f32) -> Run {
        let mut buffer = Buffer::new(&mut self.system, Metrics::new(size, size * 1.2));
        // Pas de retour à la ligne : le découpage en lignes est fait par `wrap`, qui mesure mot par mot.
        buffer.set_size(None, None);
        let attrs = Attrs::new()
            .family(Family::Name(face.family()))
            .weight(Weight::BOLD)
            .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.system, false);
        let mut run = Run {
            glyphs: Vec::with_capacity(text.chars().count()),
            width: 0.0,
            cells: Vec::with_capacity(text.chars().count()),
        };
        for line in buffer.layout_runs() {
            run.width = run.width.max(line.line_w);
            for glyph in line.glyphs {
                let x = glyph.x + glyph.font_size * glyph.x_offset;
                run.cells.push((x, glyph.w));
                let key = glyph.physical((0.0, 0.0), 1.0).cache_key;
                let Some(commands) = self.swash.get_outline_commands(&mut self.system, key) else {
                    continue;
                };
                if let Some(path) = outline(commands, x) {
                    run.glyphs.push(path);
                }
            }
        }
        run
    }

    /// Largeur de `text` à `size` pixels.
    pub fn width(&mut self, text: &str, face: Face, size: f32) -> f32 {
        self.layout(text, face, size).width
    }

    /// Découpe `text` en lignes qui tiennent dans `max_width` (`wrap` de Sillage) : on ajoute des mots tant que la ligne
    /// tient ; un mot seul trop long reste sur sa ligne.
    pub fn wrap(&mut self, text: &str, face: Face, size: f32, max_width: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut line = String::new();
        for word in text.split(' ') {
            let next = if line.is_empty() { word.to_owned() } else { format!("{line} {word}") };
            if !line.is_empty() && self.width(&next, face, size) > max_width {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            } else {
                line = next;
            }
        }
        if !line.is_empty() {
            lines.push(line);
        }
        lines
    }
}

/// Convertit les commandes de contour de `swash` (y vers le haut) en chemin `tiny-skia` (y vers le bas), décalé de `x`.
fn outline(commands: &[cosmic_text::Command], x: f32) -> Option<Path> {
    use cosmic_text::Command::{Close, CurveTo, LineTo, MoveTo, QuadTo};
    let mut pb = PathBuilder::new();
    for command in commands {
        match command {
            MoveTo(p) => pb.move_to(x + p.x, -p.y),
            LineTo(p) => pb.line_to(x + p.x, -p.y),
            QuadTo(c, p) => pb.quad_to(x + c.x, -c.y, x + p.x, -p.y),
            CurveTo(c1, c2, p) => pb.cubic_to(x + c1.x, -c1.y, x + c2.x, -c2.y, x + p.x, -p.y),
            Close => pb.close(),
        }
    }
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_pas_fixe_a_la_meme_chasse_partout() {
        let mut fonts = Fonts::new();
        let a = fonts.width("MMMMMMMM", Face::Mono, 24.0);
        let b = fonts.width("iiiiiiii", Face::Mono, 24.0);
        assert!((a - b).abs() < 0.5, "{a} ≠ {b}");
        assert!(a > 100.0);
    }

    #[test]
    fn les_accents_et_les_signes_du_catalogue_sont_dessines() {
        let mut fonts = Fonts::new();
        for text in ["ÉCHÉANCE À LA BÊTE", "« STEAK »", "MOTEL ∞", "◆ SECT.4", "AUJOURD'HUI…", "100% BIO*"] {
            let run = fonts.layout(text, Face::Sans, 40.0);
            let visible = text.chars().filter(|c| !c.is_whitespace()).count();
            assert_eq!(run.glyphs.len(), visible, "{text} : une lettre n'a pas de contour (police sans ce signe ?)");
        }
    }

    #[test]
    fn le_decoupage_ne_depasse_pas_et_garde_tous_les_mots() {
        let mut fonts = Fonts::new();
        let text = "LE MOT « STEAK » EST PROTÉGÉ. CE QU'IL Y A DERRIÈRE, MOINS.";
        let lines = fonts.wrap(text, Face::Mono, 25.0, 256.0);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), text);
        for line in &lines {
            assert!(fonts.width(line, Face::Mono, 25.0) <= 256.0 + 0.5 || !line.contains(' '), "{line}");
        }
    }
}
