//! Le vol de la voiture : étages, décollage, figures, esquives, et les deux pilotes qui lui donnent des ordres.
//!
//! - [`obstacles`] (`flight/obstacles.ts`, `flight/floors.ts`) : tranches d'altitude interdites, part d'esquive ;
//! - [`machine`] (`flight/flight.ts`) : la machine de vol ([`Flight`]) et la pose qu'elle produit ([`FlightFrame`]) ;
//! - [`auto`] (`AutoFlight`) : quand décoller, quel étage, quelle figure, selon les portes du look ;
//! - [`dance`] (`DanceFlight`) : la danse en hauteur, hors spectacle.
//!
//! Tout est pur : le temps et le hasard (graine) sont injectés.

pub mod auto;
pub mod dance;
pub mod machine;
pub mod obstacles;

pub use auto::{AutoContext, AutoFlight, FlightCommand, ceiling};
pub use dance::{DanceContext, DanceFlight};
pub use machine::{
    Flight, FlightEnv, FlightEvents, FlightFrame, FlightPhase, HOVER_SECONDS, Refusal, TrickKind,
};
pub use obstacles::{
    Band, CAR_ABOVE, CAR_BELOW, CLEARANCE, Dodge, DodgeSide, Obstruction, bridge_obstructions, overlaps, resolve_dodge,
};
