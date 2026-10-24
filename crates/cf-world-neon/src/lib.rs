//! Neon City, le monde repris de Sillage : route infinie, immeubles néon, ciel et planètes, voiture en pilote
//! automatique, trafic, cascades, enseignes et événements. Implémente le trait `World` de `cf-render`.
//!
//! Contenu actuel (J5-01, J5-02, J5-03, J5-05, J5-06) :
//!
//! - [`road`] : la route (plan unique, shader WESL, réaction au pouls et aux basses, teinte d'ambiance) ;
//! - [`route`] : le plan de route (ponts, planificateur du pilote), pur et déterministe ;
//! - [`scenery`] : la ville (immeubles par tronçon, arches, collisions) ;
//! - [`sky`] : le ciel (voûte, étoiles, aurores, astres, géante à anneau, étoiles filantes), couche arrière-plan ;
//! - [`facade`] : le matériau de façade (fenêtres, néons, pulsation sur les temps) et son shader ;
//! - [`world`] : [`NeonWorld`], l'implémentation du trait `World` (streaming) ;
//! - [`chase`] : caméra de poursuite provisoire, en attendant le réalisateur (J6), et point suivi provisoire pour les
//!   exemples sans pilote ;
//! - [`cinema`] (J6-01) : le branchement sur le réalisateur (collisions, `CinemaWorld` de chaque image) ;
//! - [`pilot`] (J5-06) : le pilote automatique (allure, chorégraphie, vol, esquives) et [`CarAnimation`] ;
//! - [`car`] (J5-05) : la voiture (`CyberCar` de Sillage), ses lumières, sa traînée, la fumée de ses pneus. Elle prend la
//!   place du pavé du point suivi provisoire.
//!
//! # Couches de rendu
//!
//! Toute entité du monde porte [`WorldLayer`](cf_render::WorldLayer) (couche 1), les lumières leurs `RenderLayers`
//! (`Layer::World.render_layers()`) : Bevy ne propage pas ces marqueurs aux enfants, et une entité sans marqueur est
//! invisible pour la caméra du monde. Les pièces que les tronçons créent (J5-02) en héritent par le bundle qu'on
//! déclare à `PieceRegistry::add` : y mettre `WorldLayer`.

pub mod car;
pub mod chase;
pub mod cinema;
pub mod facade;
pub mod pilot;
pub mod rng;
pub mod road;
pub mod route;
pub mod scenery;
pub mod sky;
pub mod world;

use bevy::prelude::*;
use cf_render::WorldPlugin;

pub use car::{Car, CarControls, CarPlugin};
pub use chase::{ChaseCameraPlugin, ChasePlugin, Runner, RunnerConfig, chase_pose};
pub use cinema::{CinemaInput, NeonCinemaPlugin, fill_cinema_world};
pub use facade::{FacadeLook, FacadeMaterial, FacadePlugin};
pub use pilot::{CarAnimation, Pilot, PilotPlugin, PilotSettings, PilotTraffic};
pub use road::{Road, RoadLook, RoadMaterial, RoadPalette, RoadPlugin};
pub use route::{RoutePlan, RoutePlanner};
pub use scenery::{Clearing, SceneryConfig, SpotAnchor};
pub use sky::{RingPlanet, ShootingStarRequest, SkyLook, SkyPlugin};
pub use world::NeonWorld;

/// Plugin du monde Neon City : le monde streamé, la route et le plan de route.
///
/// Par défaut il ajoute aussi la caméra de poursuite provisoire, la voiture et son pilote automatique ([`PilotPlugin`]) :
/// de quoi voir le monde seul. Quand le réalisateur existe, [`NeonWorldPlugin::embedded`] retire la caméra, la voiture
/// et le pilote : l'application les ajoute (le pilote crée la voiture).
///
/// À ajouter après `CorePlugin` et les plugins de rendu de Bevy.
#[derive(Debug, Clone, Copy)]
pub struct NeonWorldPlugin {
    /// Ajoute la caméra de poursuite provisoire, la voiture et son pilote ([`ChaseCameraPlugin`], [`PilotPlugin`]).
    pub standalone: bool,
    /// Affiche la ville (immeubles et arches). Faux : la route seule ([`NeonWorld::without_city`]).
    pub city: bool,
}

impl Default for NeonWorldPlugin {
    fn default() -> Self {
        Self {
            standalone: true,
            city: true,
        }
    }
}

impl NeonWorldPlugin {
    /// Le monde seul : ni point suivi ni caméra, c'est l'application (voiture, réalisateur) qui les fournit.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            standalone: false,
            city: true,
        }
    }

    /// Sans ville : la route seule (captures de référence de la route, `road_demo`).
    #[must_use]
    pub fn without_city(mut self) -> Self {
        self.city = false;
        self
    }
}

impl Plugin for NeonWorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            WorldPlugin::new(if self.city {
                NeonWorld::default()
            } else {
                NeonWorld::default().without_city()
            }),
            RoadPlugin,
            FacadePlugin,
            SkyPlugin,
        ))
        .init_resource::<RoutePlan>()
        .init_resource::<RoutePlanner>()
        .add_systems(
            Update,
            route::update_route_plan
                .in_set(cf_render::CfSet::Simulate)
                .after(cf_render::WorldSet::Chunks),
        )
        .add_systems(
            Update,
            scenery::arches::update_arches
                .in_set(cf_render::CfSet::Simulate)
                .after(route::update_route_plan)
                .run_if(resource_exists::<scenery::arches::ArchPool>),
        );
        if self.standalone {
            // Le pilote crée la voiture (elle porte `StreamFocus`) : pas de point suivi provisoire avec lui.
            app.add_plugins((ChaseCameraPlugin, PilotPlugin::default(), CarPlugin::default()));
        }
    }
}
