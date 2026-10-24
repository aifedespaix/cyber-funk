//! Le plugin de chargement : états `Loading` → `Ready`, préchauffe et détecteur de compilations tardives.
//!
//! ```text
//! Loading / Assets      attend que toutes les étapes du LoadTracker soient closes (voiture, musiques, logos…)
//! Loading / Compiling   fait naître les copies de préchauffe, toutes les couches rendent (sous le rideau), attend que
//!                       le PipelineCache n'ait plus de pipeline en attente et que leur nombre ne bouge plus
//! Ready                 copies détruites, rideau levé, détecteur armé : tout nouveau pipeline est un avertissement
//! ```
//!
//! Règle 8 de `CLAUDE.md` : aucun chargement d'asset ni compilation de shader pendant un show.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;

use super::curtain::{lift_curtain, spawn_curtain};
use super::files::{FileJobs, poll_file_jobs};
use super::mirror::{WarmEntity, place_warm_entities};
use super::pipelines::{PipelineWatch, install};
use super::registry::{WarmupAppExt, WarmupRegistry};
use super::tracker::LoadTracker;
use crate::layers::LayerWarmup;

/// Image minimale passée en chargement avant de lancer la préchauffe : les `Startup` et les premiers systèmes des
/// mondes (pools, fenêtre de tronçons) ont tourné, les entités à copier existent.
const MIN_ASSET_FRAMES: u32 = 30;
/// Délai (s) au-delà duquel une étape de chargement encore en cours est déclarée en échec : on ouvre quand même.
pub const ASSET_TIMEOUT_S: f32 = 120.0;
/// Images minimales de compilation : le temps que les copies soient extraites et spécialisées par le monde de rendu.
const MIN_COMPILE_FRAMES: u32 = 12;
/// Images consécutives sans pipeline en attente ni nouveau pipeline avant de déclarer la compilation finie.
pub const STABLE_FRAMES: u32 = 12;
/// Délai (s) au-delà duquel on ouvre sans attendre plus (un pipeline qui ne finit pas ne doit pas bloquer le show).
pub const COMPILE_TIMEOUT_S: f32 = 90.0;

/// État de l'application : chargement, puis spectacle.
#[derive(States, Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Chargement et préchauffe : le projecteur reste noir, l'opérateur voit la progression.
    #[default]
    Loading,
    /// Tout est chargé et compilé : le spectacle tourne.
    Ready,
}

/// Où en est le chargement.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WarmupPhase {
    /// Attente des étapes de préchargement.
    #[default]
    Assets,
    /// Copies de préchauffe posées, compilation des pipelines.
    Compiling,
    /// Terminé (état `Ready` demandé).
    Done,
}

/// Le suivi de la préchauffe, lu par l'écran de progression de l'opérateur.
#[derive(Resource, Debug, Default, Clone)]
pub struct WarmupProgress {
    /// Phase courante.
    pub phase: WarmupPhase,
    /// Secondes de chargement écoulées (temps réel).
    pub elapsed_s: f32,
    /// Images de chargement écoulées.
    pub frames: u32,
    /// Images de la phase de compilation, et parmi elles, consécutives sans mouvement du cache.
    pub compile_frames: u32,
    /// Images consécutives où le cache de pipelines n'a pas bougé et ne compile rien.
    pub stable_frames: u32,
    /// Préchauffes exécutées : nom et nombre d'entités créées.
    pub entries: Vec<(String, usize)>,
    /// Vrai si l'ouverture a eu lieu sur un délai dépassé (compilation ou chargement).
    pub timed_out: bool,
    /// Durée totale du chargement, connue à `Ready`.
    pub ready_after_s: Option<f32>,
    last_total: usize,
}

/// Le nombre de pipelines compilés après `Ready`, mis à jour à chaque image : 0 est l'état sain.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LatePipelines(pub usize);

/// Les copies de préchauffe vivantes (pour les détruire d'un coup).
#[derive(Resource, Debug, Default)]
struct WarmEntities(Vec<Entity>);

/// Condition d'exécution : le spectacle peut avancer. Vraie sans `WarmupPlugin` (tests, exemples), sinon seulement à
/// `Ready` : l'horloge d'un show ne doit pas courir pendant le chargement.
pub fn show_may_run(state: Option<Res<State<AppState>>>) -> bool {
    state.is_none_or(|state| *state.get() == AppState::Ready)
}

/// Chargement, préchauffe des pipelines et détecteur de compilations tardives.
///
/// À ajouter après `CorePlugin` et les plugins de rendu de Bevy, **avant** les plugins des mondes (ils déclarent leurs
/// préchauffes dans leur `build` ; le registre est de toute façon créé à la demande).
#[derive(Debug, Default, Clone, Copy)]
pub struct WarmupPlugin;

impl Plugin for WarmupPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<StatesPlugin>() {
            app.add_plugins(StatesPlugin);
        }
        install(app);
        app.init_state::<AppState>()
            .init_resource::<LoadTracker>()
            .init_resource::<FileJobs>()
            .init_resource::<WarmupRegistry>()
            .init_resource::<WarmupProgress>()
            .init_resource::<LatePipelines>()
            .init_resource::<WarmEntities>()
            // Toutes les couches rendent pendant le chargement, quel que soit le look : c'est ce qui compile le
            // pipeline de l'avant-plan ou de la surcouche avant qu'un look les allume en plein show.
            .insert_resource(LayerWarmup(true))
            .add_systems(Startup, spawn_curtain);
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            // Les matériaux standard du monde (carrosserie, quads des couches…) : toutes leurs variantes de pipeline.
            app.warm_material::<StandardMaterial>("matériaux standard");
        }
        app.add_systems(
            Update,
            (poll_file_jobs, drive_loading, place_warm_entities)
                .chain()
                .run_if(in_state(AppState::Loading)),
        )
        .add_systems(OnEnter(AppState::Ready), (finish_loading, lift_curtain))
        .add_systems(Update, report_late_pipelines.run_if(in_state(AppState::Ready)));
    }
}

/// `Loading` : fait avancer la machine à phases (voir la doc du module).
fn drive_loading(world: &mut World) {
    let dt = world.resource::<Time<Real>>().delta_secs();
    let watch = world.resource::<PipelineWatch>().clone();
    let (phase, elapsed_s, frames) = {
        let mut progress = world.resource_mut::<WarmupProgress>();
        progress.elapsed_s += dt;
        progress.frames += 1;
        (progress.phase, progress.elapsed_s, progress.frames)
    };
    match phase {
        WarmupPhase::Assets => {
            let complete = {
                let mut tracker = world.resource_mut::<LoadTracker>();
                if !tracker.is_complete() && elapsed_s > ASSET_TIMEOUT_S {
                    tracker.fail_pending("délai de chargement dépassé");
                    world.resource_mut::<WarmupProgress>().timed_out = true;
                }
                world.resource::<LoadTracker>().is_complete()
            };
            if complete && frames >= MIN_ASSET_FRAMES {
                start_compiling(world, &watch);
            }
        }
        WarmupPhase::Compiling => {
            let mut progress = world.resource_mut::<WarmupProgress>();
            progress.compile_frames += 1;
            let total = watch.total();
            let quiet = watch.waiting() == 0 && total == progress.last_total;
            progress.last_total = total;
            progress.stable_frames = if quiet { progress.stable_frames + 1 } else { 0 };
            // Sans monde de rendu (tests, serveur sans écran) il n'y a rien à compiler : seul le délai minimal compte.
            let compiled = if watch.attached() {
                progress.compile_frames >= MIN_COMPILE_FRAMES && progress.stable_frames >= STABLE_FRAMES
            } else {
                progress.compile_frames >= MIN_COMPILE_FRAMES
            };
            let timed_out = elapsed_s > ASSET_TIMEOUT_S + COMPILE_TIMEOUT_S && !compiled;
            if timed_out {
                tracing::warn!(
                    waiting = watch.waiting(),
                    "préchauffe : délai dépassé, ouverture sans attendre la fin de la compilation"
                );
                progress.timed_out = true;
            }
            if compiled || timed_out {
                progress.phase = WarmupPhase::Done;
                progress.ready_after_s = Some(elapsed_s);
                drop(progress);
                world.resource_mut::<NextState<AppState>>().set(AppState::Ready);
            }
        }
        WarmupPhase::Done => {}
    }
}

/// Fin du chargement des assets : exécute les préchauffes déclarées et passe à la compilation.
fn start_compiling(world: &mut World, watch: &PipelineWatch) {
    let (entities, counts) = world.resource_scope(|world, registry: Mut<WarmupRegistry>| registry.run_all(world));
    let copies = entities.len();
    for (label, count) in &counts {
        tracing::info!(warmup = %label, copies = count, "préchauffe : copies posées");
    }
    world.resource_mut::<WarmEntities>().0 = entities;
    let mut progress = world.resource_mut::<WarmupProgress>();
    progress.phase = WarmupPhase::Compiling;
    progress.entries = counts;
    progress.last_total = watch.total();
    tracing::info!(copies, "préchauffe : compilation des pipelines");
}

/// `OnEnter(Ready)` : détruit les copies, rend aux couches leur opacité de look, arme le détecteur.
fn finish_loading(world: &mut World) {
    let copies = std::mem::take(&mut world.resource_mut::<WarmEntities>().0);
    for entity in copies {
        // Une copie déjà détruite par ailleurs n'est pas une erreur.
        if let Ok(entity) = world.get_entity_mut(entity) {
            entity.despawn();
        }
    }
    // Filet : toute copie échappée au registre (déclaration qui n'a pas rendu ses entités).
    let strays: Vec<Entity> = world.query_filtered::<Entity, With<WarmEntity>>().iter(world).collect();
    for entity in strays {
        world.entity_mut(entity).despawn();
    }
    world.resource_mut::<LayerWarmup>().0 = false;
    let watch = world.resource::<PipelineWatch>().clone();
    watch.arm();
    let progress = world.resource::<WarmupProgress>();
    tracing::info!(
        seconds = progress.ready_after_s.unwrap_or_default(),
        pipelines = watch.total(),
        timed_out = progress.timed_out,
        "prêt : chargement terminé, détecteur de compilations tardives armé"
    );
}

/// `Ready` : recopie le compteur de compilations tardives pour l'interface opérateur.
fn report_late_pipelines(watch: Res<PipelineWatch>, mut late: ResMut<LatePipelines>) {
    late.set_if_neq(LatePipelines(watch.late()));
}
