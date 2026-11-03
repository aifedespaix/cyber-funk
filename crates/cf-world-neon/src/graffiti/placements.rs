//! Les projecteurs posés à la main (`assets/graffiti/placements.ron`).

use super::ProjectorSize;
use serde::Deserialize;

/// Un projecteur holographique à un endroit précis.
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Placement {
    /// Distance le long de la route, en mètres.
    pub z_m: f64,
    /// Côté de la route : -1 gauche, 1 droite.
    pub side: i8,
    /// Hauteur de la barre, en mètres.
    pub height_m: f32,
    /// Classe de taille.
    pub size: ProjectorSize,
    /// Texte du mot (voir `words.ron`).
    pub word: String,
    /// Rotation autour de la verticale, en degrés.
    pub yaw_deg: f32,
}

/// Lit `placements.ron` ; une entrée cassée est ignorée avec un avertissement.
#[must_use]
pub fn load_placements(ron_text: &str) -> (Vec<Placement>, Vec<String>) {
    super::load_list(ron_text, "placements.ron")
}
