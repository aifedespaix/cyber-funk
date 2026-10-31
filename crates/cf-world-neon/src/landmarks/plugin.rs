//! Le plugin des immeubles-vitrines et la scène partagée.
//!
//! # Partage de la scène
//!
//! Un événement du trafic n'a pas accès au `World` ; la scène est donc derrière un [`LandmarkHandle`] (`Arc<Mutex<_>>`),
//! partagé entre l'événement (par [`StageLandmark`]), qui l'avance, et les systèmes Bevy qui la nourrissent (palette,
//! fraîcheur) et la dessinent. Le verrou est pris une fois par système et par image, jamais sur le thread audio.
//!
//! # Branchement
//!
//! L'événement `landmark` est celui de J7-05 ([`LandmarkEvent`](crate::traffic::grand::LandmarkEvent)) : `TrafficPlugin` l'inscrit
//! avec une vitrine de secours. [`LandmarksPlugin`] **remplace** la fabrique de cet événement, dans `Plugin::finish` (après
//! la construction de tous les plugins), par une qui tire les immeubles de la scène ([`StageLandmark`]).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy::asset::Assets;
use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, SceneMood, ScenePalette, ShowRuntime, WarmupAppExt, WorldEventAppExt};

use super::adapter::StageLandmark;
use super::field::CityField;
use super::frame::Colors;
use super::landmark::LandmarkKind;
use super::render;
use super::stage::LandmarkStage;
use crate::traffic::grand::{LandmarkFactory, grand_factories};
use crate::traffic::{EventCtx, EventName, EventRegistry, TrafficSet};

/// La scène des immeubles, partagée.
#[derive(Resource, Clone, Debug)]
pub struct LandmarkHandle(Arc<Mutex<LandmarkStage>>);

impl Default for LandmarkHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl LandmarkHandle {
    /// Une scène neuve (construit les neuf immeubles).
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(LandmarkStage::new())))
    }

    /// Verrouille la scène. Un verrou empoisonné (un panic ailleurs) est repris tel quel : le show continue.
    pub fn lock(&self) -> MutexGuard<'_, LandmarkStage> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Outil de développement : le prochain événement `landmark` sera cet immeuble (`force` de `director.ts`).
    pub fn force(&self, kind: LandmarkKind) {
        self.lock().force(Some(kind));
    }
}

/// Installe les immeubles-vitrines : la scène, le champ de la ville, le rendu, l'événement.
///
/// À ajouter après les plugins de rendu de Bevy (les shaders sont embarqués à la construction) et, pour l'événement, avec
/// le trafic ([`TrafficPlugin`](crate::TrafficPlugin)). **Sans rendu (tests)**, seules la logique et le champ sont installés.
#[derive(Debug, Default, Clone, Copy)]
pub struct LandmarksPlugin {
    /// Installer aussi sans rendu (tests d'intégration) : les entités sont créées, le matériau est seulement déclaré.
    pub force: bool,
}

impl LandmarksPlugin {
    /// Installation complète même sans moteur de rendu.
    #[must_use]
    pub const fn always() -> Self {
        Self { force: true }
    }
}

impl Plugin for LandmarksPlugin {
    fn build(&self, app: &mut App) {
        let rendering = app.is_plugin_added::<bevy::pbr::PbrPlugin>();
        if rendering {
            app.add_plugins(MaterialPlugin::<render::LandmarkMaterial>::default())
                .add_warmup("immeubles-vitrines", render::warm);
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                render::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<render::LandmarkMaterial>();
        }
        app.insert_resource(LandmarkHandle::new())
            .init_resource::<CityField>()
            .add_systems(
                Update,
                (
                    sync_inputs.before(TrafficSet::Step),
                    super::catalog::take_landmark_triggers.before(TrafficSet::Step),
                    publish.after(TrafficSet::Step),
                )
                    .in_set(CfSet::Simulate),
            )
            .register_world_events(super::catalog::world_events());
        if rendering || self.force {
            app.add_systems(PostStartup, render::spawn_rig).add_systems(
                Update,
                render::apply_stage
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<render::LandmarkRig>),
            );
        }
    }

    fn finish(&self, app: &mut App) {
        let Some(handle) = app.world().get_resource::<LandmarkHandle>().cloned() else {
            return;
        };
        let Some(mut registry) = app.world_mut().get_resource_mut::<EventRegistry>() else {
            return;
        };
        let provider: LandmarkFactory =
            Box::new(move |ctx: &mut EventCtx<'_>| Box::new(StageLandmark::choose(&handle, ctx)));
        // La fabrique de J7-05 (variation du spectacle, côté et teinte au hasard hors spectacle) avec nos immeubles : on ne
        // garde de ses fabriques que celle de `landmark`.
        for (name, build) in grand_factories(Some(provider)) {
            if name == EventName::Landmark {
                registry.register(name, build);
            }
        }
    }
}

/// `CfSet::Simulate`, avant le trafic : ce que la scène lit en dehors de l'image du trafic (palette, teinte d'humeur) et la
/// fraîcheur de chaque immeuble d'après la mémoire d'anti-répétition du chef du spectacle.
fn sync_inputs(
    handle: Res<LandmarkHandle>,
    palette: Option<Res<ScenePalette>>,
    mood: Option<Res<SceneMood>>,
    time: Res<Time>,
    runtime: Option<Res<ShowRuntime>>,
) {
    let mut stage = handle.lock();
    if let Some(palette) = palette {
        stage.inputs.colors = Colors::of(&palette);
    }
    if let Some(mood) = mood {
        stage.inputs.mood = Vec3::new(mood.color.r, mood.color.g, mood.color.b);
    }
    if let Some(runtime) = runtime {
        let now = time.elapsed_secs_f64();
        let freshness = runtime.director().freshness();
        for kind in LandmarkKind::ALL {
            stage.set_freshness(kind, freshness.weight(kind.fresh_id(), now));
        }
    }
}

/// `CfSet::Simulate`, après le trafic : marque les immeubles tirés chez le chef du spectacle et publie le champ de la ville
/// que lisent les façades et la route.
fn publish(handle: Res<LandmarkHandle>, runtime: Option<ResMut<ShowRuntime>>, mut field: ResMut<CityField>) {
    let mut stage = handle.lock();
    match runtime {
        Some(mut runtime) => {
            stage.take_marks(|kind, now| runtime.director_mut().freshness_mut().mark(kind.fresh_id(), now));
        }
        // Sans spectacle, personne ne se souvient : on vide quand même la file.
        None => stage.take_marks(|_, _| {}),
    }
    if field.state != stage.field {
        field.state = stage.field;
    }
}
