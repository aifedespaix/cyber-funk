//! Banc de simulation du pilote : les portes viennent des vrais looks de niveau (`assets/looks/niveau-*.ron`), la
//! musique est un kick à 128 bpm. Logique seule, pas d'`App`, pas de GPU.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cf_analysis::Section;
use cf_world_neon::pilot::{BeatInput, PilotGates, PilotSettings, PilotTraffic, ShowInput};

/// Pas de simulation : 60 images par seconde.
pub const DT: f64 = 1.0 / 60.0;

const LOOKS: [&str; 6] = [
    include_str!("../../../../assets/looks/niveau-0-pre-intro.ron"),
    include_str!("../../../../assets/looks/niveau-1-ville.ron"),
    include_str!("../../../../assets/looks/niveau-2-rues.ron"),
    include_str!("../../../../assets/looks/niveau-3-envol.ron"),
    include_str!("../../../../assets/looks/niveau-4-meteores.ron"),
    include_str!("../../../../assets/looks/niveau-5-cosmos.ron"),
];

/// Le spectacle de l'image au niveau `level`, portes lues dans le vrai look.
pub fn show(level: u8, heat: f64) -> ShowInput {
    let look = cf_show::load::parse_look(LOOKS[usize::from(level)], "test").expect("look de niveau");
    let g = look.gates;
    ShowInput {
        level,
        heat,
        breathing: false,
        drop_now: false,
        time_scale: 1.0,
        gates: PilotGates {
            cruise: f64::from(g.cruise),
            flight_rate: f64::from(g.flight_rate),
            max_floor: g.max_floor,
            flight_on_drop_only: g.flight_on_drop_only,
            tricks: g.tricks,
        },
    }
}

/// Un morceau soutenu : kick à 128 bpm, volume qui respire, section groove puis pic.
pub fn music(frame: u64, intensity: f64) -> BeatInput {
    let t = frame as f64 * DT;
    let period = 60.0 / 128.0;
    let kick = (t / period).floor() > ((t - DT).max(0.0) / period).floor() && t > DT;
    BeatInput {
        live: true,
        kick,
        loud: 0.55 + 0.25 * (t / 7.0).sin(),
        bass: 0.5,
        bpm: Some(128.0),
        intensity,
        section: if intensity > 0.7 { Section::Peak } else { Section::Groove },
        count: (t / period).floor() as u32,
    }
}

/// Réglages de test : graine choisie, le reste par défaut.
pub fn settings(seed: u64) -> PilotSettings {
    PilotSettings {
        seed,
        ..PilotSettings::default()
    }
}

/// Une route sans circulation.
pub fn quiet() -> PilotTraffic {
    PilotTraffic::default()
}
