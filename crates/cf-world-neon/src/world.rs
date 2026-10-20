//! Le monde Neon City comme implémentation du trait [`World`] : géométrie du streaming et contrat avec le cœur de
//! `cf-render` (tronçons, portes, sujets).
//!
//! J5-01 ne remplit encore aucun tronçon : la route est un plan unique qui suit le point suivi (voir [`crate::road`]).
//! Les immeubles (J5-02), les enseignes et les événements déclareront leurs familles de pièces dans
//! [`World::setup`] et les tireront dans [`World::plan_chunk`].

use bevy::ecs::world::World as EcsWorld;
use bevy::prelude::*;
use cf_render::world::{SubjectContext, SubjectSink};
use cf_render::{ChunkInfo, ChunkPlan, PieceRegistry, Replan, StreamLayout, World, WorldError};
use cf_show::model::ShowGates;

/// Longueur d'un tronçon : l'écart entre deux rangées d'immeubles (`SPACING` de `scenery.ts`), en m.
pub const CHUNK_LENGTH_M: f32 = 26.0;
/// Rangées gardées derrière la voiture (`BEHIND` de `scenery.ts`) : 390 m. La caméra ne recule pas à plus de 90 m
/// derrière la voiture (`MAX_BEHIND` de `cinema/director.ts`).
pub const BEHIND: u32 = 15;
/// Rangées devant : `ROWS` (34) − `BEHIND` − la rangée courante. La ville s'étend à ~494 m devant ; la caméra n'avance
/// pas à plus de 190 m devant la voiture (`MAX_AHEAD`).
pub const AHEAD: u32 = 18;
/// Graine du monde par défaut.
pub const DEFAULT_SEED: u64 = 0x0000_4E30_C17E;

/// La ville de Neon City.
#[derive(Resource, Debug, Clone)]
pub struct NeonWorld {
    seed: u64,
}

impl Default for NeonWorld {
    fn default() -> Self {
        Self::new(DEFAULT_SEED)
    }
}

impl NeonWorld {
    /// Un monde de graine `seed` : avec l'index d'un tronçon, elle détermine tout son contenu.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { seed }
    }

    /// La graine du monde.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

impl World for NeonWorld {
    const ID: &'static str = "neon";
    type SubjectParam = ();

    fn layout(&self) -> StreamLayout {
        StreamLayout {
            chunk_length_m: CHUNK_LENGTH_M,
            behind: BEHIND,
            ahead: AHEAD,
            seed: self.seed,
        }
    }

    fn setup(&mut self, _ecs: &mut EcsWorld, _pieces: &mut PieceRegistry) -> Result<(), WorldError> {
        Ok(())
    }

    fn plan_chunk(&self, _chunk: &ChunkInfo, _plan: &mut ChunkPlan) {}

    fn apply_gates(&mut self, _gates: &ShowGates) -> Replan {
        Replan::Keep
    }

    fn subjects(&self, _ctx: &SubjectContext<'_>, &(): &(), _out: &mut SubjectSink<'_>) {}
}
