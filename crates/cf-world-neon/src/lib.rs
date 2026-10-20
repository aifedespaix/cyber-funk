//! Neon City, le monde repris de Sillage : route infinie, immeubles néon, ciel et planètes, voiture en pilote
//! automatique, trafic, cascades, enseignes et événements. Implémente le trait `World` de `cf-render`.

pub mod rng;
pub mod route;

use bevy::prelude::*;

/// Plugin du monde Neon City. Vide tant que J5 n'est pas commencé.
#[derive(Debug, Default)]
pub struct NeonWorldPlugin;

impl Plugin for NeonWorldPlugin {
    fn build(&self, _app: &mut App) {}
}
