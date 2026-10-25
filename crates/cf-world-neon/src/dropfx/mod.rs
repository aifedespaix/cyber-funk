//! Le paquet du drop vu du monde (J5-07 ; `Effects` de `fx.ts` de Sillage) : au drop, une gerbe d'étincelles jaillit de la
//! voiture et des ondes de choc se posent au sol, en cascade. Le chef du spectacle prépare le paquet pendant la montée
//! (`DropFxPackage`, J2-04) ; le monde le joue **sur la même image que `drop_now`**.
//!
//! # Qui écrit quoi
//!
//! - [`DropFx`] : les ondes, la gerbe et la voie de flashs ([`DropFlash`]). Écrit par le seul [`systems::step_dropfx`]
//!   (`CfSet::Simulate`, [`DropFxSet`]), à partir de `CurrentShowFx::drop`, présent sur la seule image du `drop_now`
//!   (`cf_render::show::publish_show_fx`, `CfSet::Show`, juste avant).
//! - [`DropRig`] et les entités : créés au démarrage, réécrits par `sync::sync_dropfx` (`CfSet::Present`) dans la même image.
//!
//! # Pulsation de la ville (`cityPulse`)
//!
//! Elle existe déjà : `DropFxPackage::city_pulse` n'est qu'un dosage du look (`gates.fx.city_pulse`) que le trafic lit
//! pour décider de lancer l'événement `city-pulse` (J7-05, `traffic::grand::city_pulse`). Ses fenêtres pulsent sur le kick
//! **que le limiteur laisse passer** (`SceneFx::kick`) et se changent en respiration quand les flashs ne sont pas permis.
//! Le drop n'en lance pas un second ici : deux écrivains pour `FacadeLook::win_pulse` ne feraient que se battre.
//!
//! # Sécurité photosensible (règle 7)
//!
//! Voir [`safety`] et [`logic`] : chaque départ (gerbe + première onde, puis chaque onde de la cascade) passe par un
//! `FlashLimiter` ; refusé, il reste sombre ; flashs coupés, respiration ou pas de spectacle, rien ne s'allume et ce qui
//! brillait redescend en `MIN_FADE` au plus. Rien ne s'allume d'un coup : montée de 0,1 s.
//!
//! # Culling et coût
//!
//! Voir [`rig`] : aucun `NoFrustumCulling`, `Aabb` exacts ou recalculés. Les `RINGS + 1` entités existent dès le démarrage,
//! aucune n'est créée en régime établi ; la gerbe réécrit ses attributs en place (aucune allocation) et seulement tant
//! qu'elle vit. Les matériaux sont déclarés à `warm_material` (règle 8).

pub mod logic;
pub mod material;
pub mod rig;
pub mod safety;
mod sync;
pub mod systems;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, WarmupAppExt, WorldSet};

pub use logic::{
    BURST_Y_M, CYCLE_LEN, DROP_RING_GAP_S, DROP_RINGS_MAX, DropFx, DropInput, PARTICLE_LIFE_S, PARTICLES, RING_LIFE_S,
    RINGS, Ring,
};
pub use material::{DropRingMaterial, DropSparkMaterial, RingParams};
pub use rig::{DropPart, DropPiece, DropRig};
pub use safety::{DropFlash, GRANTED_GAIN, UNGRANTED_GAIN};

/// L'étape où le paquet du drop avance (`CfSet::Simulate`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DropFxSet;

/// Installe le paquet du drop : matériaux et shaders, pool d'entités, le système qui le joue et celui qui le montre.
///
/// À ajouter après les plugins de rendu de Bevy et [`NeonWorldPlugin`](crate::NeonWorldPlugin) (la voiture porte
/// `StreamFocus`). Sans rendu (tests), seuls les matériaux sont déclarés et la logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct DropFxPlugin;

impl Plugin for DropFxPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins((
                MaterialPlugin::<DropRingMaterial>::default(),
                MaterialPlugin::<DropSparkMaterial>::default(),
            ))
            // Cachés au repos, ils ne compileraient qu'au premier drop en plein show sans cette préchauffe.
            .warm_material::<DropRingMaterial>("drop : ondes de choc")
            .warm_material::<DropSparkMaterial>("drop : gerbe");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<DropRingMaterial>().init_asset::<DropSparkMaterial>();
        }
        app.init_resource::<DropFx>()
            .add_systems(Startup, rig::spawn_dropfx.run_if(resource_exists::<Assets<Mesh>>))
            .add_systems(
                Update,
                systems::step_dropfx
                    .in_set(DropFxSet)
                    .in_set(CfSet::Simulate)
                    .after(WorldSet::Chunks),
            )
            .add_systems(
                Update,
                sync::sync_dropfx
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<DropRig>),
            );
    }
}

#[cfg(test)]
mod tests;
