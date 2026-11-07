//! Les polices des graffitis : lues dans `assets/fonts/` à l'ouverture (jamais pendant un show), une base `fontdb` par
//! police (aucune police du système, aucun remplacement silencieux par une autre police), et la mise en forme d'un mot
//! en contours de lettres, comme les enseignes ([`crate::signs::paint::text`]).

use std::path::Path;
use std::sync::Arc;

use cosmic_text::fontdb;
use cosmic_text::{
    Attrs, Buffer, CacheKeyFlags, Family, FontSystem, Metrics, Shaping, Stretch, Style, SwashCache, Weight,
};
use tiny_skia::{Path as Outline, Rect};

use crate::signs::paint::text::outline;

/// Taille de mise en forme, en pixels : les contours sont ensuite mis à l'échelle de la cellule.
const REF_PX: f32 = 100.0;

/// Une police chargée.
pub(super) struct LoadedFont {
    system: FontSystem,
    swash: SwashCache,
    family: String,
    weight: Weight,
    style: Style,
    stretch: Stretch,
}

impl std::fmt::Debug for LoadedFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoadedFont")
            .field("family", &self.family)
            .finish_non_exhaustive()
    }
}

/// Un mot mis en forme : les contours de ses lettres et leur boîte englobante (y vers le bas).
#[derive(Debug, Clone)]
pub(super) struct Shape {
    pub glyphs: Vec<Outline>,
    pub bounds: Rect,
}

/// Charge la police `name` (`<name>.ttf` ou `<name>.otf` dans `dir`). L'erreur dit pourquoi, en clair.
pub(super) fn load_font(dir: &Path, name: &str) -> Result<LoadedFont, String> {
    // Un nom est un fichier du dossier, rien d'autre : pas de chemin qui en sorte.
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err("au nom invalide".to_owned());
    }
    let data = ["ttf", "otf"]
        .iter()
        .find_map(|ext| std::fs::read(dir.join(format!("{name}.{ext}"))).ok())
        .ok_or_else(|| "absente de assets/fonts".to_owned())?;
    let mut db = fontdb::Database::new();
    let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(data)));
    let face = ids
        .first()
        .and_then(|id| db.face(*id))
        .ok_or_else(|| "illisible".to_owned())?;
    let family = face
        .families
        .first()
        .map(|(family, _)| family.clone())
        .ok_or_else(|| "illisible (sans nom de famille)".to_owned())?;
    let (weight, style, stretch) = (face.weight, face.style, face.stretch);
    Ok(LoadedFont {
        system: FontSystem::new_with_locale_and_db("fr-FR".to_owned(), db),
        swash: SwashCache::new(),
        family,
        weight,
        style,
        stretch,
    })
}

impl LoadedFont {
    /// Met `text` en forme. Échoue si un signe manque à la police (il serait dessiné en boîte vide) ou si rien n'est
    /// visible.
    pub fn shape(&mut self, text: &str) -> Result<Shape, String> {
        let family = self.family.clone();
        let mut buffer = Buffer::new(&mut self.system, Metrics::new(REF_PX, REF_PX * 1.2));
        buffer.set_size(None, None);
        let attrs = Attrs::new()
            .family(Family::Name(&family))
            .weight(self.weight)
            .style(self.style)
            .stretch(self.stretch)
            .cache_key_flags(CacheKeyFlags::DISABLE_HINTING);
        buffer.set_text(text, &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut self.system, false);
        let mut glyphs = Vec::with_capacity(text.len());
        let mut missing = String::new();
        for line in buffer.layout_runs() {
            for glyph in line.glyphs {
                let source = text.get(glyph.start..glyph.end).unwrap_or_default();
                if glyph.glyph_id == 0 && !source.trim().is_empty() {
                    missing.push_str(source);
                    continue;
                }
                let x = glyph.x + glyph.font_size * glyph.x_offset;
                let key = glyph.physical((0.0, 0.0), 1.0).cache_key;
                let Some(commands) = self.swash.get_outline_commands(&mut self.system, key) else {
                    continue;
                };
                if let Some(path) = outline(commands, x) {
                    glyphs.push(path);
                }
            }
        }
        if !missing.is_empty() {
            return Err(format!("signes absents de la police : « {missing} »"));
        }
        let bounds = glyphs
            .iter()
            .map(Outline::bounds)
            .reduce(|a, b| {
                Rect::from_ltrb(
                    a.left().min(b.left()),
                    a.top().min(b.top()),
                    a.right().max(b.right()),
                    a.bottom().max(b.bottom()),
                )
                .unwrap_or(a)
            })
            .ok_or_else(|| "aucun signe visible".to_owned())?;
        Ok(Shape { glyphs, bounds })
    }
}
