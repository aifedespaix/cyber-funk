//! `CinemaWorld` : l'état du monde vu par le réalisateur, recomposé à chaque image par le monde (`cinema/world.ts`).
//!
//! Sillage passait au réalisateur un objet qui portait aussi des fonctions (`collides`, `launchMeteor`). En Bevy les
//! données vivent dans une ressource ([`CinemaWorld`], `Copy`, sans allocation) et les questions posées au décor passent
//! par le trait [`Obstacles`], implémenté par la ressource du monde (`NeonWorld`…). Le réalisateur reçoit les deux
//! réunis dans une [`CinemaFrame`], qui se lit comme le `CinemaWorld` de Sillage (`frame.car`, `frame.collides(..)`).

use std::ops::Deref;

use bevy::math::{DVec3, Quat, Vec3};
use bevy::prelude::{Message, Resource};
use cf_render::Subject;

use crate::types::{BeatView, Cabin, CarAnimation, MeteorBlast, RingPlanet, Section, ShowView, Stunt, Trick};

/// Plus grand pas de temps qu'un plan reçoit (s) : un accroc de l'image ne doit pas projeter la caméra.
pub const MAX_DT: f32 = 0.05;

/// L'état du monde vu par le réalisateur. Écrit par un seul système, celui du monde (`CfSet::Simulate`), lu par le
/// réalisateur (`CfSet::Direct`). Positions absolues (`f64`).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct CinemaWorld {
    /// Horloge de la scène (s).
    pub time: f64,
    /// Pas de temps de l'image (s, ≤ [`MAX_DT`]).
    pub dt: f32,
    /// Position de la voiture, altitude comprise (absolue, m).
    pub car: DVec3,
    /// Vitesse de la voiture dans le monde (m/s) : surtout (dx/dt, 0, −speed).
    pub car_velocity: Vec3,
    /// Orientation de la caisse (penche, cabre, tonneaux et saltos compris).
    pub car_rotation: Quat,
    /// Les yeux du conducteur.
    pub cabin: Cabin,
    /// Vitesse le long de la route, m/s.
    pub speed: f32,
    /// Altitude de la voiture au-dessus de la route (m).
    pub altitude: f32,
    /// L'animation de la voiture en cours (vol, roues, esquive, rebond).
    pub animation: CarAnimation,
    /// Figure en cours, sinon `None`.
    pub trick: Option<Trick>,
    /// L'état du rythme.
    pub beat: BeatView,
    /// Section du morceau.
    pub section: Section,
    /// Énergie lissée du morceau, 0 (break) → 1 (sommet).
    pub intensity: f32,
    /// Vrai pendant une seule image : un drop commence.
    pub drop: bool,
    /// La section qui vient de commencer, pendant une seule image ; sinon `None`.
    pub section_changed: Option<Section>,
    /// Musique (ou micro) active : à l'arrêt, le réalisateur garde son plan.
    pub live: bool,
    /// Cascade scénarisée en cours.
    pub stunt: Option<Stunt>,
    /// Le spectacle (inactif sans show chargé).
    pub show: ShowView,
    /// La géante à anneau du ciel, si elle est affichée.
    pub ring_planet: Option<RingPlanet>,
    /// Dernier astre écrasé contre un immeuble.
    pub last_blast: Option<MeteorBlast>,
}

impl Default for CinemaWorld {
    fn default() -> Self {
        Self {
            time: 0.0,
            dt: 0.0,
            car: DVec3::ZERO,
            car_velocity: Vec3::NEG_Z,
            car_rotation: Quat::IDENTITY,
            cabin: Cabin::default(),
            speed: 0.0,
            altitude: 0.0,
            animation: CarAnimation::default(),
            trick: None,
            beat: BeatView::default(),
            section: Section::Groove,
            intensity: 0.4,
            drop: false,
            section_changed: None,
            live: false,
            stunt: None,
            show: ShowView::default(),
            ring_planet: None,
            last_blast: None,
        }
    }
}

/// Ordre au monde de lancer un astre (`launchMeteor` de Sillage), émis par le réalisateur quand le plan `meteor-ride`
/// n'a aucun astre en vol à filmer. Le monde (J7) le lit en `CfSet::Simulate` et fait apparaître un astre dans les
/// [`Subjects`](cf_render::Subjects) ; tant que personne ne le lit, le plan se termine seul après son délai de grâce.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LaunchMeteor;

/// Le décor que la caméra ne doit jamais traverser.
pub trait Obstacles {
    /// Une sphère (centre `at` absolu, rayon `radius` en m) touche-t-elle un immeuble ?
    fn collides(&self, at: DVec3, radius: f64) -> bool;
}

/// Un décor vide (route seule, tests).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoObstacles;

impl Obstacles for NoObstacles {
    fn collides(&self, _at: DVec3, _radius: f64) -> bool {
        false
    }
}

/// Ce qu'un plan reçoit à chaque image : le monde, son décor et ses sujets.
#[derive(Clone, Copy)]
pub struct CinemaFrame<'a> {
    world: &'a CinemaWorld,
    obstacles: &'a dyn Obstacles,
    /// Sujets filmables de l'image (positions absolues).
    pub subjects: &'a [Subject],
}

impl std::fmt::Debug for CinemaFrame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CinemaFrame")
            .field("world", self.world)
            .field("subjects", &self.subjects.len())
            .finish_non_exhaustive()
    }
}

impl<'a> CinemaFrame<'a> {
    /// Réunit le monde de l'image, son décor et ses sujets.
    #[must_use]
    pub fn new(world: &'a CinemaWorld, obstacles: &'a dyn Obstacles, subjects: &'a [Subject]) -> Self {
        Self {
            world,
            obstacles,
            subjects,
        }
    }

    /// Le monde de l'image.
    #[must_use]
    pub fn world(&self) -> &'a CinemaWorld {
        self.world
    }

    /// Une sphère touche-t-elle un immeuble ? Pour ne jamais poser la caméra dans un mur.
    #[must_use]
    pub fn collides(&self, at: DVec3, radius: f64) -> bool {
        self.obstacles.collides(at, radius)
    }

    /// Hauteur du sol sous la voiture (m) : la voiture vole, la caméra ne doit jamais passer dessous.
    #[must_use]
    pub fn ground(&self) -> f64 {
        self.world.car.y - f64::from(self.world.altitude)
    }
}

impl Deref for CinemaFrame<'_> {
    type Target = CinemaWorld;

    fn deref(&self) -> &CinemaWorld {
        self.world
    }
}
