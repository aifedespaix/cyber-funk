//! L'atmosphère de Neon City (J5-04 ; `mist.ts`, `rain.ts`, `weather.ts` de Sillage) : la brume sous la route, la pluie qui
//! tombe devant la caméra, et la météo qui décide quand il pleut.
//!
//! - [`logic`] : le cycle des averses ([`Weather`]), pur et déterministe (RNG à graine) ;
//! - [`mist`] : la nappe de brouillard (un plan, un shader) ;
//! - [`rain`] : la pluie (2 200 gouttes, un maillage statique, tout dans le vertex shader) et ses compteurs de coût ;
//! - [`storm`] : les éclairs d'orage, derrière le limiteur photosensible (règle 7) ;
//! - [`systems`] : les systèmes et les ressources.
//!
//! # Qui écrit quoi
//!
//! - [`WeatherState`] : écrit par le seul `step_weather` (`CfSet::Simulate`, [`WeatherSet`]). Il publie l'humidité et la
//!   pluie dans [`RoadLook`](crate::RoadLook) (route et façades, une seule source) et dans `LensWeather` (gouttes sur
//!   l'objectif, post-process de `cf-render`).
//! - [`Thunder`] : message d'une image quand le tonnerre gronde, pour le son à venir.
//!
//! # Portes du spectacle
//!
//! Sillage règle les averses sur le niveau du spectacle (`SHOW_RAIN`) : jamais en pré-intro (niveau 0), plus fréquentes et
//! avec des orages aux niveaux 4 et 5. Les portes (`ShowGates`) n'ont pas de porte de pluie ; le niveau est celui de
//! `ShowState::level`, que le look pilote. Flashs coupés (`FlashKill`) ou respiration : aucun éclair, la pluie continue.
//!
//! # Qualité adaptative
//!
//! La pluie lit `QualityLevers::rain_density` (et le mode calme), la brume `QualityLevers::particle_density`. Voir
//! [`rain`] pour le coût de la pluie.
//!
//! # Règles de `CLAUDE.md`
//!
//! Les deux entités sont créées une fois au démarrage (règle 4) ; aucun `NoFrustumCulling` (règle 6, voir [`mist`] et
//! [`rain`]) ; les éclairs passent par le limiteur (règle 7) ; les matériaux sont déclarés à `warm_material` (règle 8) et
//! les shaders embarqués dans le binaire.

pub mod logic;
pub mod mist;
pub mod rain;
pub mod storm;
pub mod systems;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, WarmupAppExt, WorldSet};

pub use logic::{RainPlan, Weather, show_rain_plan};
pub use mist::{Mist, MistMaterial, MistParams};
pub use rain::{DROPS, Rain, RainMaterial, RainParams};
pub use storm::Lightning;
pub use systems::{Thunder, WeatherSet, WeatherSettings, WeatherState, rain_density, rain_tilt};

/// Installe l'atmosphère : matériaux et shaders, brume, pluie, météo.
///
/// À ajouter après les plugins de rendu de Bevy et `RoadPlugin` (`RoadLook`). Sans rendu (tests), seuls les matériaux sont
/// déclarés et la logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins((MaterialPlugin::<MistMaterial>::default(), MaterialPlugin::<RainMaterial>::default()))
                // Cachée sans averse, la pluie ne compilerait qu'à la première goutte en plein show sans cette préchauffe.
                .warm_material::<MistMaterial>("brume")
                .warm_material::<RainMaterial>("pluie");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                mist::embed_shader(&mut shaders);
                rain::embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<MistMaterial>().init_asset::<RainMaterial>();
        }
        app.init_resource::<WeatherSettings>()
            .init_resource::<crate::RoadLook>()
            .init_resource::<cf_render::postfx::LensWeather>()
            .insert_resource(WeatherState::default())
            .add_message::<Thunder>()
            .add_systems(
                Startup,
                (mist::spawn_mist, rain::spawn_rain).run_if(resource_exists::<Assets<Mesh>>),
            )
            .add_systems(
                Update,
                (
                    systems::step_weather.in_set(WeatherSet),
                    systems::follow_mist.after(WorldSet::Chunks),
                )
                    .in_set(CfSet::Simulate),
            )
            .add_systems(
                Update,
                (
                    systems::update_mist.run_if(resource_exists::<mist::MistRig>),
                    systems::update_rain.run_if(resource_exists::<rain::RainRig>),
                )
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<cf_render::ScenePalette>),
            );
    }
}

#[cfg(test)]
mod tests;
