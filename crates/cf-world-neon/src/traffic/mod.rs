//! Le trafic de Neon City, socle (J7-03) : portage de `traffic/` de Sillage. La ville ne s'arrête jamais de vivre :
//! des véhicules volants dans quatre couloirs aériens, des voitures au sol à dépasser, et par-dessus des événements
//! (poursuites, convois, motos…) que les lots J7-04 et J7-05 écrivent sur ce socle.
//!
//! # Contenu
//!
//! - [`director`] et [`scheduler`] : le chef d'orchestre et le tirage des événements, dans le respect des portes du look
//!   (`events`, `eventRate`, `groundTraffic`) ; [`names`] : le catalogue (poids, familles, humeur par section) ;
//! - [`event`] : le contrat [`TrafficEvent`] et le registre qui fabrique les événements ([`EventRegistry`],
//!   [`TrafficAppExt::register_traffic_event`]) ;
//! - [`fleet`], [`motion`], [`variant`] : les véhicules (pool de 220, `Motion` sans fermeture), leurs trajectoires ;
//! - [`air_lanes`], [`ground`] : le trafic de fond et le trafic au sol ;
//! - [`glints`], [`walls`], [`bike_patterns`] : les points lumineux, les murs de lumière des motos, les figures des motos ;
//! - [`flash`] : les limiteurs de flashs du trafic (règle 7) ;
//! - [`model`], [`render`], [`systems`] : les maillages (GLB de Kenney, moto procédurale), les entités préallouées et le
//!   branchement sur Bevy.
//!
//! # Ce qui n'est pas ici
//!
//! Les événements eux-mêmes (J7-04, J7-05), et ce qui ne sert qu'à eux : portiques et rideaux d'hologramme
//! (`portals.ts`, `hologram.ts`), blocs d'immeubles posés à la main (`block-set.ts`). Le registre est prêt à les
//! recevoir.
//!
//! # Branchement
//!
//! [`TrafficPlugin`] ajoute le directeur, les matériaux et les entités. Il remplit [`PilotTraffic`](crate::PilotTraffic)
//! avant le pilote (`TrafficSet::Publish`), avance le trafic après la cadence de simulation (`TrafficSet::Step`) et
//! donne ses sujets au réalisateur par `NeonWorld::subjects`.
//!
//! # Règles tenues
//!
//! - aucune entité créée après le démarrage : le pool est préalloué (212 véhicules, 220 traînées, 2 maillages
//!   dynamiques) ; aucune allocation par image dans la flotte, les couloirs, le trafic au sol, les points et les murs ;
//! - la cadence de chaque véhicule suit le `SimulationLod` de son entité : hors champ, un tour sur huit, le temps est
//!   rattrapé au tour suivant ([`FleetStats`]) ;
//! - la qualité adaptative espace les naissances et réduit le trafic au sol (`QualityLevers::particle_density`) ;
//! - tout éclat (pouls des néons, gyrophares) passe par le limiteur de flashs ([`TrafficFlash`]).
//!
//! L'événement tiré par `Box<dyn TrafficEvent>` alloue une fois à son démarrage (toutes les quinze à trente-quatre
//! secondes), jamais par image.

pub mod air_lanes;
pub mod bike_patterns;
pub mod director;
pub mod event;
pub mod flash;
pub mod fleet;
pub mod frame;
pub mod glints;
pub mod ground;
pub mod model;
pub mod motion;
pub mod names;
pub mod render;
pub mod scheduler;
pub mod systems;
pub mod variant;
pub mod walls;

use std::path::PathBuf;

use bevy::pbr::PbrPlugin;
use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, CullingSet, ScenePalette, ThemeLibrary, WarmupAppExt, WorldSet};

pub use director::TrafficDirector;
pub use event::{EventCtx, EventFactory, EventRegistry, Focus, TrafficAppExt, TrafficEvent};
pub use flash::TrafficFlash;
pub use fleet::{Craft, CraftId, Fleet, FleetStats};
pub use frame::{TrafficBeat, TrafficDrive, TrafficFrame, TrafficShow};
pub use names::EventName;
pub use render::{
    GlintMaterial, TrafficBody, TrafficEntities, TrafficSource, TrafficTrail, TrailMaterial, VehicleMaterial,
    WallMaterial,
};
pub use systems::{TrafficSettings, TrafficStats};

use crate::pilot::PilotSet;

/// Les étapes du trafic dans l'image, toutes en `CfSet::Simulate` (sauf le rendu, en `Present`).
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrafficSet {
    /// Avant le pilote : ce que le trafic a décidé à l'image précédente est publié dans `PilotTraffic`.
    Publish,
    /// Après la cadence de simulation du culling : le trafic avance d'une image.
    Step,
    /// Après le pas : la pose des véhicules est recopiée dans leurs entités.
    Sync,
}

/// Installe le trafic. À ajouter après `CorePlugin`, `CullingPlugin`, le monde ([`NeonWorldPlugin`](crate::NeonWorldPlugin))
/// et le pilote ([`PilotPlugin`](crate::PilotPlugin)).
///
/// Les modèles sont lus dans `assets_dir/models/crafts`, comme la voiture lit le sien : par un chemin relatif au dossier
/// de travail, pas par l'`AssetServer`. Un modèle absent est remplacé par un engin de secours.
///
/// Sans `PbrPlugin` (tests de logique), les matériaux sont seulement déclarés comme assets : aucun rendu.
#[derive(Debug, Clone)]
pub struct TrafficPlugin {
    /// Dossier `assets/` du dépôt.
    pub assets_dir: PathBuf,
    /// Réglages de départ.
    pub settings: TrafficSettings,
}

impl Default for TrafficPlugin {
    fn default() -> Self {
        Self {
            assets_dir: "assets".into(),
            settings: TrafficSettings::default(),
        }
    }
}

impl Plugin for TrafficPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<AssetPlugin>() {
            return;
        }
        app.insert_resource(self.settings)
            .insert_resource(TrafficDirector::new(self.settings.seed))
            .insert_resource(TrafficSource {
                models_dir: self.assets_dir.join("models/crafts"),
            })
            .init_resource::<EventRegistry>()
            .init_resource::<TrafficFlash>()
            .init_resource::<TrafficStats>()
            .init_resource::<crate::PilotTraffic>()
            .init_resource::<crate::CarAnimation>();
        if app.is_plugin_added::<PbrPlugin>() {
            app.add_plugins((
                MaterialPlugin::<VehicleMaterial>::default(),
                MaterialPlugin::<TrailMaterial>::default(),
                MaterialPlugin::<GlintMaterial>::default(),
                MaterialPlugin::<WallMaterial>::default(),
            ))
            .warm_material::<VehicleMaterial>("trafic : véhicules")
            .warm_material::<TrailMaterial>("trafic : traînées")
            .warm_material::<GlintMaterial>("trafic : points lumineux")
            .warm_material::<WallMaterial>("trafic : murs de lumière");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                render::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<VehicleMaterial>()
                .init_asset::<TrailMaterial>()
                .init_asset::<GlintMaterial>()
                .init_asset::<WallMaterial>();
        }
        // Sans `PalettePlugin`, le thème de départ de Sillage (« Void Velvet ») : les véhicules ont besoin de ses couleurs.
        if !app.world().contains_resource::<ScenePalette>() {
            let velvet = ThemeLibrary::builtin()
                .map(|library| *library.preset(cf_show::model::PresetTheme::Velvet))
                .unwrap_or_default();
            app.insert_resource(velvet);
        }
        app.configure_sets(
            Update,
            (TrafficSet::Publish, TrafficSet::Step, TrafficSet::Sync)
                .chain()
                .in_set(CfSet::Simulate),
        )
        .add_systems(Startup, render::setup_traffic.run_if(resource_exists::<Assets<Mesh>>))
        .add_systems(
            Update,
            (
                systems::publish_traffic.in_set(TrafficSet::Publish).before(PilotSet),
                (systems::drive_traffic, systems::apply_clearings)
                    .chain()
                    .in_set(TrafficSet::Step)
                    .after(CullingSet::Lod)
                    .after(WorldSet::Chunks),
                render::sync_fleet
                    .in_set(TrafficSet::Sync)
                    .run_if(resource_exists::<TrafficEntities>),
                (
                    systems::update_flash,
                    render::update_materials.run_if(resource_exists::<TrafficEntities>),
                    render::sync_dynamic.run_if(resource_exists::<TrafficEntities>),
                )
                    .chain()
                    .in_set(CfSet::Present),
            ),
        );
    }
}
