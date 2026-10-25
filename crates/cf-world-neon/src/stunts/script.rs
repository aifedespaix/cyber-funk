//! Le contrat d'une cascade (`stunts/types.ts`) et le registre qui les range (`SCRIPTS` de `scripts/index.ts`).
//!
//! Les scripts concrets (J7-07) implémentent [`StuntScript`] et s'inscrivent avec
//! [`StuntAppExt::register_stunt`] (ou [`StuntRegistry::register`]), comme les événements du trafic. Le directeur ne
//! tire que les cascades inscrites.
//!
//! Règles communes (fiche J7) : une cascade passe par les pools (véhicules pris à la flotte avec
//! [`StuntWorld::spawn`], rendus dans [`StuntRun::end`], y compris de force), n'envoie aucun flash ni variation de
//! luminance (règle 7 : une cascade ne touche que la pose de la voiture et ses propres véhicules), et ne fait jamais
//! voler la voiture au niveau 0 (le directeur ne la lance pas : voir [`super::director`]).

use std::fmt;

use bevy::app::App;
use bevy::math::DVec3;
use bevy::prelude::Resource;

use super::kind::{STUNT_COUNT, StuntAlign, StuntKind, StuntTier};
use super::world::StuntWorld;
use crate::pilot::rand::Rand;
use crate::traffic::TrafficBeat;

/// Ce que la cascade veut de la voiture à cet instant. Les angles sont en radians, nuls (à 2π près) en début et en fin.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StuntPose {
    /// Abscisse voulue (m), hors des limites de la route si besoin.
    pub x: f64,
    /// Altitude au-dessus de la route, m.
    pub altitude: f64,
    /// + : le nez se lève (un salto arrière fait +2π).
    pub pitch: f64,
    /// + : roulis à droite.
    pub roll: f64,
    /// Roues repliées, répulseurs allumés.
    pub hover: bool,
    /// Le point que filme la caméra du mode cinéma (la voiture par défaut), absolu, m.
    pub focus: DVec3,
    /// Ralenti du monde demandé cette image : `(facteur 0,1 → 1, durée en s)`. Écrit par la cascade (l'Ascension),
    /// relevé et remis à `None` par le directeur, qui le transmet au chef du spectacle (`requestTimeScale`).
    pub time_scale: Option<(f32, f32)>,
}

/// Le spectacle vu des cascades : les portes du look et ce que le chef du spectacle a décidé.
#[derive(Debug, Clone, Copy)]
pub struct StuntShow<'a> {
    /// Les portes du look (`stunt_kinds`, `stunt_tiers`, `stunt_gap_scale`, `ascension`, `max_floor`).
    pub gates: &'a cf_show::model::ShowGates,
    /// Mode respiration : aucune cascade.
    pub breathing: bool,
    /// Le drop frappe maintenant (une image).
    pub drop_now: bool,
    /// La cascade préparée pendant la montée du drop.
    pub planned: Planned,
}

/// La cascade que le chef des drops a préparée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Planned {
    /// Aucune.
    #[default]
    None,
    /// Une cascade du catalogue.
    Kind(StuntKind),
    /// Un identifiant que le catalogue ne connaît pas : le drop la laisse tomber (voir `validate`).
    Unknown,
}

/// Ce que les cascades lisent à chaque image (`StuntEnv`).
#[derive(Debug, Clone, Copy)]
pub struct StuntEnv<'a> {
    /// Durée de l'image, s.
    pub dt: f64,
    /// Horloge de la scène, s.
    pub time: f64,
    /// Abscisse de la voiture (conduite normale), m.
    pub car_x: f64,
    /// Profondeur de la voiture, m.
    pub car_z: f64,
    /// Altitude de la voiture hors cascade, m.
    pub altitude: f64,
    /// Vitesse du défilement de la route, m/s.
    pub speed: f64,
    /// Durée d'un temps (s), figée au départ de la cascade et ramenée entre 0,4 et 0,75 s.
    pub period: f64,
    /// L'audio de l'image.
    pub beat: TrafficBeat,
    /// Le spectacle ; `None` hors spectacle.
    pub show: Option<StuntShow<'a>>,
}

/// Tout ce qu'une cascade manipule pendant qu'elle se déroule.
pub struct StuntCtx<'a> {
    /// Les entrées de l'image.
    pub env: &'a StuntEnv<'a>,
    /// La ville et ses véhicules.
    pub world: &'a mut dyn StuntWorld,
    /// Le hasard (graine injectée, jamais global).
    pub rng: &'a mut Rand,
}

impl fmt::Debug for StuntCtx<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StuntCtx")
            .field("env", self.env)
            .finish_non_exhaustive()
    }
}

/// Une cascade en cours (`StuntRun`).
pub trait StuntRun: Send + Sync {
    /// Durée prévue (s) : la pose s'estompe sur la fin.
    fn duration(&self) -> f64;

    /// Écrit la pose à `t` secondes du départ. `false` : la cascade est finie.
    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool;

    /// Libère les véhicules. Appelé une fois, en fin de course ou de force.
    fn end(&mut self, world: &mut dyn StuntWorld);
}

/// Un script de cascade (`StuntScript`).
pub trait StuntScript: Send + Sync {
    /// Laquelle.
    fn kind(&self) -> StuntKind;
    /// Rang de rareté.
    fn tier(&self) -> StuntTier;
    /// Poids de tirage dans son rang.
    fn weight(&self) -> f64;
    /// Départ : mesure, phrase ou drop.
    fn align(&self) -> StuntAlign;
    /// Y a-t-il de la place pour la jouer (route dégagée, structure à portée) ?
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool;
    /// Prépare la cascade : la position de départ, les véhicules à poser. Alloue une fois (le `Box`), au départ.
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun>;
}

/// Les cascades que l'on sait jouer. Rempli au démarrage par les plugins (J7-07).
#[derive(Resource, Default)]
pub struct StuntRegistry {
    scripts: [Option<Box<dyn StuntScript>>; STUNT_COUNT],
}

impl fmt::Debug for StuntRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let registered: Vec<&str> = self.iter().map(|s| s.kind().as_str()).collect();
        f.debug_struct("StuntRegistry")
            .field("registered", &registered)
            .finish()
    }
}

impl StuntRegistry {
    /// Inscrit (ou remplace) un script, rangé selon sa [`StuntScript::kind`].
    pub fn register(&mut self, script: Box<dyn StuntScript>) {
        let index = script.kind().index();
        self.scripts[index] = Some(script);
    }

    /// Cette cascade est-elle inscrite ?
    #[must_use]
    pub fn contains(&self, kind: StuntKind) -> bool {
        self.scripts[kind.index()].is_some()
    }

    /// Le script de cette cascade, s'il est inscrit.
    #[must_use]
    pub fn get(&self, kind: StuntKind) -> Option<&dyn StuntScript> {
        self.scripts[kind.index()].as_deref()
    }

    /// Les scripts inscrits, dans l'ordre du répertoire de Sillage.
    pub fn iter(&self) -> impl Iterator<Item = &dyn StuntScript> + '_ {
        self.scripts.iter().filter_map(|s| s.as_deref())
    }

    /// Nombre de scripts inscrits.
    #[must_use]
    pub fn len(&self) -> usize {
        self.iter().count()
    }

    /// Aucun script inscrit.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Inscription de cascades depuis le `build` d'un plugin.
pub trait StuntAppExt {
    /// Inscrit un script au registre des cascades.
    fn register_stunt(&mut self, script: impl StuntScript + 'static) -> &mut Self;
}

impl StuntAppExt for App {
    fn register_stunt(&mut self, script: impl StuntScript + 'static) -> &mut Self {
        self.init_resource::<StuntRegistry>();
        self.world_mut()
            .resource_mut::<StuntRegistry>()
            .register(Box::new(script));
        self
    }
}
