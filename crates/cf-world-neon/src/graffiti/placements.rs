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

/// Lit `placements.ron` ; une entrée cassée ou invalide (côté hors de {-1, 1}, hauteur négative ou non finie) est
/// ignorée avec un avertissement.
#[must_use]
pub fn load_placements(ron_text: &str) -> (Vec<Placement>, Vec<String>) {
    let (mut list, mut warnings) = super::load_list::<Placement>(ron_text, "placements.ron");
    list.retain(|p| match p.problem() {
        Some(why) => {
            warnings.push(format!("placements.ron : « {} » ignoré ({why})", p.word));
            false
        }
        None => true,
    });
    (list, warnings)
}

impl Placement {
    /// Ce qui rend le placement inutilisable, ou `None` : côté hors de {-1, 1}, hauteur négative ou non finie.
    #[must_use]
    pub fn problem(&self) -> Option<&'static str> {
        if self.side != -1 && self.side != 1 {
            Some("côté hors de -1 / 1")
        } else if !self.height_m.is_finite() || self.height_m < 0.0 {
            Some("hauteur négative")
        } else {
            None
        }
    }
}
