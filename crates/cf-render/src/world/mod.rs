//! Le trait [`World`] : le contrat que tout univers (Neon City, la ville de boîtes de test, les suivants) remplit
//! pour être streamé, filmé et dosé par le spectacle.
//!
//! Sillage n'avait qu'un monde, câblé « une voiture file vers −z » dans `sillage-scene.ts`. Ici un univers est un
//! plugin qui implémente ce trait ; le cœur de `cf-render` s'occupe du reste, de la même façon pour tous :
//!
//! - **streaming** : une fenêtre de tronçons suit le point suivi ([`StreamFocus`](crate::StreamFocus)) ; chaque
//!   tronçon qui y entre est décrit par le monde ([`World::plan_chunk`]) puis appliqué à des entités préallouées.
//!   Aucun `spawn`/`despawn` après le démarrage ;
//! - **origine flottante** : le monde place ses pièces en repère local de tronçon, ses sujets en absolu (`f64`) ; le
//!   cœur convertit ;
//! - **portes** : à chaque changement des portes du spectacle, le monde est prévenu ([`World::apply_gates`]) ;
//! - **sujets** : à chaque image, le monde liste ce que la caméra peut filmer ([`World::subjects`]).
//!
//! Ce qui reste au monde : ses animations, son trafic, ses matériaux réactifs. Ses systèmes vont dans
//! `CfSet::Simulate`, après [`WorldSet::Chunks`] (les tronçons de l'image sont alors en place).
//!
//! Le nom `World` est celui de l'architecture ; il coïncide avec `bevy::prelude::World`. Un import explicite
//! (`use cf_render::World;`) l'emporte sur le prélude de Bevy, dont le `World` s'écrit alors
//! `bevy::ecs::world::World`.

mod boxes;
mod plugin;
mod subject;

use bevy::ecs::component::Mutable;
use bevy::ecs::system::{ReadOnlySystemParam, SystemParamItem};
use bevy::ecs::world::World as EcsWorld;
use bevy::prelude::*;
use cf_show::model::ShowGates;

use crate::streaming::{ChunkInfo, ChunkPlan, PieceRegistry, StreamLayout};

pub use boxes::BoxWorld;
pub use plugin::{WorldCorePlugin, WorldPlugin};
pub use subject::{LiveChunks, SUBJECT_CAPACITY, Subject, SubjectContext, SubjectKind, SubjectSink, Subjects};

/// Un univers filmable : streamé par tronçons, dosé par les portes, peuplé de sujets.
///
/// Repère commun (celui de Sillage) : on avance vers −z, y vers le haut, unités en mètres et secondes. Le tronçon
/// d'index `i` couvre z ∈ [−(i+1)·L, −i·L] en absolu.
///
/// Règles du contrat :
/// - **déterminisme** : [`World::plan_chunk`] ne dépend que du tronçon (index, graine) et de l'état du monde au moment
///   du tirage (réglages, portes déjà appliquées). Ni horloge, ni hasard global : un tronçon revisité est identique ;
/// - **régime établi sans allocation** : `plan_chunk`, `apply_gates` et `subjects` sont appelés pendant le show ; ils
///   n'allouent pas, ne chargent rien, ne créent aucune entité. Tout se prépare dans [`World::setup`] ;
/// - **un seul écrivain** : la ressource du monde n'est modifiée que par `apply_gates` (via le cœur) et par les
///   systèmes du monde lui-même.
pub trait World: Resource + Component<Mutability = Mutable> + Sized {
    /// Identifiant stable, référencé par les looks (`world: "neon"`).
    const ID: &'static str;

    /// Ce que le monde lit dans l'ECS pour lister ses sujets (`()` s'il n'en a pas besoin). Lecture seule : la liste
    /// des sujets ne modifie rien et peut tourner en parallèle d'autres systèmes.
    type SubjectParam: ReadOnlySystemParam + 'static;

    /// Géométrie du streaming. Lue une fois, avant [`World::setup`].
    fn layout(&self) -> StreamLayout;

    /// Démarrage : crée meshes et matériaux, déclare les familles de pièces et leur capacité par tronçon.
    ///
    /// Accès exclusif à l'ECS (assets, ressources du monde, pools d'entités hors tronçons). Les entités des tronçons
    /// sont créées par le cœur juste après, à partir de `pieces`.
    ///
    /// # Errors
    ///
    /// Une ressource manquante ou un réglage incohérent : le monde reste vide, l'erreur est journalisée.
    fn setup(&mut self, ecs: &mut EcsWorld, pieces: &mut PieceRegistry) -> Result<(), WorldError>;

    /// Décrit le tronçon `chunk` dans `plan` (déjà vide) : pièces en repère local, z ∈ [−L, 0].
    ///
    /// Appelé quand le tronçon entre dans la fenêtre, ou quand un re-tirage a été demandé ([`Replan::All`],
    /// `ChunkStream::replan`). Une famille pleine refuse les pièces suivantes : la capacité déclarée dans `setup`
    /// est un plafond, pas une cible.
    fn plan_chunk(&self, chunk: &ChunkInfo, plan: &mut ChunkPlan);

    /// Réaction aux portes du look courant. Appelée à chaque changement de [`CurrentGates`], donc à chaque image
    /// pendant un fondu de portes (les nombres glissent) : quantifier avant de décider d'un re-tirage.
    ///
    /// [`Replan::Keep`] (cas normal) : seuls les tronçons à venir en tiennent compte ; ils entrent au loin, dans la
    /// brume, et le changement arrive sans saut. [`Replan::All`] : les tronçons vivants sont re-tirés tout de suite
    /// (réservé à ce qui doit changer à l'image près, au prix d'un changement visible).
    fn apply_gates(&mut self, gates: &ShowGates) -> Replan;

    /// Liste les sujets filmables de l'image (positions absolues). Appelé une fois par image, entre
    /// `CfSet::Simulate` et `CfSet::Direct`.
    fn subjects(
        &self,
        ctx: &SubjectContext<'_>,
        param: &SystemParamItem<'_, '_, Self::SubjectParam>,
        out: &mut SubjectSink<'_>,
    );
}

/// Ce que les tronçons déjà vivants deviennent après un changement de portes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Replan {
    /// Ils restent tels quels ; les tronçons à venir suivent les nouvelles portes.
    #[default]
    Keep,
    /// Ils sont tous re-tirés dans l'image.
    All,
}

/// Les portes du look courant, telles que les mondes les voient.
///
/// Écrite par un seul système, le pont du spectacle (J2-05) : copie des portes du `ShowState` de l'image, mise à
/// jour en place (`clone_from`, sans allocation) et seulement quand elles changent. Tant qu'elle n'existe pas (pas
/// de show chargé), les mondes gardent leurs réglages de départ, l'équivalent des portes ouvertes de Sillage.
#[derive(Resource, Debug, Clone, PartialEq, Deref)]
pub struct CurrentGates(pub ShowGates);

/// Erreurs de démarrage d'un monde.
#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    /// Une ressource attendue n'est pas dans l'ECS (plugin d'assets absent, ordre des plugins).
    #[error("ressource absente au démarrage du monde : {0}")]
    MissingResource(&'static str),
    /// Longueur de tronçon nulle ou non finie, ou fenêtre démesurée.
    #[error("géométrie de streaming invalide : {0:?}")]
    InvalidLayout(StreamLayout),
    /// Toute autre erreur propre au monde.
    #[error("démarrage du monde impossible : {0}")]
    Setup(String),
}

/// Les étapes du monde dans l'image. Configurées par [`WorldCorePlugin`] ; les systèmes d'un monde s'y rangent.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorldSet {
    /// `Update`, avant `CfSet::Input` : l'origine flottante de l'image est fixée avant toute logique.
    Origin,
    /// `CfSet::Simulate`, en tête : réaction aux portes.
    Gates,
    /// `CfSet::Simulate`, après `Gates` : la fenêtre de tronçons suit le point suivi, les tronçons entrants sont
    /// remplis. Les systèmes du monde se placent après.
    Chunks,
    /// Entre `CfSet::Simulate` et `CfSet::Direct` : les sujets de l'image sont listés pour le réalisateur.
    Subjects,
    /// `PostUpdate`, avant la propagation des transforms : positions absolues → `Transform`.
    Sync,
}
