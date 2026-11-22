//! Le paramètre de système qui donne aux ponts le rythme tel que le contrat le publie (`docs/rythme.md`).
//!
//! Il regroupe l'énergie (`AudioFeatures`) et les signaux filtrés (`FilteredSignals` : phase, accents, confiance de
//! grille) en un seul paramètre : un système Bevy ne dépasse pas seize paramètres, et les ponts n'ont ainsi qu'une
//! porte à franchir.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::AudioFeatures;
use super::routing::FilteredSignals;

/// L'énergie et les signaux filtrés de l'image. Les deux sont optionnels : sans pont audio ou sans routage, on lit le
/// repos (`FilteredSignals::default()`, grille sans confiance).
#[derive(SystemParam, Debug)]
pub struct RhythmInputs<'w> {
    /// L'état d'analyse de l'image (énergie, section, tempo).
    pub features: Option<Res<'w, AudioFeatures>>,
    /// Les signaux filtrés de l'image (accents, grille, confiance).
    pub signals: Option<Res<'w, FilteredSignals>>,
}

impl RhythmInputs<'_> {
    /// Les signaux filtrés de l'image, au repos s'ils n'existent pas.
    #[must_use]
    pub fn signals(&self) -> FilteredSignals {
        self.signals.as_deref().copied().unwrap_or_default()
    }
}
