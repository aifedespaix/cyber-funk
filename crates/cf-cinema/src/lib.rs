//! Le réalisateur du mode Cinéma : il choisit et enchaîne les plans sur les temps musicaux, dans les limites des
//! portes du look courant, et écrit la pose de caméra lue par le rig de `cf-render`. Portage de `engine/cinema/` de
//! Sillage.
//!
//! # Contenu (J6-01 : le cœur)
//!
//! - [`types`] : vocabulaire (rangs, transitions, repères, animation de la voiture, spectacle vu par la caméra) ;
//! - [`world`] : [`CinemaWorld`], recomposé à chaque image par le monde, et le trait [`Obstacles`] (collisions) ;
//! - [`shot`] : le contrat [`Shot`] / [`ShotSpec`] ;
//! - [`director`] : [`CinemaDirector`], transitions, bornes, anti-collision, dernier bon cadrage ;
//! - [`cues`], [`pacing`], [`cutter`] : repères musicaux et grille de coupe ;
//! - [`editor`] : tirage des plans (provisoire, le vrai éditeur arrive avec J6-02) ;
//! - [`car_sync`], [`kit`], [`subjects`], [`pose`], [`transition`], [`rng`], [`screen_time`] : outils des plans ;
//! - [`shots`] : le catalogue (un seul plan pour l'instant, `chase`) ;
//! - [`plugin`] : la couche Bevy.
//!
//! # Sécurité du public
//!
//! Le réalisateur ne produit aucun flash : il ne touche qu'à la pose. Une coupe franche n'est pas une variation de
//! luminance au sens du limiteur photosensible, mais un plan qui couperait à chaque temps sera plafonné à
//! [`MAX_CUTS_PER_SECOND`] (J6-02).

pub mod car_sync;
pub mod cues;
pub mod cutter;
pub mod director;
pub mod editor;
pub mod kit;
pub mod pacing;
pub mod plugin;
pub mod pose;
pub mod rng;
pub mod screen_time;
pub mod shot;
pub mod shots;
pub mod subjects;
pub mod transition;
pub mod types;
pub mod world;

pub use director::{
    CAMERA_RADIUS, CinemaDirector, DirectorStats, MAX_AHEAD, MAX_BEHIND, MAX_CUTS_PER_SECOND, MAX_HEIGHT,
};
pub use plugin::{CinemaPlugin, CinemaSet, CinemaWorldPlugin, DEFAULT_SEED};
pub use shot::{Shot, ShotSpec};
pub use types::{
    BeatView, Cabin, CarAnimation, Cue, CueKind, FlightPhase, MeteorBlast, RingPlanet, Section, ShotTier, ShowView,
    Stunt, TransitionKind, Trick, TrickKind,
};
pub use world::{CinemaFrame, CinemaWorld, MAX_DT, NoObstacles, Obstacles};
