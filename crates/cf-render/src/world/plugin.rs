//! Plugins : le cœur commun à tous les mondes ([`WorldCorePlugin`]) et le branchement d'un monde ([`WorldPlugin`]).

use bevy::ecs::system::StaticSystemParam;
use bevy::ecs::world::World as EcsWorld;
use bevy::prelude::*;

use super::{CurrentGates, Replan, SubjectContext, Subjects, World, WorldError, WorldSet};
use crate::CfSet;
use crate::origin::OriginPlugin;
use crate::streaming::{ChunkStream, PieceRegistry, StreamStats, stream_chunks};

/// Sous-étapes de `WorldSet::Subjects` : vider la liste une fois, puis chaque monde y dépose ses sujets.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SubjectStep {
    Clear,
    Collect,
}

/// Ce que tous les mondes partagent : origine flottante, ordre des étapes, sujets, compteurs. Ajouté par
/// [`WorldPlugin`] s'il manque.
#[derive(Debug, Default)]
pub struct WorldCorePlugin;

impl Plugin for WorldCorePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<OriginPlugin>() {
            app.add_plugins(OriginPlugin);
        }
        app.init_resource::<Subjects>()
            .init_resource::<StreamStats>()
            .configure_sets(
                Update,
                (WorldSet::Gates, WorldSet::Chunks).chain().in_set(CfSet::Simulate),
            )
            .configure_sets(Update, WorldSet::Subjects.after(CfSet::Simulate).before(CfSet::Direct))
            .configure_sets(
                Update,
                (SubjectStep::Clear, SubjectStep::Collect)
                    .chain()
                    .in_set(WorldSet::Subjects),
            )
            .add_systems(Update, clear_subjects.in_set(SubjectStep::Clear));
    }
}

/// Branche le monde `W` : sa ressource, la création de ses tronçons au démarrage, ses étapes dans l'image.
///
/// ```ignore
/// app.add_plugins((CorePlugin, WorldPlugin::new(BoxWorld::default())));
/// ```
///
/// Un seul monde actif à la fois : deux mondes branchés streameraient tous les deux (les compteurs de
/// [`StreamStats`] seraient alors ceux du dernier passé).
#[derive(Debug)]
pub struct WorldPlugin<W> {
    world: W,
}

impl<W: World> WorldPlugin<W> {
    /// Le plugin du monde `world` (sa configuration de départ : graine, densité…).
    pub fn new(world: W) -> Self {
        Self { world }
    }
}

impl<W: World + Clone> Plugin for WorldPlugin<W> {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<WorldCorePlugin>() {
            app.add_plugins(WorldCorePlugin);
        }
        app.insert_resource(self.world.clone())
            .add_systems(Startup, setup_stream::<W>)
            .add_systems(
                Update,
                (
                    apply_gates::<W>
                        .in_set(WorldSet::Gates)
                        .run_if(resource_exists::<ChunkStream<W>>)
                        .run_if(resource_exists_and_changed::<CurrentGates>),
                    stream_chunks::<W>
                        .in_set(WorldSet::Chunks)
                        .run_if(resource_exists::<ChunkStream<W>>),
                    collect_subjects::<W>
                        .in_set(SubjectStep::Collect)
                        .run_if(resource_exists::<ChunkStream<W>>),
                ),
            );
    }
}

/// Démarrage : le monde prépare ses assets et ses familles de pièces, puis le cœur crée tout le pool de tronçons.
fn setup_stream<W: World>(ecs: &mut EcsWorld) {
    let prepared = ecs.try_resource_scope(|ecs, mut world: Mut<W>| {
        let layout = world.layout();
        if !layout.is_valid() {
            return Err(WorldError::InvalidLayout(layout));
        }
        let mut registry = PieceRegistry::default();
        world.setup(ecs, &mut registry)?;
        Ok((layout, registry))
    });
    match prepared {
        Some(Ok((layout, registry))) => {
            let stream = ChunkStream::<W>::spawn(ecs, layout, &registry);
            info!(
                "monde « {} » : {} tronçons de {} m, {} pièces préallouées",
                W::ID,
                layout.count(),
                layout.chunk_length_m,
                layout.count() as usize * registry.pieces_per_chunk()
            );
            ecs.insert_resource(stream);
        }
        Some(Err(error)) => error!("monde « {} » non démarré, il restera vide : {error}", W::ID),
        None => error!("monde « {} » non démarré : ressource du monde absente", W::ID),
    }
}

/// `WorldSet::Gates` : prévient le monde d'un changement de portes, re-tire les tronçons s'il le demande.
fn apply_gates<W: World>(mut world: ResMut<W>, gates: Res<CurrentGates>, mut stream: ResMut<ChunkStream<W>>) {
    if world.apply_gates(&gates.0) == Replan::All {
        stream.replan_all();
    }
}

fn clear_subjects(mut subjects: ResMut<Subjects>) {
    subjects.clear();
}

/// `WorldSet::Subjects` : le monde dépose ses sujets de l'image.
fn collect_subjects<W: World>(
    world: Res<W>,
    stream: Res<ChunkStream<W>>,
    param: StaticSystemParam<W::SubjectParam>,
    mut subjects: ResMut<Subjects>,
) {
    let ctx = SubjectContext {
        chunks: stream.live(),
        focus: stream.focus(),
    };
    world.subjects(&ctx, &param, &mut subjects.sink());
}
