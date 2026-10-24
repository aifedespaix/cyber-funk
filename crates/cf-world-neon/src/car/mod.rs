//! La voiture de Neon City (J5-05) : la CyberCar de Sillage, ses lumières, sa traînée, la fumée de ses pneus.
//!
//! # Contenu
//!
//! - [`glb`], [`model`], [`frame`] : lecture du GLB (meshopt + WebP, que `bevy_gltf` ne sait pas lire), arbre de nœuds,
//!   repère de la voiture, position des optiques (pur, sans GPU) ;
//! - [`motion`], [`glow`], [`trail`], [`smoke`], [`paint`] : le mouvement de la caisse, la lumière, la traînée, la fumée
//!   et la teinte de la peinture (logique pure, testable seule) ;
//! - [`materials`], [`shapes`], [`spawn`], [`rig`], [`systems`], [`effects`] : leur mise en scène dans Bevy.
//!
//! # Branchement
//!
//! [`CarPlugin`] charge le modèle, puis pose la voiture sur le point suivi provisoire (`Runner`, J5-01) ou sur
//! l'entité [`Car`] que le pilote crée (J5-06). Le pilote écrit [`CarControls`] (volant, freinage, faisceau traversé) ;
//! la vitesse, la vitesse latérale et l'accélération sont mesurées sur la position absolue de la voiture. La voiture
//! écrit en retour la lueur et le freinage que lit la route ([`RoadLook`](crate::RoadLook)).
//!
//! # Licence du modèle
//!
//! La licence de la CyberCar est **inconnue** : ne pas redistribuer `assets/models/car/` hors du dépôt.

mod doc;
pub mod effects;
pub mod frame;
pub mod glb;
pub mod glow;
pub mod loader;
pub mod materials;
pub mod model;
pub mod motion;
pub mod paint;
pub mod rig;
pub mod shapes;
pub mod smoke;
pub mod spawn;
pub mod systems;
pub mod trail;

use bevy::asset::AssetApp;
use bevy::pbr::PbrPlugin;
use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, ScenePalette, ThemeLibrary, WorldSet};

pub use glb::{CarModelError, Gltf};
pub use loader::CarModelLoader;
pub use materials::{CarLightMaterial, CarPaintMaterial};
pub use model::CarModelAsset;
pub use rig::{Car, CarRig, CarState};
pub use systems::{CarControls, CarModelHandle, CarSettings, ModelQuality};

/// Installe la voiture. À ajouter après les plugins de rendu de Bevy, `CorePlugin` et le monde.
///
/// Sans `AssetPlugin` (tests de pure logique) le plugin ne fait rien.
#[derive(Debug, Default, Clone, Copy)]
pub struct CarPlugin {
    /// Version du modèle à charger.
    pub quality: ModelQuality,
}

impl Plugin for CarPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<AssetPlugin>() {
            return;
        }
        app.init_asset::<CarModelAsset>()
            .register_asset_loader(CarModelLoader)
            .insert_resource(systems::CarQuality(self.quality))
            .init_resource::<CarControls>()
            .init_resource::<CarSettings>()
            .init_resource::<crate::RoadLook>();
        if app.is_plugin_added::<PbrPlugin>() {
            app.add_plugins((
                MaterialPlugin::<CarLightMaterial>::default(),
                MaterialPlugin::<CarPaintMaterial>::default(),
            ));
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                materials::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<CarLightMaterial>().init_asset::<CarPaintMaterial>();
        }
        // Sans `PalettePlugin`, le thème de départ de Sillage (« Void Velvet ») : la voiture a besoin de ses couleurs.
        if !app.world().contains_resource::<ScenePalette>() {
            let velvet = ThemeLibrary::builtin()
                .map(|library| *library.preset(cf_show::model::PresetTheme::Velvet))
                .unwrap_or_default();
            app.insert_resource(velvet);
        }
        app.add_systems(Startup, systems::load_model)
            .add_systems(
                Update,
                (systems::build_car, systems::drive_car, systems::light_car)
                    .chain()
                    .in_set(CfSet::Simulate)
                    .after(WorldSet::Gates),
            )
            .add_systems(
                Update,
                (effects::update_trail, effects::update_smoke).in_set(CfSet::Present),
            );
    }
}
