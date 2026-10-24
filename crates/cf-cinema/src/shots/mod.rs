//! Le catalogue des plans (`cinema/shots/index.ts`).
//!
//! Lot A de J6-03 : les plans voiture (poursuite, habitacle, pare-chocs, travelling, roue, orbite, vue de dessus, hélice,
//! vue de dieu, grue, vertige, bascule vers le ciel, épaule penchée, coupe) et les trois plans de sujets déjà portables
//! (`push-in`, `ride-along`, `dolly-zoom`). Les lots suivants ajoutent leurs plans ici : les animations de la voiture
//! (J6-04, bloc « lot B » ci-dessous), les sujets du monde (J6-05). Le strobe reste hors périmètre.

mod car_cut;
mod car_follow;
mod car_motion;
mod car_orbit;
mod chase;
mod dolly_zoom;
mod push_in;
mod ride_along;
// --- Lot B (J6-04) : vol, cascades, vitesse ---
mod ascension_cam;
mod barrel;
mod bullet_time;
mod car_anim;
mod city_wide;
mod fpv_dive;
mod sky_trip;
mod stunt;
// --- fin du lot B ---
// --- Lot C (J6-05) : sujets du monde ---
mod ad;
mod bikers;
mod dj_sign;
mod impact;
mod landmark;
mod meteor;
mod moon;
mod pursuit;
mod ringworld;
// --- fin du lot C ---

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_flight;
#[cfg(test)]
mod tests_world;

pub use car_cut::Flyby;
pub use car_follow::{Bumper, Cockpit, SideTrack, Wheel};
pub use car_motion::{Crane, Dutch, SkyTilt, Vertigo};
pub use car_orbit::{GodView, Orbit, Spiral, TopDown};
pub use chase::Chase;
pub use dolly_zoom::DollyZoom;
pub use push_in::PushIn;
pub use ride_along::RideAlong;

// --- Lot B (J6-04) ---
pub use ascension_cam::AscensionCam;
pub use barrel::Barrel;
pub use bullet_time::BulletTime;
pub use car_anim::{DodgePass, Takeoff, Touchdown};
pub use city_wide::CityWide;
pub use fpv_dive::FpvDive;
pub use stunt::StuntCam;
// --- fin du lot B ---

// --- Lot C (J6-05) ---
pub use ad::AdGlance;
pub use bikers::Bikers;
pub use dj_sign::DjSign;
pub use impact::MeteorImpactWatch;
pub use landmark::LandmarkReveal;
pub use meteor::MeteorRide;
pub use moon::MoonPass;
pub use pursuit::Pursuit;
pub use ringworld::Ringworld;
// --- fin du lot C ---

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
        // --- Lot B (J6-04) : animations de la voiture, figure, cascades, plans de vitesse ---
        Box::new(Takeoff),
        Box::new(Touchdown),
        Box::new(DodgePass),
        Box::new(Barrel),
        Box::new(StuntCam),
        Box::new(AscensionCam),
        Box::new(BulletTime),
        Box::new(FpvDive),
        Box::new(CityWide),
        // --- fin du lot B ---
        // --- Lot C (J6-05) : sujets du monde (disponibles quand le monde les fournit, J7) ---
        Box::new(AdGlance),
        Box::new(Pursuit),
        Box::new(Bikers),
        Box::new(LandmarkReveal),
        Box::new(MeteorRide),
        Box::new(Ringworld),
        Box::new(MoonPass),
        Box::new(MeteorImpactWatch),
        Box::new(DjSign),
        // --- fin du lot C ---
    ]
}
