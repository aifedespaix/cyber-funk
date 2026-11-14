//! L'atmosphère de la ville (chantier C3 ; DA §1.2, §2.1, §3.2 et P2 §1.3) : de l'air entre les plans.
//!
//! - [`formula`] : les formules du brouillard analytique (transmittance, phase, tube, coquille de kick), miroir Rust de
//!   `atmosphere.wesl`, testées contre une intégration numérique ;
//! - [`modulation`] : la brume réagit au son (densité, anisotropie, bouffée de drop **[L]**, apnée, vent) ;
//! - [`light`] : le bloc d'uniformes `SceneLight` (principale, remplissage, contre-jour, brume) et sa composition ;
//! - [`dust`] : les poussières, trois coquilles, un maillage instancié sans état ;
//! - [`systems`] : les systèmes et les ressources.
//!
//! # Ce que ce module ne remplace pas
//!
//! Les plans de brume `mist` et `haze` ([`crate::weather`]) restent : ils donnent le détail proche que la formule n'a
//! pas. Le fondu de distance que chaque shader fait vers sa teinte de fond (presque noire) reste aussi : il efface le
//! détail au loin et laisse une silhouette. `atmo_fog` s'applique par-dessus : il ne change rien tant que le bloc est à
//! zéro (tests, démos sans ce plugin), et découpe la silhouette dans la brume sinon. Aucune passe plein écran n'est ajoutée.
//!
//! # Qui écrit quoi
//!
//! - [`SceneLightUniform`] et [`FogReach`] : écrits par le seul `update_scene_light` (`CfSet::Simulate`, après la météo).
//! - Le champ `scene` de chaque matériau du monde : écrit par le seul `sync_scene_light::<M>` (`CfSet::Present`).
//! - [`AtmosphereLook`] : écrit par les événements du spectacle (apnée).
//!
//! # Règles de `CLAUDE.md`
//!
//! Aucune allocation par image (états de taille fixe) ; l'entité des poussières est créée une fois (règle 4) ; `Aabb`
//! explicite, pas de `NoFrustumCulling` (règle 6) ; la bouffée de drop passe par un `FlashLimiter`, la coquille de kick et
//! le scintillement sont coupés en respiration (règle 7) ; shaders embarqués et matériau préchauffé (règle 8).

pub mod dust;
pub mod formula;
pub mod light;
pub mod modulation;
pub mod systems;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, WarmupAppExt};

pub use dust::{Dust, DustMaterial, DustParams};
pub use light::{ATMOSPHERE_WESL, FogReach, LightTuning, SceneLight, SceneLightParams, SceneLightUniform};
pub use modulation::{FogModulation, FogState, FogTuning};
pub use systems::{AtmosphereLook, AtmosphereTuning, SceneLit};

use crate::facade::FacadeMaterial;
use crate::landmarks::render::LandmarkMaterial;
use crate::road::RoadMaterial;
use crate::signs::SignMaterial;
use crate::traffic::VehicleMaterial;
use crate::weather::WeatherSet;

systems::scene_lit!(DustMaterial);

/// Installe l'atmosphère : éclairage de scène, brouillard par fragment, poussières.
///
/// À ajouter après les plugins du monde (façades, route, enseignes, trafic, repères). Sans lui, les shaders gardent leur
/// brouillard d'avant.
#[derive(Debug, Default, Clone, Copy)]
pub struct AtmospherePlugin;

impl Plugin for AtmospherePlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins(MaterialPlugin::<DustMaterial>::default())
                .warm_material::<DustMaterial>("poussières");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                dust::embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<DustMaterial>();
        }
        app.init_resource::<AtmosphereTuning>()
            .init_resource::<AtmosphereLook>()
            .init_resource::<SceneLightUniform>()
            .init_resource::<FogReach>()
            .add_systems(Startup, dust::spawn_dust.run_if(resource_exists::<Assets<Mesh>>))
            .add_systems(
                Update,
                systems::update_scene_light.in_set(CfSet::Simulate).after(WeatherSet),
            )
            .add_systems(
                Update,
                (
                    systems::sync_scene_light::<FacadeMaterial>,
                    systems::sync_scene_light::<RoadMaterial>,
                    systems::sync_scene_light::<SignMaterial>,
                    systems::sync_scene_light::<VehicleMaterial>,
                    systems::sync_scene_light::<LandmarkMaterial>,
                    systems::sync_scene_light::<DustMaterial>,
                    systems::update_dust.run_if(resource_exists::<dust::DustRig>),
                )
                    .in_set(CfSet::Present),
            );
    }
}

#[cfg(test)]
mod tests;
