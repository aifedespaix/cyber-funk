//! Neon City, le monde repris de Sillage : route infinie, immeubles néon, ciel et planètes, voiture en pilote
//! automatique, trafic, cascades, enseignes et événements. Implémente le trait `World` de `cf-render`.
//!
//! Contenu actuel (J5-01) :
//!
//! - [`road`] : la route (plan unique, shader WESL, réaction au pouls et aux basses, teinte d'ambiance) ;
//! - [`route`] : le plan de route (ponts, planificateur du pilote), pur et déterministe ;
//! - [`world`] : [`NeonWorld`], l'implémentation du trait `World` (streaming) ;
//! - [`chase`] : point suivi et caméra de poursuite provisoires, en attendant la voiture (J5-05) et le réalisateur (J6).
//!
//! # Couches de rendu
//!
//! Toute entité du monde porte [`WorldLayer`](cf_render::WorldLayer) (couche 1), les lumières leurs `RenderLayers`
//! (`Layer::World.render_layers()`) : Bevy ne propage pas ces marqueurs aux enfants, et une entité sans marqueur est
//! invisible pour la caméra du monde. Les pièces que les tronçons créent (J5-02) en héritent par le bundle qu'on
//! déclare à `PieceRegistry::add` : y mettre `WorldLayer`.

pub mod car;
pub mod chase;
pub mod rng;
pub mod road;
pub mod route;
pub mod world;

use bevy::prelude::*;
use cf_render::WorldPlugin;

pub use chase::{ChasePlugin, Runner, RunnerConfig, chase_pose};
pub use road::{Road, RoadLook, RoadMaterial, RoadPalette, RoadPlugin};
pub use route::{RoutePlan, RoutePlanner};
pub use world::NeonWorld;

/// Plugin du monde Neon City : le monde streamé, la route et le plan de route.
///
/// Par défaut il ajoute aussi le point suivi et la caméra de poursuite provisoires ([`ChasePlugin`]) : de quoi voir le
/// monde seul. Quand la voiture et le réalisateur existent, [`NeonWorldPlugin::embedded`] les retire.
///
/// À ajouter après `CorePlugin` et les plugins de rendu de Bevy.
#[derive(Debug, Clone, Copy)]
pub struct NeonWorldPlugin {
    /// Ajoute [`ChasePlugin`] (point suivi et caméra provisoires).
    pub standalone: bool,
}

impl Default for NeonWorldPlugin {
    fn default() -> Self {
        Self { standalone: true }
    }
}

impl NeonWorldPlugin {
    /// Le monde seul : ni point suivi ni caméra, c'est l'application (voiture, réalisateur) qui les fournit.
    #[must_use]
    pub fn embedded() -> Self {
        Self { standalone: false }
    }
}

impl Plugin for NeonWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((WorldPlugin::new(NeonWorld::default()), RoadPlugin))
            .init_resource::<RoutePlan>()
            .init_resource::<RoutePlanner>()
            .add_systems(
                Update,
                route::update_route_plan
                    .in_set(cf_render::CfSet::Simulate)
                    .after(cf_render::WorldSet::Chunks),
            );
        if self.standalone {
            app.add_plugins(ChasePlugin::default());
        }
    }
}
