//! Les mots des graffitis (`assets/graffiti/words.ron`) et les licences des polices.

use serde::Deserialize;

/// Rendu des lettres.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum GraffitiStyle {
    /// Lettres pleines.
    Fill,
    /// Contour seul.
    Outline,
    /// Lettres extrudées.
    Extrude,
}

/// Un motif : un mot, sa police et ses registres permis.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct GraffitiWord {
    /// Texte affiché.
    pub text: String,
    /// Nom de la police (fichier sans extension dans `assets/fonts/`).
    pub font: String,
    /// Style des lettres.
    pub style: GraffitiStyle,
    /// Permis en graffiti mural.
    pub wall: bool,
    /// Permis en projecteur holographique.
    pub holo: bool,
    /// Teintes suggérées, en degrés.
    pub hues: Vec<f32>,
}

/// Lit `words.ron` ; une entrée cassée est ignorée avec un avertissement.
#[must_use]
pub fn load_words(ron_text: &str) -> (Vec<GraffitiWord>, Vec<String>) {
    super::load_list(ron_text, "words.ron")
}

/// Noms des polices (fichier sans extension) dont la ligne de `LICENCES.md` est remplie. Une ligne marquée
/// `À COMPLÉTER` ou sans licence n'est pas comptée.
#[must_use]
pub fn licensed_fonts(licences_md: &str) -> Vec<String> {
    licences_md
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.trim().trim_matches('|').split('|').map(str::trim).collect();
            // Colonnes : Nom, Fichier, Source, Licence, Date.
            let [_, file, _, licence, ..] = cells.as_slice() else {
                return None;
            };
            let file = file.trim_matches('`');
            let (stem, ext) = file.rsplit_once('.')?;
            let is_font = ext.eq_ignore_ascii_case("ttf") || ext.eq_ignore_ascii_case("otf");
            let filled = !licence.is_empty() && !licence.contains("À COMPLÉTER");
            (is_font && filled).then(|| stem.to_string())
        })
        .collect()
}
