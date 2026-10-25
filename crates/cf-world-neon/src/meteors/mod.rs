//! Les astres qui tombent et leurs impacts (J7-10 ; `meteors.ts`, `meteor-blast.ts`, `meteor-materials.ts` de Sillage) :
//! des blocs texturés (les textures des planètes du ciel) surgissent haut dans le ciel, passent au-dessus de la caméra en
//! diagonale et filent vers la nappe de brouillard, loin devant la voiture. Chacun traîne une queue lumineuse et un halo
//! de chaleur. Contre un immeuble il explose (éclair, onde de choc, gerbe de débris) ; dans la brume il s'éteint dans un
//! anneau.
//!
//! # Qui écrit quoi
//!
//! - [`Meteors`] : le pool d'astres ([`MeteorField`]), les explosions ([`Blasts`]) et la voie des éclairs d'impact
//!   ([`ImpactFlash`]). Écrit par le seul [`systems::step_meteors`] (`CfSet::Simulate`, [`MeteorSet`]).
//! - [`MeteorRig`] et les entités : créés au démarrage, réécrits par `sync::sync_meteors` (`CfSet::Present`).
//! - Les **sujets** (`SubjectKind::Meteor`) sont lus par [`NeonWorld`](crate::NeonWorld) dans `WorldSet::Subjects`
//!   ([`Meteors::collect_subjects`]) : les plans `meteor-ride` et `meteor-impact-watch` s'y accrochent par l'identifiant.
//! - `CinemaWorld::last_blast` (`lastBlast` de Sillage) est rempli par `build_cinema_world` d'après
//!   [`Meteors::last_blast`].
//! - `LaunchMeteor` (`launchMeteor` de Sillage) : le plan `meteor-ride` le demande par le réalisateur, le monde le lit ici
//!   et lance l'astre ; sans lui le plan s'arrêtait seul après 1,2 s. Tant que l'astre vit, `Shot::holding` du plan
//!   (et de `meteor-impact-watch`) retient la coupe : l'explosion est publiée par `last_blast`.
//!
//! # Portes du spectacle
//!
//! `gates.meteor_rate` (lancers par minute, 0 : aucun ; 0,4 × en respiration) et `gates.meteor_shower` (une pluie de 6 à
//! 12 astres sur 4 à 6,5 s part parfois sur un drop, au plus une par 100 s, jamais en respiration ni en mode calme).
//! Sans spectacle, la cadence libre de Sillage (un astre toutes les 2 à 6 s).
//!
//! # Sécurité photosensible (règle 7)
//!
//! Tout éclair d'impact passe par un [`FlashLimiter`](cf_show::limiter::FlashLimiter) : voir [`flash`]. Rien ne s'allume
//! d'un coup : la lueur d'un astre, l'onde de choc et la gerbe montent en 0,1 s au moins.
//!
//! # Culling et coût
//!
//! Voir [`rig`] : aucun `NoFrustumCulling`, `Aabb` exacts ou recalculés. Les 66 entités du pool existent dès le
//! démarrage, aucune n'est créée en régime établi ; les matériaux ne sont réécrits que s'ils changent ; la gerbe réécrit
//! ses attributs en place (aucune allocation). Les matériaux sont déclarés à `warm_material` (règle 8).

pub mod blast;
pub mod field;
pub mod flash;
pub mod material;
pub mod rig;
mod sync;
pub mod systems;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, WarmupAppExt, WorldSet};

pub use blast::{BLASTS, Blast, Blasts, DEBRIS};
pub use field::{
    BlastRecord, FieldEvent, FieldInput, HEAT_COUNT, MIST_Y, Meteor, MeteorField, POOL, ShowGate, Splash,
};
pub use flash::ImpactFlash;
pub use material::{MeteorDebrisMaterial, MeteorFxMaterial, MeteorSurfaceMaterial};
pub use rig::{MeteorPart, MeteorRig, Piece};
pub use sync::{billboard, trail_pose};
pub use systems::Meteors;

pub use crate::sky::PlanetTextures;

/// L'étape où les astres avancent (`CfSet::Simulate`) : `build_cinema_world` la lit, le `CinemaWorld` de l'image montre
/// donc le dernier impact de l'image.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MeteorSet;

/// Installe les astres : matériaux et shaders, pool d'entités, le système qui les fait tomber et celui qui les montre.
///
/// À ajouter après les plugins de rendu de Bevy et [`SkyPlugin`](crate::SkyPlugin) (les textures des planètes viennent du
/// ciel : sans elles rien ne tombe). Sans rendu (tests), seuls les matériaux sont déclarés et la logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct MeteorsPlugin;

impl Plugin for MeteorsPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins((
                MaterialPlugin::<MeteorSurfaceMaterial>::default(),
                MaterialPlugin::<MeteorFxMaterial>::default(),
                MaterialPlugin::<MeteorDebrisMaterial>::default(),
            ))
            // Cachés au repos, ils ne compileraient qu'au premier astre en plein show sans cette préchauffe.
            .warm_material::<MeteorSurfaceMaterial>("astres : surface")
            .warm_material::<MeteorFxMaterial>("astres : queue, halo, éclairs, ondes")
            .warm_material::<MeteorDebrisMaterial>("astres : débris");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<MeteorSurfaceMaterial>()
                .init_asset::<MeteorFxMaterial>()
                .init_asset::<MeteorDebrisMaterial>();
        }
        app.init_resource::<Meteors>()
            .add_message::<cf_cinema::world::LaunchMeteor>()
            .add_systems(Startup, rig::spawn_meteors.run_if(resource_exists::<Assets<Mesh>>))
            .add_systems(
                Update,
                systems::step_meteors
                    .in_set(MeteorSet)
                    .in_set(CfSet::Simulate)
                    .after(WorldSet::Chunks),
            )
            .add_systems(
                Update,
                sync::sync_meteors
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<MeteorRig>)
                    .run_if(resource_exists::<cf_render::ScenePalette>),
            );
    }
}

#[cfg(test)]
mod tests;
