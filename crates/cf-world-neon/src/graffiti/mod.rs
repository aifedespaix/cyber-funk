//! Graffitis muraux et projecteurs holographiques : les données (mots, placements, licences des polices).
//!
//! Tout est tolérant : une entrée cassée ou une police sans licence est ignorée avec un avertissement lisible, et
//! l'application démarre quand même.

pub mod atlas;
pub mod bake;
pub mod fonts;
pub mod holo_material;
pub mod holo_systems;
pub mod placements;
pub mod projector;
pub mod quality;
pub mod raster;
pub mod words;

#[cfg(test)]
mod bake_tests;
#[cfg(test)]
mod holo_tests;
#[cfg(test)]
mod projector_tests;
#[cfg(test)]
mod quality_tests;
#[cfg(test)]
mod tests;

pub use bake::{GraffitiAtlas, GraffitiPlugin};
pub use holo_material::HoloMaterial;
pub use holo_systems::HoloPlugin;
pub use placements::{Placement, load_placements};
pub use quality::GraffitiQuality;
pub use words::{GraffitiStyle, GraffitiWord, licensed_fonts, load_words};

use serde::Deserialize;

/// Les trois classes de taille d'un projecteur holographique (valeurs de départ, à régler à l'œil).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum ProjectorSize {
    /// Devantures et mâts.
    Small,
    /// Immeubles moyens.
    Medium,
    /// Haut des tours.
    Large,
}

impl ProjectorSize {
    /// Longueur de la barre émettrice, en mètres.
    #[must_use]
    pub fn bar_len_m(self) -> f32 {
        match self {
            Self::Small => 2.0,
            Self::Medium => 6.0,
            Self::Large => 20.0,
        }
    }

    /// Hauteur des lettres projetées, en mètres.
    #[must_use]
    pub fn letter_h_m(self) -> f32 {
        match self {
            Self::Small => 1.0,
            Self::Medium => 3.0,
            Self::Large => 10.0,
        }
    }
}

/// Lit une liste RON entrée par entrée : chaque élément est converti séparément pour qu'une entrée cassée
/// n'invalide pas les autres. Retourne les valeurs lues et les avertissements.
pub(crate) fn load_list<T: serde::de::DeserializeOwned>(ron_text: &str, what: &str) -> (Vec<T>, Vec<String>) {
    if ron_text.trim().is_empty() {
        return (Vec::new(), Vec::new());
    }
    // `RawValue` garde le texte brut de chaque entrée : `ron::Value` ne sait pas porter les énumérations.
    let entries: Vec<Box<ron::value::RawValue>> = match ron::from_str(ron_text) {
        Ok(v) => v,
        Err(e) => return (Vec::new(), vec![format!("{what} : fichier illisible ({e})")]),
    };
    let mut out = Vec::with_capacity(entries.len());
    let mut warnings = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        match entry.into_rust::<T>() {
            Ok(v) => out.push(v),
            Err(e) => warnings.push(format!("{what} : entrée {} ignorée ({e})", i + 1)),
        }
    }
    (out, warnings)
}
