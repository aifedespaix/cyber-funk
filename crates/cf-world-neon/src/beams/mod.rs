//! Les projecteurs du décor (J5-07 ; `beams.ts` de Sillage) : des faisceaux fins accrochés aux façades des tours, qui balaient
//! la route avec une tache lumineuse posée au sol, et ceux que les événements du trafic braquent sur un point de la route
//! (sirènes, ravitailleur, cargo publicitaire : `Scene::pins`, publiés par J7-04/05 et que personne ne dessinait).
//!
//! # Qui écrit quoi
//!
//! - [`Beams`] : le champ de projecteurs ([`BeamField`]), la voie du pouls ([`BeamPulse`]) et le cache des points d'attache
//!   ([`SpotWindow`]). Écrit par le seul [`systems::step_beams`] (`CfSet::Simulate`, [`BeamSet`], après le trafic).
//! - [`BeamRig`] et les entités : créés au démarrage, réécrits par `sync::sync_beams` (`CfSet::Present`).
//! - [`Beams::sample`] (`beams.sample` de Sillage) donne la teinte du faisceau sur lequel roule la voiture ; l'éclairage de la
//!   voiture n'est pas branché dessus ici (voir les écarts de la PR).
//!
//! # Sécurité photosensible (règle 7)
//!
//! Le pouls des kicks sur le cône et sur la tache passe par un `FlashLimiter` ([`safety`]) : 3 départs par seconde au plus,
//! pente de `MIN_FADE` au moins, rien sans spectacle, en respiration ou flashs coupés. Le balayage est lisse ([`field`]) : le
//! compteur de temps est adouci, la fréquence du point visé reste très sous 3 Hz, les médiums sont lissés.
//!
//! # Culling et coût
//!
//! Voir [`rig`] : aucun `NoFrustumCulling`. Les [`MAX_BEAMS`]` × 2` entités existent dès le démarrage, aucune n'est créée en
//! régime établi ; deux lots de dessin au plus ; le matériau (déclaré à `warm_material`, règle 8) n'est réécrit que si son
//! niveau change.

pub mod field;
pub mod material;
pub mod rig;
pub mod safety;
pub mod spots;
mod sync;
pub mod systems;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, WarmupAppExt, WorldSet};

pub use field::{BEAM_RADIUS, BeamField, BeamInput, BeamSlot, MAX_BEAMS, max_sweep_hz};
pub use material::{BeamKind, BeamMaterial, BeamParams};
pub use rig::{BeamPart, BeamPiece, BeamRig};
pub use safety::BeamPulse;
pub use spots::SpotWindow;
pub use sync::{cone_pose, pack_tint, pool_pose};

use crate::traffic::TrafficSet;

/// L'étape où les projecteurs avancent (`CfSet::Simulate`, après le trafic dont ils lisent les projecteurs épinglés).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BeamSet;

/// Les projecteurs du décor. Écrit par le seul [`systems::step_beams`].
#[derive(Resource, Debug, Default)]
pub struct Beams {
    field: BeamField,
    pulse: BeamPulse,
    window: SpotWindow,
}

impl Beams {
    /// Les projecteurs de l'image.
    #[must_use]
    pub fn field(&self) -> &BeamField {
        &self.field
    }

    /// La voie du pouls des kicks.
    #[must_use]
    pub fn pulse(&self) -> &BeamPulse {
        &self.pulse
    }

    /// Teinte du faisceau sur lequel roule la voiture en `(x, z)` (absolus), ou `None` si elle est dans l'ombre.
    #[must_use]
    pub fn sample(&self, x: f64, z: f64) -> Option<[f32; 3]> {
        self.field.sample(x, z)
    }
}

/// Installe les projecteurs : matériau et shader, pool d'entités, le système qui les place et celui qui les montre.
///
/// À ajouter après les plugins de rendu de Bevy et [`NeonWorldPlugin`](crate::NeonWorldPlugin) (la ville fournit les points
/// d'attache) ; le trafic, s'il est là, fournit les projecteurs épinglés. Sans rendu (tests), seul le matériau est déclaré et la
/// logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct BeamsPlugin;

impl Plugin for BeamsPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins(MaterialPlugin::<BeamMaterial>::default())
                // Caché au repos, il ne compilerait qu'au premier projecteur en plein show sans cette préchauffe.
                .warm_material::<BeamMaterial>("projecteurs : faisceaux et taches");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<BeamMaterial>();
        }
        app.init_resource::<Beams>()
            .add_systems(Startup, rig::spawn_beams.run_if(resource_exists::<Assets<Mesh>>))
            .add_systems(
                Update,
                systems::step_beams
                    .in_set(BeamSet)
                    .in_set(CfSet::Simulate)
                    .after(WorldSet::Chunks)
                    .after(TrafficSet::Step),
            )
            .add_systems(
                Update,
                sync::sync_beams
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<BeamRig>),
            );
    }
}

#[cfg(test)]
mod tests;
