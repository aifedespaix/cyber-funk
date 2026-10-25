//! Les enseignes de la route (J7-01) : portiques, potences, panneaux de toit, hologrammes, projections, néons et écrans-caméra
//! (`engine/signs/` de Sillage).
//!
//! # Du `<canvas>` repeint à l'atlas cuit
//!
//! Sillage dessinait chaque enseigne dans un canevas 2D, repeint à chaque image tant qu'elle était proche. Ici, **tout est
//! dessiné une fois, à l'ouverture** (règle 8 de `CLAUDE.md`), et le matériau anime ce qui se calculait par image :
//!
//! - [`paint`] : la cuisson CPU, `tiny-skia` pour les formes et `cosmic-text` pour le texte (polices du dépôt) ;
//! - [`atlas`] : les cellules (512 × 256, quatre canaux de couverture) et les bandeaux, rangés en pages, cuits en parallèle
//!   dans une tâche de fond inscrite au carnet de chargement ;
//! - [`material`] et `sign.wesl` : un seul pipeline (alpha prémultiplié) ; le shader pose la palette vivante sur les
//!   canaux, ajoute le voile d'accent, le bandeau défilant, le cadre, la trame et le flux des caméras ;
//! - [`road`] : le chef d'orchestre (qui est où, fondus, bug des portiques, radar), pur et déterministe ;
//! - [`flash`] : tout ce qui brille d'un coup passe par le `FlashLimiter` (règle 7) ;
//! - [`structure`], [`ticker`], [`formats`], [`ads`], [`show_signs`], [`icons`] : portages directs, testés contre les
//!   données de référence de Sillage (`tests/golden/signs.json`).
//!
//! # Couches, culling, coût
//!
//! Onze unités créées au démarrage, cachées au repos, jamais détruites ; chacune est une racine à position absolue
//! (origine flottante) dont les enfants portent `WorldLayer`. Les `Aabb` sont ceux des maillages (aucun `NoFrustumCulling`).
//! Un panneau hors portée est caché (`Visibility::Hidden`), et son matériau n'est plus réécrit.

pub mod ads;
pub mod atlas;
pub mod audio;
pub mod bake;
pub mod flash;
pub mod formats;
pub mod icons;
pub mod lookup;
pub mod material;
pub mod paint;
pub mod rig;
pub mod road;
pub mod show_signs;
pub mod structure;
pub mod systems;
pub mod ticker;

#[cfg(test)]
mod golden;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, ScenePalette, ThemeLibrary, WarmupAppExt, WorldSet};

pub use ads::{Ad, AdId, AdKind, CATALOGUE};
pub use atlas::{BakedAtlas, CellKey};
pub use bake::SignAtlas;
pub use formats::{FormatId, Look, Mount, format_at};
pub use material::{SignMaterial, SignParams};
pub use road::{CamShot, SignAd, SignRoad, Unit};
pub use show_signs::{SignShow, SlotNames};
pub use systems::{SignFrame, SignObstructions, SignRoadRes};

/// Installe les enseignes : matériau, atlas, route et rendu.
///
/// À ajouter après les plugins de rendu de Bevy : le shader est embarqué dans `Assets<Shader>` à la construction. **Sans
/// rendu (tests)** il ne fait rien d'autre que déclarer ses ressources et son matériau (comme les autres plugins du
/// monde) ; [`SignsPlugin::always`] force l'installation complète, cuisson comprise.
#[derive(Debug, Default, Clone, Copy)]
pub struct SignsPlugin {
    /// Installer aussi sans rendu (tests d'intégration).
    pub force: bool,
}

impl SignsPlugin {
    /// Installation complète même sans moteur de rendu.
    #[must_use]
    pub const fn always() -> Self {
        Self { force: true }
    }
}

impl Plugin for SignsPlugin {
    fn build(&self, app: &mut App) {
        let rendering = app.is_plugin_added::<bevy::pbr::PbrPlugin>();
        if rendering {
            app.add_plugins(MaterialPlugin::<SignMaterial>::default())
                .warm_material::<SignMaterial>("enseignes");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<SignMaterial>();
        }
        if !app.world().contains_resource::<ScenePalette>() {
            // Sans `PalettePlugin` (exemples, tests) : le thème de départ de Sillage.
            app.insert_resource(ThemeLibrary::builtin().map_or_else(
                |_| ScenePalette::default(),
                |library| *library.preset(cf_show::model::PresetTheme::Velvet),
            ));
        }
        app.init_resource::<SignShow>()
            .init_resource::<SignRoadRes>()
            .init_resource::<SignFrame>()
            .init_resource::<SignObstructions>()
            .init_resource::<systems::SignResolved>();
        if !rendering && !self.force {
            return;
        }
        app.add_systems(Startup, (rig::spawn_rig, bake::start_bake))
            .add_systems(
                Update,
                (
                    bake::poll_bake.run_if(resource_exists::<bake::SignBake>),
                    systems::update_signs
                        .after(WorldSet::Chunks)
                        .after(crate::pilot::PilotSet)
                        .run_if(resource_exists::<SignAtlas>),
                )
                    .chain()
                    .in_set(CfSet::Simulate),
            )
            .add_systems(
                Update,
                systems::render_signs
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<SignAtlas>)
                    .run_if(resource_exists::<rig::SignRig>),
            );
    }
}
