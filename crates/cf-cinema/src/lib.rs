//! Le réalisateur du mode Cinéma : il choisit et enchaîne les plans sur les temps musicaux, dans les limites des
//! portes du look courant, et écrit la pose de caméra lue par le rig de `cf-render`. Portage de `engine/cinema/` de
//! Sillage.
//!
//! # Contenu (J6-01 : le cœur, J6-02 : éditeur et chorégraphie)
//!
//! - [`types`] : vocabulaire (rangs, transitions, repères, animation de la voiture, spectacle vu par la caméra) ;
//! - [`world`] : [`CinemaWorld`], recomposé à chaque image par le monde, et le trait [`Obstacles`] (collisions) ;
//! - [`shot`] : le contrat [`Shot`] / [`ShotSpec`] ;
//! - [`director`] : [`CinemaDirector`], transitions, bornes, anti-collision, dernier bon cadrage ;
//! - [`cues`], [`pacing`], [`cutter`] : repères musicaux et grille de coupe ;
//! - [`editor`] : tirage des plans (rang, énergie, cooldown, opportunité, rareté, bonus build/break/drop, plafond de
//!   l'habitacle, respiration, fraîcheur commune) ;
//! - [`choreo`] : mise en scène du spectacle (préparation et frappe du drop, salves, ordres `SkipShot` / `ShowDj`),
//!   plafond de cadence des coupes, et [`ShowSide`], ce que le chef du spectacle fournit ;
//! - [`car_sync`], [`kit`], [`subjects`], [`pose`], [`transition`], [`rng`], [`screen_time`] : outils des plans ;
//! - [`shots`] : le catalogue (plans voiture, lot A de J6-03 ; plans sujets déjà portables) ;
//! - [`plugin`] : la couche Bevy.
//!
//! # Sécurité du public
//!
//! Le réalisateur ne produit aucun flash : il ne touche qu'à la pose. Une coupe franche n'est pas une variation de
//! luminance au sens du limiteur photosensible, mais un plan qui coupe à chaque temps (`strobe`) revient au même pour
//! l'œil. Donc : `strobe` n'est jamais tiré quand les flashs sont coupés ou en respiration, et **toutes** les coupes
//! (transitions `Cut` comme sauts de pose d'un plan) sont plafonnées à [`MAX_CUTS_PER_SECOND`] par seconde glissante.

pub mod car_sync;
pub mod choreo;
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
#[cfg(test)]
mod testkit;
pub mod transition;
pub mod types;
pub mod world;

pub use choreo::{Call, Choreography, NoShow, ShowSide};
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
