//! Les sujets filmables : ce que le monde offre à la caméra en dehors de la voiture (`Subject` de `cinema/types.ts`).
//!
//! Le monde les liste à chaque image dans [`Subjects`] (capacité fixe, comme les 160 emplacements de
//! `cinema/world.ts`) ; le réalisateur (J6) les lit. Positions en absolu (`f64`) : un plan calcule sa pose en absolu,
//! la conversion vers le repère rendu se fait au dernier moment (voir `origin`).

use bevy::math::DVec3;
use bevy::prelude::*;

use crate::streaming::ChunkInfo;

/// Nombre de sujets gardés par image (`CAPACITY` de `cinema/world.ts`). Au-delà, les suivants sont ignorés et comptés.
pub const SUBJECT_CAPACITY: usize = 160;

/// Ce qu'un plan peut filmer en dehors de la voiture (`SubjectKind` de Sillage).
///
/// Vocabulaire commun à tous les mondes : un univers qui ajoute un genre de sujet l'ajoute ici, et le compilateur
/// signale chaque plan du réalisateur qui doit en tenir compte.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubjectKind {
    /// Voiture de police d'une poursuite.
    Police,
    /// Le fuyard qu'elle poursuit.
    Quarry,
    /// Moto lumineuse (événement des motos).
    Bike,
    /// Véhicule volant ordinaire du trafic ou d'un convoi.
    Craft,
    /// Gros porteur : cargo, dirigeable, vaisseau publicitaire.
    Heavy,
    /// Astre qui tombe.
    Meteor,
    /// Panneau publicitaire lisible, au bord de la route.
    Sign,
    /// Structure d'un événement (immeuble-vitrine, méga-tour, route croisée).
    Landmark,
    /// L'enseigne géante du DJ : `position` = centre des lettres, `size` = largeur totale, `facing` = +z.
    DjSign,
}

/// Un sujet filmable, recopié à chaque image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Subject {
    /// Stable tant que le sujet vit ; unique pour un même `kind`. Un plan qui suit un sujet le retrouve par
    /// (`kind`, `id`).
    pub id: u64,
    /// Genre du sujet.
    pub kind: SubjectKind,
    /// Position absolue (m).
    pub position: DVec3,
    /// Vitesse dans le monde (m/s), nulle pour un panneau.
    pub velocity: Vec3,
    /// Taille caractéristique (m) : longueur d'un véhicule, rayon d'un astre, largeur d'un panneau, hauteur d'une
    /// structure.
    pub size: f32,
    /// Panneaux : direction (unitaire) vers laquelle la face lisible est tournée.
    pub facing: Option<Vec3>,
}

/// Les sujets de l'image. Écrits par les systèmes de `WorldSet::Subjects` (vidés puis remplis à chaque image), lus
/// par le réalisateur dans `CfSet::Direct`.
#[derive(Resource, Debug, Clone)]
pub struct Subjects {
    list: Vec<Subject>,
    dropped: u32,
}

impl Default for Subjects {
    fn default() -> Self {
        Self {
            list: Vec::with_capacity(SUBJECT_CAPACITY),
            dropped: 0,
        }
    }
}

impl Subjects {
    /// Tous les sujets de l'image.
    #[must_use]
    pub fn as_slice(&self) -> &[Subject] {
        &self.list
    }

    /// Les sujets d'un genre.
    pub fn of_kind(&self, kind: SubjectKind) -> impl Iterator<Item = &Subject> + '_ {
        self.list.iter().filter(move |s| s.kind == kind)
    }

    /// Le sujet (`kind`, `id`), s'il est encore là.
    #[must_use]
    pub fn find(&self, kind: SubjectKind, id: u64) -> Option<&Subject> {
        self.list.iter().find(|s| s.kind == kind && s.id == id)
    }

    /// Sujets ignorés cette image faute de place (diagnostic : un monde trop généreux).
    #[must_use]
    pub fn dropped(&self) -> u32 {
        self.dropped
    }

    /// Vide la liste sans libérer sa mémoire (début de `WorldSet::Subjects`).
    pub(crate) fn clear(&mut self) {
        self.list.clear();
        self.dropped = 0;
    }

    /// De quoi ajouter des sujets sans jamais dépasser la capacité.
    pub(crate) fn sink(&mut self) -> SubjectSink<'_> {
        SubjectSink {
            list: &mut self.list,
            dropped: &mut self.dropped,
        }
    }
}

/// Où un monde dépose ses sujets : refuse au-delà de [`SUBJECT_CAPACITY`] plutôt que de réallouer.
#[derive(Debug)]
pub struct SubjectSink<'a> {
    list: &'a mut Vec<Subject>,
    dropped: &'a mut u32,
}

impl SubjectSink<'_> {
    /// Ajoute un sujet ; `false` s'il n'y a plus de place (le sujet est compté comme ignoré).
    pub fn push(&mut self, subject: Subject) -> bool {
        if self.list.len() >= SUBJECT_CAPACITY {
            *self.dropped += 1;
            return false;
        }
        self.list.push(subject);
        true
    }

    /// Places restantes cette image.
    #[must_use]
    pub fn remaining(&self) -> usize {
        SUBJECT_CAPACITY.saturating_sub(self.list.len())
    }
}

/// Ce que le cœur fournit au monde pour lister ses sujets.
#[derive(Debug, Clone, Copy)]
pub struct SubjectContext<'a> {
    /// Les tronçons vivants (dans l'ordre des emplacements, pas des index).
    pub chunks: LiveChunks<'a>,
    /// Position absolue du point suivi (voiture ou caméra), m.
    pub focus: DVec3,
}

/// Les tronçons vivants d'un monde, à parcourir sans allocation.
#[derive(Debug, Clone, Copy)]
pub struct LiveChunks<'a> {
    layout: &'a crate::streaming::StreamLayout,
    live: &'a [Option<i64>],
}

impl<'a> LiveChunks<'a> {
    pub(crate) fn new(layout: &'a crate::streaming::StreamLayout, live: &'a [Option<i64>]) -> Self {
        Self { layout, live }
    }

    /// Chaque tronçon vivant, tel que le monde l'a reçu pour le remplir.
    pub fn iter(&self) -> impl Iterator<Item = ChunkInfo> + 'a {
        let layout = self.layout;
        self.live.iter().filter_map(move |index| index.map(|i| layout.chunk(i)))
    }

    /// Nombre de tronçons vivants.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live.iter().filter(|i| i.is_some()).count()
    }

    /// Aucun tronçon vivant (avant la première image).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject(id: u64) -> Subject {
        Subject {
            id,
            kind: SubjectKind::Sign,
            position: DVec3::ZERO,
            velocity: Vec3::ZERO,
            size: 4.0,
            facing: Some(Vec3::Z),
        }
    }

    #[test]
    fn capacite_fixe_jamais_depassee() {
        let mut subjects = Subjects::default();
        let capacity = subjects.list.capacity();
        let mut sink = subjects.sink();
        for id in 0..(SUBJECT_CAPACITY as u64 + 5) {
            sink.push(subject(id));
        }
        assert_eq!(sink.remaining(), 0);
        assert_eq!(subjects.as_slice().len(), SUBJECT_CAPACITY);
        assert_eq!(subjects.dropped(), 5);
        assert_eq!(subjects.list.capacity(), capacity, "aucune réallocation");
        assert!(subjects.find(SubjectKind::Sign, 3).is_some());
        assert!(subjects.find(SubjectKind::Bike, 3).is_none());
        subjects.clear();
        assert_eq!(subjects.as_slice().len(), 0);
    }
}
