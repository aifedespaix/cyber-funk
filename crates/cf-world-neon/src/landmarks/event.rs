//! L'événement `landmark` du trafic et le plugin des immeubles-vitrines.
//!
//! [`LandmarkEvent`] est l'adaptateur entre la scène des immeubles ([`LandmarkStage`]) et le directeur du trafic : il pose
//! l'immeuble devant la voiture, dégage la ville autour, lui passe la musique à chaque image et le range quand il est dépassé.
//! C'est une structure : jamais deux en même temps (`TrafficEvent::structure`). Il publie le **sujet `Landmark`** que filme le
//! plan `landmark-reveal` du réalisateur (`TrafficEvent::focus`) et, pour une arche ou des mecha, la structure à esquiver.
//!
//! # Partage de la scène
//!
//! Un événement du trafic n'a pas accès au `World` ; la scène est donc derrière un [`LandmarkHandle`] (`Arc<Mutex<_>>`), partagé
//! entre l'événement, qui l'avance, et les systèmes Bevy qui la nourrissent (palette, fraîcheur) et la dessinent. Le verrou est
//! pris une fois par système et par image, jamais sur le thread audio.
//!
//! # Branchement
//!
//! [`LandmarksPlugin`] inscrit la fabrique de l'événement `landmark` **seulement si aucune autre ne l'est** à la fin de la
//! construction de l'application (`Plugin::finish`) : le lot B des événements du trafic (J7-05) peut inscrire la sienne autour
//! de [`LandmarkPass`](super::pass::LandmarkPass), qui porte déjà toute la mécanique.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy::asset::Assets;
use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{AudioFeatures, CfSet, SceneMood, ScenePalette, ShowRuntime, WarmupAppExt};

use super::field::CityField;
use super::frame::Colors;
use super::landmark::LandmarkKind;
use super::pass::{LandmarkPass, PassStart};
use super::render;
use super::stage::{Forced, LandmarkStage};
use crate::pilot::flight::Obstruction;
use crate::scenery::Clearing;
use crate::traffic::{EventCtx, EventName, EventRegistry, Focus, TrafficEvent, TrafficSet};

/// Longueur du cycle prismatique (les teintes de la variation du spectacle).
const CYCLE_LEN: usize = 6;

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

    /// Outil de développement : le prochain événement `landmark` sera cet immeuble, à cette distance du centre devant la
    /// voiture et de ce côté si on les donne (`force` de `director.ts`).
    pub fn force(&self, kind: LandmarkKind, at: Option<(f32, i8)>) {
        self.lock().force(Some(Forced { kind, at }));
    }
}

/// Un immeuble-vitrine qui passe (`LandmarkEvent`).
#[derive(Debug)]
pub struct LandmarkEvent {
    handle: LandmarkHandle,
    pass: LandmarkPass,
}

impl LandmarkEvent {
    /// Tire un immeuble (jamais l'un des deux derniers, selon la fraîcheur, les plus calmes en mode calme) et le pose.
    pub fn start(handle: &LandmarkHandle, ctx: &mut EventCtx<'_>) -> Self {
        let mut stage = handle.lock();
        let forced = stage.take_forced();
        let kind = forced.map_or_else(
            || stage.choose(ctx.rng, ctx.frame.calm, ctx.frame.time),
            |forced| forced.kind,
        );
        let spans_road = stage.landmark(kind).spans_road();
        // Hors spectacle : un côté et une teinte au hasard ; au spectacle : l'allure varie d'un passage à l'autre.
        let (mut side, tint, scale) = if ctx.frame.show.is_some() {
            stage.variation(ctx.rng, spans_road, CYCLE_LEN)
        } else {
            let side = if ctx.rng.coin() { 1 } else { -1 };
            let tint = ((ctx.rng.unit() * CYCLE_LEN as f64) as usize % CYCLE_LEN) as u8;
            (side, tint, 1.0)
        };
        let mut debug_center = None;
        if let Some((center, forced_side)) = forced.and_then(|f| f.at) {
            debug_center = Some(center);
            side = forced_side;
        }
        let pass = LandmarkPass::begin(
            &mut stage,
            kind,
            ctx.frame.car.z,
            PassStart {
                side,
                tint,
                scale,
                debug_center,
            },
        );
        drop(stage);
        Self {
            handle: handle.clone(),
            pass,
        }
    }

    /// Le passage (lecture).
    #[must_use]
    pub fn pass(&self) -> &LandmarkPass {
        &self.pass
    }
}

impl TrafficEvent for LandmarkEvent {
    fn structure(&self) -> bool {
        true
    }

    fn clearing(&self) -> Option<Clearing> {
        Some(self.pass.clearing())
    }

    fn obstruction(&self) -> Option<Obstruction> {
        self.pass.obstruction()
    }

    fn focus(&self) -> Option<Focus> {
        let (position, size) = self.pass.focus();
        Some(Focus { position, size })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        let mut stage = self.handle.lock();
        self.pass.update(&mut stage, ctx.frame, dt)
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        let mut stage = self.handle.lock();
        self.pass.end(&mut stage);
    }
}

/// Installe les immeubles-vitrines : la scène, le champ de la ville, le rendu, l'événement.
///
/// À ajouter après les plugins de rendu de Bevy (les shaders sont embarqués à la construction) et, pour l'événement, avec
/// le trafic ([`TrafficPlugin`](crate::TrafficPlugin)). **Sans rendu (tests)**, seules la logique et le champ sont installés.
#[derive(Debug, Default, Clone, Copy)]
pub struct LandmarksPlugin;

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
                (sync_inputs.before(TrafficSet::Step), publish.after(TrafficSet::Step)).in_set(CfSet::Simulate),
            );
        if rendering {
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
        if !registry.contains(EventName::Landmark) {
            registry.register(
                EventName::Landmark,
                Box::new(move |ctx| Box::new(LandmarkEvent::start(&handle, ctx))),
            );
        }
    }
}

/// `CfSet::Simulate`, avant le trafic : ce que la scène lit en dehors de l'image du trafic (temps musical, palette, teinte
/// d'humeur) et la fraîcheur de chaque immeuble d'après la mémoire d'anti-répétition du chef du spectacle.
fn sync_inputs(
    handle: Res<LandmarkHandle>,
    features: Option<Res<AudioFeatures>>,
    palette: Option<Res<ScenePalette>>,
    mood: Option<Res<SceneMood>>,
    time: Res<Time>,
    runtime: Option<Res<ShowRuntime>>,
) {
    let mut stage = handle.lock();
    if let Some(features) = features {
        stage.inputs.beat = features.0.beat as f32;
    }
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
            stage.take_marks(|kind, now| runtime.director_mut().freshness_mut().mark(kind.fresh_id(), now))
        }
        // Sans spectacle, personne ne se souvient : on vide quand même la file.
        None => stage.take_marks(|_, _| {}),
    }
    if field.state != stage.field {
        field.state = stage.field;
    }
}
