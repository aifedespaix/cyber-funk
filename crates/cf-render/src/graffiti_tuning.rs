//! Les réglages des graffitis et des projecteurs holographiques faits à la main dans le panneau Debug (F4), vus de la
//! fenêtre opérateur.
//!
//! Comme [`crate::FacadeTuning`] : la fenêtre opérateur ne connaît pas le monde (`cf-world-neon`). Elle écrit
//! [`GraffitiTuning`], le monde la lit et re-tire la ville quand elle change. Ces réglages sont réservés à la Préparation
//! (comme le reste de F4), ne sont **pas** sauvegardés dans les looks ni dans les réglages : chaque session repart des
//! densités livrées.

use bevy::prelude::*;

/// Borne haute des curseurs de densité : 4 fois plus de graffitis que la densité livrée.
pub const MAX_GRAFFITI_DENSITY: f32 = 4.0;

/// Réglages des graffitis faits à la main.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct GraffitiTuning {
    /// Les graffitis muraux sont peints.
    pub wall_enabled: bool,
    /// Les projecteurs holographiques sont posés.
    pub holo_enabled: bool,
    /// Multiple de la densité livrée des graffitis muraux : 1 = comme livré, 0 = aucun, 4 = quatre fois plus.
    pub wall_density: f32,
    /// Multiple de la densité livrée des projecteurs automatiques.
    pub holo_density: f32,
    /// Mot imposé à tous les projecteurs proches (un mot holographique chargé), ou `None` pour les mots tirés.
    pub forced_word: Option<String>,
}

impl Default for GraffitiTuning {
    fn default() -> Self {
        Self {
            wall_enabled: true,
            holo_enabled: true,
            wall_density: 1.0,
            holo_density: 1.0,
            forced_word: None,
        }
    }
}

impl GraffitiTuning {
    /// Les mêmes réglages, densités bornées à `0..=`[`MAX_GRAFFITI_DENSITY`] (une valeur illisible vaut 1).
    #[must_use]
    pub fn clamped(mut self) -> Self {
        let fix = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, MAX_GRAFFITI_DENSITY)
            } else {
                1.0
            }
        };
        self.wall_density = fix(self.wall_density);
        self.holo_density = fix(self.holo_density);
        self
    }

    /// Multiple de densité que le monde applique aux graffitis muraux : 0 si coupés.
    #[must_use]
    pub fn wall_factor(&self) -> f32 {
        if self.wall_enabled {
            self.clone().clamped().wall_density
        } else {
            0.0
        }
    }

    /// Multiple de densité que le monde applique aux projecteurs automatiques : 0 si coupés.
    #[must_use]
    pub fn holo_factor(&self) -> f32 {
        if self.holo_enabled {
            self.clone().clamped().holo_density
        } else {
            0.0
        }
    }
}

/// Les mots holographiques que le monde a chargés (écrit par le monde une fois les atlas cuits), pour que le panneau n'offre
/// que des mots qui existent.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct GraffitiWords(pub Vec<String>);

impl GraffitiWords {
    /// Le mot imposé est-il un mot chargé ?
    #[must_use]
    pub fn contains(&self, word: &str) -> bool {
        self.0.iter().any(|w| w == word)
    }
}
