//! Le pool de tronçons d'un monde et le système qui le fait suivre le point suivi.
//!
//! Au démarrage : `N` racines de tronçon (une par emplacement de la fenêtre), et sous chacune les pièces de toutes
//! les familles, cachées. En régime établi : quand un tronçon entre dans la fenêtre, sa racine prend la position
//! absolue du tronçon et ses pièces reçoivent le plan du monde. Rien n'est créé ni détruit, rien n'est alloué.

use std::marker::PhantomData;

use bevy::ecs::world::World as EcsWorld;
use bevy::math::DVec3;
use bevy::mesh::MeshTag;
use bevy::prelude::*;

use super::plan::{ArchetypeId, ChunkPlan, PieceRegistry};
use super::window::{ChunkInfo, ChunkWindow, StreamLayout};
use crate::origin::{AbsolutePosition, StreamFocus};
use crate::world::{LiveChunks, World};

/// Racine d'un tronçon (un emplacement de la fenêtre). Porte la position absolue du tronçon.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRoot {
    /// Emplacement dans la fenêtre.
    pub slot: u32,
}

/// Une pièce d'un tronçon, enfant de sa [`ChunkRoot`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamedPiece {
    /// Emplacement du tronçon.
    pub slot: u32,
    /// Famille de la pièce.
    pub archetype: ArchetypeId,
}

/// Compteurs du streaming, pour la surcouche de diagnostic (J4-05). Écrits par le système de streaming du monde actif.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StreamStats {
    /// Tronçons vivants.
    pub chunks_live: u32,
    /// Taille de la fenêtre (emplacements).
    pub chunks_capacity: u32,
    /// Tronçons remplis cette image.
    pub chunks_filled: u32,
    /// Tronçons remplis depuis le démarrage.
    pub chunks_filled_total: u64,
    /// Pièces préallouées (toutes familles, tous tronçons).
    pub pieces_pooled: u32,
    /// Pièces affichées (placées par les plans des tronçons vivants).
    pub pieces_used: u32,
    /// Pièces refusées depuis le démarrage (famille pleine) : un monde qui sous-dimensionne ses capacités.
    pub pieces_refused: u64,
}

/// Le streaming d'un monde `W` : fenêtre, pool de tronçons, plan de travail.
///
/// Créée au démarrage par le cœur. Les systèmes du monde la lisent pour savoir quels tronçons vivent
/// ([`ChunkStream::live`]) ou viennent d'être remplis ([`ChunkStream::filled`], valable dans l'image, après
/// `WorldSet::Chunks`), et peuvent demander un re-tirage ([`ChunkStream::replan`]).
#[derive(Resource)]
pub struct ChunkStream<W> {
    window: ChunkWindow,
    /// Pour chaque famille : (décalage dans le bloc d'un tronçon, capacité).
    regions: Vec<(usize, usize)>,
    per_chunk: usize,
    roots: Vec<Entity>,
    /// Pièces, par bloc de `per_chunk` : `[emplacement][famille][rang]`.
    pieces: Vec<Entity>,
    live: Vec<Option<i64>>,
    dirty: Vec<bool>,
    used: Vec<u32>,
    filled: Vec<u32>,
    plan: ChunkPlan,
    focus: DVec3,
    _world: PhantomData<fn() -> W>,
}

impl<W> std::fmt::Debug for ChunkStream<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkStream")
            .field("layout", self.window.layout())
            .field("first", &self.window.first())
            .field("pieces", &self.pieces.len())
            .finish_non_exhaustive()
    }
}

impl<W: World> ChunkStream<W> {
    /// Crée racines et pièces (cachées) dans l'ECS. Démarrage seulement.
    pub(crate) fn spawn(ecs: &mut EcsWorld, layout: StreamLayout, registry: &PieceRegistry) -> Self {
        let count = layout.count() as usize;
        let mut regions = Vec::with_capacity(registry.len());
        let mut offset = 0;
        for (_, per_chunk) in registry.iter() {
            regions.push((offset, usize::from(per_chunk)));
            offset += usize::from(per_chunk);
        }
        let per_chunk = offset;
        let mut roots = Vec::with_capacity(count);
        let mut pieces = Vec::with_capacity(count * per_chunk);
        for slot in 0..count as u32 {
            let root = ecs
                .spawn((
                    Name::new(format!("{} · tronçon {slot}", W::ID)),
                    ChunkRoot { slot },
                    AbsolutePosition::default(),
                    Visibility::Hidden,
                ))
                .id();
            roots.push(root);
            for (archetype, &(_, capacity)) in regions.iter().enumerate() {
                let id = ArchetypeId::from_index(archetype);
                for _ in 0..capacity {
                    let mut piece = ecs.spawn((
                        StreamedPiece { slot, archetype: id },
                        MeshTag::new(0),
                        Visibility::Hidden,
                    ));
                    piece.insert(ChildOf(root));
                    registry.insert_into(archetype, &mut piece);
                    pieces.push(piece.id());
                }
            }
        }
        Self {
            window: ChunkWindow::new(layout),
            regions,
            per_chunk,
            roots,
            pieces,
            live: vec![None; count],
            dirty: vec![false; count],
            used: vec![0; count],
            filled: Vec::with_capacity(count),
            plan: ChunkPlan::new(registry),
            focus: DVec3::ZERO,
            _world: PhantomData,
        }
    }
}

impl<W> ChunkStream<W> {
    /// Géométrie de la fenêtre.
    #[must_use]
    pub fn layout(&self) -> &StreamLayout {
        self.window.layout()
    }

    /// Les tronçons vivants.
    #[must_use]
    pub fn live(&self) -> LiveChunks<'_> {
        LiveChunks::new(self.window.layout(), &self.live)
    }

    /// Les tronçons remplis (ou re-tirés) dans l'image en cours.
    pub fn filled(&self) -> impl Iterator<Item = ChunkInfo> + '_ {
        let layout = self.window.layout();
        self.filled
            .iter()
            .filter_map(|&slot| self.live.get(slot as usize).copied().flatten())
            .map(|index| layout.chunk(index))
    }

    /// Position absolue du point suivi lors du dernier passage du streaming.
    #[must_use]
    pub fn focus(&self) -> DVec3 {
        self.focus
    }

    /// Emplacement du tronçon `index` s'il est vivant.
    #[must_use]
    pub fn slot_of(&self, index: i64) -> Option<usize> {
        let slot = self.window.layout().slot_of(index);
        (self.live.get(slot).copied().flatten() == Some(index)).then_some(slot)
    }

    /// Entité racine d'un emplacement.
    #[must_use]
    pub fn root(&self, slot: usize) -> Option<Entity> {
        self.roots.get(slot).copied()
    }

    /// Les pièces d'une famille dans un emplacement (pour les systèmes du monde qui posent des composants en plus).
    #[must_use]
    pub fn pieces(&self, slot: usize, archetype: ArchetypeId) -> &[Entity] {
        self.regions
            .get(archetype.index())
            .and_then(|&(offset, capacity)| {
                let start = slot * self.per_chunk + offset;
                self.pieces.get(start..start + capacity)
            })
            .unwrap_or(&[])
    }

    /// Re-tire le tronçon `index` à la prochaine passe du streaming (s'il est vivant). Renvoie `false` sinon.
    pub fn replan(&mut self, index: i64) -> bool {
        let Some(slot) = self.slot_of(index) else { return false };
        if let Some(dirty) = self.dirty.get_mut(slot) {
            *dirty = true;
        }
        true
    }

    /// Re-tire tous les tronçons vivants à la prochaine passe du streaming.
    pub fn replan_all(&mut self) {
        for (dirty, live) in self.dirty.iter_mut().zip(&self.live) {
            *dirty |= live.is_some();
        }
    }
}

/// Les requêtes du streaming : racines et pièces, disjointes (une racine n'est jamais une pièce).
type Roots<'w, 's> =
    Query<'w, 's, (&'static mut AbsolutePosition, &'static mut Visibility), (With<ChunkRoot>, Without<StreamFocus>)>;
type Pieces<'w, 's> = Query<
    'w,
    's,
    (&'static mut Transform, &'static mut Visibility, &'static mut MeshTag),
    (With<StreamedPiece>, Without<ChunkRoot>),
>;

/// `WorldSet::Chunks` : recale la fenêtre sur le point suivi et remplit les tronçons entrants ou à re-tirer.
///
/// Sans point suivi (ou s'il y en a plusieurs), la fenêtre reste où elle est.
pub(crate) fn stream_chunks<W: World>(
    world: Res<W>,
    mut stream: ResMut<ChunkStream<W>>,
    focus: Query<&AbsolutePosition, With<StreamFocus>>,
    mut roots: Roots<'_, '_>,
    mut pieces: Pieces<'_, '_>,
    mut stats: ResMut<StreamStats>,
) {
    // Les lecteurs se fient à `filled()` dans l'image, pas à la détection de changement : on ne marque la ressource
    // modifiée que si un tronçon a été rempli.
    let stream_ref = stream.bypass_change_detection();
    stream_ref.filled.clear();
    if let Ok(focus) = focus.single() {
        stream_ref.focus = focus.0;
    }
    let distance = -stream_ref.focus.z;
    let ChunkStream {
        window, live, dirty, ..
    } = &mut *stream_ref;
    window.update(distance, |slot, index| {
        if let (Some(live), Some(dirty)) = (live.get_mut(slot), dirty.get_mut(slot)) {
            *live = Some(index);
            *dirty = true;
        }
    });

    let mut refused = 0;
    for slot in 0..stream_ref.dirty.len() {
        if !std::mem::take(&mut stream_ref.dirty[slot]) {
            continue;
        }
        let Some(index) = stream_ref.live[slot] else { continue };
        let chunk = stream_ref.window.layout().chunk(index);
        stream_ref.plan.clear();
        world.plan_chunk(&chunk, &mut stream_ref.plan);
        refused += u64::from(stream_ref.plan.refused());
        if let Ok((mut position, mut visibility)) = roots.get_mut(stream_ref.roots[slot]) {
            position.0 = chunk.anchor;
            visibility.set_if_neq(Visibility::Inherited);
        }
        stream_ref.used[slot] = apply_plan(stream_ref, slot, &mut pieces);
        stream_ref.filled.push(slot as u32);
    }

    let filled = stream_ref.filled.len() as u32;
    if filled > 0 {
        stream.set_changed();
    }
    *stats = StreamStats {
        chunks_live: stream.live.iter().filter(|i| i.is_some()).count() as u32,
        chunks_capacity: stream.live.len() as u32,
        chunks_filled: filled,
        chunks_filled_total: stats.chunks_filled_total + u64::from(filled),
        pieces_pooled: stream.pieces.len() as u32,
        pieces_used: stream.used.iter().sum(),
        pieces_refused: stats.pieces_refused + refused,
    };
}

/// Applique le plan de travail aux pièces d'un emplacement ; cache les pièces non utilisées. Renvoie le nombre de
/// pièces affichées.
fn apply_plan<W>(stream: &ChunkStream<W>, slot: usize, pieces: &mut Pieces<'_, '_>) -> u32 {
    let mut used = 0;
    for archetype in 0..stream.plan.archetype_count() {
        let id = ArchetypeId::from_index(archetype);
        let planned = stream.plan.pieces(id);
        for (rank, &entity) in stream.pieces(slot, id).iter().enumerate() {
            let Ok((mut transform, mut visibility, mut tag)) = pieces.get_mut(entity) else {
                continue;
            };
            if let Some(piece) = planned.get(rank) {
                *transform = piece.transform;
                visibility.set_if_neq(Visibility::Inherited);
                if tag.value != piece.tag {
                    tag.value = piece.tag;
                }
                used += 1;
            } else {
                visibility.set_if_neq(Visibility::Hidden);
            }
        }
    }
    used
}
