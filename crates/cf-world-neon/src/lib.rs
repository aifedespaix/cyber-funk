//! Neon City, le monde repris de Sillage : route infinie, immeubles néon, ciel et planètes, voiture en pilote
//! automatique, trafic, cascades, enseignes et événements. Implémente le trait `World` de `cf-render`.
//!
//! Contenu actuel (J5-01, J5-02, J5-03, J5-04, J5-05, J5-06) :
//!
//! - [`road`] : la route (plan unique, shader WESL, réaction au pouls et aux basses, teinte d'ambiance) ;
//! - [`route`] : le plan de route (ponts, planificateur du pilote), pur et déterministe ;
//! - [`scenery`] : la ville (immeubles par tronçon, arches, collisions) ;
//! - [`sky`] : le ciel (voûte, étoiles, aurores, astres, géante à anneau, étoiles filantes), couche arrière-plan ;
//! - [`meteors`] (J7-10) : les astres qui tombent, leurs explosions contre la ville et leurs anneaux dans la brume ;
//! - [`signs`] (J7-01) : les enseignes de la route (textures 2D cuites à l'ouverture, bandeaux, néons, caméras) ;
//! - [`facade`] : le matériau de façade (fenêtres, néons, pulsation sur les temps) et son shader ;
//! - [`weather`] (J5-04) : l'atmosphère (brume sous la route, pluie, météo, éclairs d'orage derrière le limiteur) ;
//! - [`world`] : [`NeonWorld`], l'implémentation du trait `World` (streaming) ;
//! - [`chase`] : caméra de poursuite provisoire, en attendant le réalisateur (J6), et point suivi provisoire pour les
//!   exemples sans pilote ;
//! - [`cinema`] (J6-01) : le branchement sur le réalisateur (collisions, `CinemaWorld` de chaque image) ;
//! - [`pilot`] (J5-06) : le pilote automatique (allure, chorégraphie, vol, esquives) et [`CarAnimation`] ;
//! - [`stunts`] (J7-06) : le directeur des cascades (portes, tirage, pose fondue sur le pilote) et le registre qui
//!   attend les scripts de J7-07 ;
//! - [`landmarks`] (J7-08) : les immeubles-vitrines (mur de code, lave, ERROR, arcade, carte mère, arche, mecha), le champ de
//!   lumières qu'ils partagent avec les façades et la route, et l'événement `landmark` du trafic ;
//! - [`traffic`] (J7-03) : le trafic (véhicules volants, voitures au sol, registre d'événements pour J7-04 et J7-05) ;
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
pub mod landmarks;
pub mod meteors;
pub mod pilot;
pub mod rng;
pub mod road;
pub mod route;
pub mod scenery;
pub mod signs;
pub mod sky;
pub mod stunts;
pub mod traffic;
pub mod weather;
pub mod world;

use bevy::prelude::*;
use cf_render::WorldPlugin;

pub use car::{Car, CarControls, CarPlugin};
pub use chase::{ChaseCameraPlugin, ChasePlugin, Runner, RunnerConfig, chase_pose};
pub use cinema::{CinemaInput, NeonCinemaPlugin, fill_cinema_world};
pub use facade::{FacadeLook, FacadeMaterial, FacadePlugin};
pub use landmarks::{CityField, LandmarkHandle, LandmarkKind, LandmarksPlugin, StageLandmark};
pub use meteors::{MeteorSet, Meteors, MeteorsPlugin};
pub use pilot::{CarAnimation, Pilot, PilotPlugin, PilotSettings, PilotTraffic};
pub use road::{Road, RoadLook, RoadMaterial, RoadPalette, RoadPlugin};
pub use route::{RoutePlan, RoutePlanner};
pub use scenery::{Clearing, SceneryConfig, SpotAnchor};
pub use signs::SignsPlugin;
pub use sky::{RingPlanet, ShootingStarRequest, SkyLook, SkyPlugin};
pub use stunts::{StuntDirector, StuntKind, StuntOrders, StuntRegistry, StuntSet, StuntsPlugin};
pub use traffic::{TrafficAppExt, TrafficEvent, TrafficPlugin, TrafficSettings, TrafficStats};
pub use weather::{Thunder, WeatherPlugin, WeatherSettings, WeatherState};
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
            MeteorsPlugin,
            SignsPlugin::default(),
            LandmarksPlugin::default(),
            WeatherPlugin,
        ))
        .init_resource::<RoutePlan>()
        .init_resource::<RoutePlanner>()
        .add_message::<cf_render::QualityChanged>()
        .add_systems(
            Update,
            apply_quality
                .in_set(cf_render::CfSet::Simulate)
                .before(cf_render::WorldSet::Chunks),
        )
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

/// `CfSet::Simulate`, avant les tronçons : un nouveau niveau de qualité retire (ou rend) les couronnes lointaines de la
/// ville, puis les tronçons vivants sont re-tirés. Les tronçons à venir entrent de toute façon au loin, dans la brume.
fn apply_quality(
    mut changes: MessageReader<cf_render::QualityChanged>,
    mut world: ResMut<NeonWorld>,
    mut stream: ResMut<cf_render::ChunkStream<NeonWorld>>,
) {
    // Seul le dernier niveau compte si plusieurs changements tombent dans la même image.
    let Some(last) = changes.read().last() else { return };
    if world.set_quality(last.to) {
        stream.replan_all();
    }
}
