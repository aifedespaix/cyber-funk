//! Le catalogue des plans (`cinema/shots/index.ts`).
//!
//! Lot A de J6-03 : les plans voiture (poursuite, habitacle, pare-chocs, travelling, roue, orbite, vue de dessus, hélice,
//! vue de dieu, grue, vertige, bascule vers le ciel, épaule penchée, coupe) et les trois plans de sujets déjà portables
//! (`push-in`, `ride-along`, `dolly-zoom`). Les lots suivants ajoutent leurs plans ici : les animations de la voiture
//! (J6-04), les sujets du monde (J6-05). Le strobe reste hors périmètre.

mod car_cut;
mod car_follow;
mod car_motion;
mod car_orbit;
mod chase;
mod dolly_zoom;
mod push_in;
mod ride_along;
#[cfg(test)]
mod tests;

pub use car_cut::Flyby;
pub use car_follow::{Bumper, Cockpit, SideTrack, Wheel};
pub use car_motion::{Crane, Dutch, SkyTilt, Vertigo};
pub use car_orbit::{GodView, Orbit, Spiral, TopDown};
pub use chase::Chase;
pub use dolly_zoom::DollyZoom;
pub use push_in::PushIn;
pub use ride_along::RideAlong;

use crate::shot::ShotSpec;

/// Le catalogue complet du réalisateur, dans l'ordre de Sillage (plans voiture, puis plans sujets). Alloué une fois, au
/// démarrage.
#[must_use]
pub fn catalog() -> Vec<Box<dyn ShotSpec>> {
    vec![
        Box::new(Chase),
        Box::new(Cockpit),
        Box::new(Bumper),
        Box::new(SideTrack),
        Box::new(Wheel),
        Box::new(Orbit),
        Box::new(TopDown),
        Box::new(Spiral),
        Box::new(GodView),
        Box::new(Crane),
        Box::new(Vertigo),
        Box::new(SkyTilt),
        Box::new(Dutch),
        Box::new(Flyby),
        Box::new(PushIn),
        Box::new(RideAlong),
        Box::new(DollyZoom),
    ]
}
