//! Le pilote automatique de Neon City (J5-06) : la voiture se conduit seule, au rythme de la musique et dans les portes
//! du look. Portage de `autopilot.ts`, `pace.ts`, `dance.ts` et de `flight/*` (sans `input.ts`), plus ce que
//! `sillage-scene.ts` fait de la voiture à chaque image (inertie, adhérence, volant), sans le jeu.
//!
//! # Contenu
//!
//! - [`pace`] : l'allure selon la musique, l'intensité et la porte `cruise` ;
//! - [`dance`] et [`race_line`] : la chorégraphie sur le temps musical (positions latérales, horloge de danse) et la
//!   ligne de course du spectacle ;
//! - [`steer`] : le volant (coût de chaque position latérale, voitures au sol à dépasser) ;
//! - [`flight`] : la machine de vol (étages, décollage, figures, esquive des arches) et les pilotes de la verticale ;
//! - [`drive`] : [`Pilot`], tout cela réuni en une fonction pure `update(PilotTick) -> PilotFrame` ;
//! - [`animation`] : [`CarAnimation`], ce que le réalisateur (J6) lit pour cadrer un envol ;
//! - [`systems`] : la couche Bevy, mince.
//!
//! # Portes du look
//!
//! | porte | effet |
//! | --- | --- |
//! | `cruise` | multiplie l'allure (lissée sur 2,5 s) |
//! | `max_floor` | plafond de vol ; 0 : la voiture ne décolle jamais (niveau 0) ; la chaleur du set le rabaisse jusqu'à 40 % |
//! | `flight_rate` | vitesse à laquelle l'attente avant un envol s'écoule ; 0 : jamais |
//! | `flight_on_drop_only` | le décollage attend un `drop_now` |
//! | `tricks` | tonneaux et saltos permis en vol |
//!
//! Au niveau 0 le volant est aussi bloqué droit, comme dans Sillage (pré-intro).
//!
//! # Branchement
//!
//! [`PilotPlugin`] crée l'entité [`Car`](crate::Car) (elle porte `StreamFocus`) et la conduit : position absolue,
//! [`CarControls`](crate::CarControls), [`CarAnimation`]. À utiliser **à la place** du point suivi provisoire
//! (`Runner`). Le trafic (J5-08) écrit [`PilotTraffic`] dans [`PilotSet`] ou avant.
//!
//! Aucune allocation par image : les tampons du pilote sont de taille fixe ou réutilisés.

pub mod animation;
pub mod dance;
pub mod drive;
pub mod flight;
pub mod input;
pub mod pace;
pub mod race_line;
pub mod rand;
pub mod steer;
mod systems;

use bevy::prelude::*;

pub use animation::CarAnimation;
pub use dance::{DanceClock, DanceStyle, dance_floor, dance_position};
pub use drive::{Pilot, PilotFrame, PilotSettings, PilotTick};
pub use flight::{DodgeSide, Flight, FlightFrame, FlightPhase, TrickKind};
pub use input::{BeatInput, PilotGates, PilotTraffic, ShowInput};
pub use pace::{CRUISE, Pace, PaceState, to_kmh};
pub use systems::{PilotBrain, PilotCar, PilotSet};

/// Installe le pilote. À ajouter après `CorePlugin`, le monde et [`CarPlugin`](crate::CarPlugin) ; sans point suivi
/// provisoire (`ChasePlugin`) : une seule entité porte `StreamFocus`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PilotPlugin {
    /// Réglages de départ.
    pub settings: PilotSettings,
}

impl Plugin for PilotPlugin {
    fn build(&self, app: &mut App) {
        systems::build(app, self.settings);
    }
}
