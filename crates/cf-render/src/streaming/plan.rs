//! Ce qu'un tronçon contient : des pièces rangées par familles (même mesh, même matériau).
//!
//! Un monde déclare ses familles une fois ([`PieceRegistry`], au démarrage), puis décrit chaque tronçon sous forme de
//! données pures ([`ChunkPlan`]) : le cœur du streaming applique ce plan aux entités du pool. Le monde ne fait jamais
//! de `spawn` en régime établi, et son générateur se teste sans Bevy (même graine → même plan).
//!
//! Une famille garde son mesh et son matériau pour toujours : le recyclage ne touche qu'au `Transform`, à la
//! `Visibility` et au `MeshTag`. Le regroupement en lots (même mesh + même matériau) reste donc stable.

use bevy::ecs::world::EntityWorldMut;
use bevy::prelude::*;

/// Identifiant d'une famille de pièces, rendu par [`PieceRegistry::add`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ArchetypeId(u16);

impl ArchetypeId {
    /// Rang de la famille dans le registre.
    #[must_use]
    pub fn index(self) -> usize {
        usize::from(self.0)
    }

    /// Identifiant de la famille de rang `index` (saturé à `u16::MAX`).
    pub(crate) fn from_index(index: usize) -> Self {
        Self(u16::try_from(index).unwrap_or(u16::MAX))
    }
}

type InsertFn = Box<dyn Fn(&mut EntityWorldMut) + Send + Sync>;

/// Une famille de pièces : son nom, sa capacité par tronçon, et les composants de rendu posés sur chacune.
pub struct PieceArchetype {
    name: &'static str,
    per_chunk: u16,
    insert: InsertFn,
}

impl std::fmt::Debug for PieceArchetype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PieceArchetype")
            .field("name", &self.name)
            .field("per_chunk", &self.per_chunk)
            .finish_non_exhaustive()
    }
}

/// Les familles de pièces d'un monde, déclarées dans `World::setup`.
#[derive(Debug, Default)]
pub struct PieceRegistry {
    archetypes: Vec<PieceArchetype>,
}

impl PieceRegistry {
    /// Déclare une famille : au plus `per_chunk` pièces par tronçon, chacune recevant une copie de `bundle`
    /// (typiquement `(Mesh3d(mesh), MeshMaterial3d(matériau))`, plus d'éventuels marqueurs du monde).
    ///
    /// Les entités sont créées au démarrage : `tronçons × per_chunk` pièces pour cette famille.
    pub fn add<B: Bundle + Clone>(&mut self, name: &'static str, per_chunk: u16, bundle: B) -> ArchetypeId {
        // 65 536 familles : hors de portée d'un monde réel. Au-delà, les pièces iraient à la dernière famille.
        let id = ArchetypeId::from_index(self.archetypes.len());
        self.archetypes.push(PieceArchetype {
            name,
            per_chunk,
            insert: Box::new(move |entity: &mut EntityWorldMut| {
                entity.insert(bundle.clone());
            }),
        });
        id
    }

    /// Nombre de familles.
    #[must_use]
    pub fn len(&self) -> usize {
        self.archetypes.len()
    }

    /// Aucune famille déclarée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.archetypes.is_empty()
    }

    /// Nom et capacité par tronçon de chaque famille, dans l'ordre des identifiants.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, u16)> + '_ {
        self.archetypes.iter().map(|a| (a.name, a.per_chunk))
    }

    /// Pièces par tronçon, toutes familles confondues.
    #[must_use]
    pub fn pieces_per_chunk(&self) -> usize {
        self.archetypes.iter().map(|a| usize::from(a.per_chunk)).sum()
    }

    /// Pose les composants de rendu de la famille `index` sur une pièce (au démarrage seulement).
    pub(crate) fn insert_into(&self, index: usize, entity: &mut EntityWorldMut) {
        if let Some(archetype) = self.archetypes.get(index) {
            (archetype.insert)(entity);
        }
    }
}

/// Une pièce d'un tronçon : sa pose dans le repère du tronçon et une étiquette libre lue par le shader (`MeshTag`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Piece {
    /// Pose relative à l'ancrage du tronçon (m). Le tronçon couvre z ∈ [−L, 0] dans ce repère.
    pub transform: Transform,
    /// Valeur copiée dans le `MeshTag` de l'entité : variante, teinte, graine… au choix du monde.
    pub tag: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Region {
    offset: usize,
    capacity: usize,
    len: usize,
}

/// Le contenu d'un tronçon, sous forme de données. Taille fixée à la création : remplir un plan n'alloue jamais.
#[derive(Debug, Clone)]
pub struct ChunkPlan {
    regions: Vec<Region>,
    pieces: Vec<Piece>,
    refused: u32,
}

impl ChunkPlan {
    /// Plan vide aux capacités du registre.
    #[must_use]
    pub fn new(registry: &PieceRegistry) -> Self {
        let mut offset = 0;
        let regions = registry
            .archetypes
            .iter()
            .map(|a| {
                let region = Region {
                    offset,
                    capacity: usize::from(a.per_chunk),
                    len: 0,
                };
                offset += region.capacity;
                region
            })
            .collect();
        Self {
            regions,
            pieces: vec![Piece::default(); offset],
            refused: 0,
        }
    }

    /// Ajoute une pièce. Renvoie `false` (et compte un refus) si la famille est pleine ou inconnue : le monde a
    /// sous-dimensionné sa capacité, la pièce n'est pas affichée.
    pub fn push(&mut self, archetype: ArchetypeId, transform: Transform, tag: u32) -> bool {
        let Some(region) = self.regions.get_mut(archetype.index()) else {
            self.refused += 1;
            return false;
        };
        if region.len >= region.capacity {
            self.refused += 1;
            return false;
        }
        let at = region.offset + region.len;
        region.len += 1;
        if let Some(piece) = self.pieces.get_mut(at) {
            *piece = Piece { transform, tag };
        }
        true
    }

    /// Les pièces placées d'une famille.
    #[must_use]
    pub fn pieces(&self, archetype: ArchetypeId) -> &[Piece] {
        self.regions
            .get(archetype.index())
            .and_then(|r| self.pieces.get(r.offset..r.offset + r.len))
            .unwrap_or(&[])
    }

    /// Places encore libres dans une famille.
    #[must_use]
    pub fn remaining(&self, archetype: ArchetypeId) -> usize {
        self.regions.get(archetype.index()).map_or(0, |r| r.capacity - r.len)
    }

    /// Nombre de pièces placées, toutes familles confondues.
    #[must_use]
    pub fn len(&self) -> usize {
        self.regions.iter().map(|r| r.len).sum()
    }

    /// Aucune pièce placée.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Pièces refusées depuis le dernier `clear` (famille pleine ou inconnue).
    #[must_use]
    pub fn refused(&self) -> u32 {
        self.refused
    }

    /// Vide le plan sans libérer sa mémoire.
    pub fn clear(&mut self) {
        for region in &mut self.regions {
            region.len = 0;
        }
        self.refused = 0;
    }

    /// Nombre de familles couvertes par le plan.
    pub(crate) fn archetype_count(&self) -> usize {
        self.regions.len()
    }
}

/// Deux plans sont égaux s'ils placent les mêmes pièces dans les mêmes familles (le reste des tampons est ignoré).
impl PartialEq for ChunkPlan {
    fn eq(&self, other: &Self) -> bool {
        self.regions.len() == other.regions.len()
            && (0..self.regions.len()).all(|i| {
                let id = ArchetypeId::from_index(i);
                self.pieces(id) == other.pieces(id)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Component, Clone)]
    struct Marker;

    #[test]
    fn capacite_par_famille_et_refus() {
        let mut registry = PieceRegistry::default();
        let a = registry.add("a", 2, Marker);
        let b = registry.add("b", 1, Marker);
        assert_eq!(registry.pieces_per_chunk(), 3);
        let mut plan = ChunkPlan::new(&registry);
        assert!(plan.push(a, Transform::from_xyz(1.0, 0.0, 0.0), 1));
        assert!(plan.push(b, Transform::default(), 2));
        assert!(plan.push(a, Transform::from_xyz(2.0, 0.0, 0.0), 3));
        assert!(!plan.push(a, Transform::default(), 4));
        assert!(!plan.push(ArchetypeId(9), Transform::default(), 5));
        assert_eq!(plan.refused(), 2);
        assert_eq!(plan.len(), 3);
        assert_eq!(plan.pieces(a).iter().map(|p| p.tag).collect::<Vec<_>>(), vec![1, 3]);
        assert_eq!(plan.pieces(b)[0].tag, 2);
        plan.clear();
        assert!(plan.is_empty());
        assert_eq!(plan.remaining(a), 2);
    }

    #[test]
    fn egalite_ignore_les_restes() {
        let mut registry = PieceRegistry::default();
        let a = registry.add("a", 4, Marker);
        let mut p1 = ChunkPlan::new(&registry);
        let mut p2 = ChunkPlan::new(&registry);
        p1.push(a, Transform::from_xyz(5.0, 0.0, 0.0), 9);
        p1.clear();
        p1.push(a, Transform::default(), 1);
        p2.push(a, Transform::default(), 1);
        assert_eq!(p1, p2);
    }
}
