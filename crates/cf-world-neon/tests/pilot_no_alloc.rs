//! Aucune allocation par image en régime établi (règle 4) : le pilote tourne sous `assert_no_alloc`, qui remplace
//! l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime couvre le vol,
//! les figures, les esquives d'arche et la circulation au sol.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pilot_rig;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_world_neon::pilot::flight::{Band, Obstruction};
use cf_world_neon::pilot::{Pilot, PilotTick, PilotTraffic};
use cf_world_neon::route::{PickupKind, PickupView, bridge_z};
use pilot_rig::{DT, music, settings, show};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let settings = settings(21);
    // La circulation est préparée une fois : voitures au sol, véhicule volant, une structure en plus de l'arche.
    let mut traffic = PilotTraffic::default();
    traffic.ground.extend((0..6).map(|i| PickupView {
        kind: PickupKind::Malus,
        x: f64::from(i) - 2.5,
        z: -2000.0 - f64::from(i) * 40.0,
        y: 0.0,
    }));
    traffic.bands.push(Band { lo: 14.0, hi: 18.0 });
    traffic.obstructions.push(Obstruction {
        z_near: -2500.0,
        z_far: -2520.0,
        lo: 19.0,
        hi: 22.0,
    });
    let shows = [show(5, 0.9), show(3, 0.5), show(0, 0.0)];
    // Départ à 400 km, juste avant une arche : le régime passe par décollage, étages, figures et esquive.
    let mut pilot = Pilot::new(21, bridge_z(0) + 800.0);
    // Échauffement hors zone : les tampons du pilote grandissent une fois.
    for frame in 0..600 {
        pilot.update(&PilotTick {
            dt: DT,
            beat: music(frame, 0.9),
            show: Some(shows[0]),
            settings: &settings,
            traffic: &traffic,
            overtakes: 0,
        });
    }
    for frame in 600..(600 + 60 * 240) {
        let show = shows[(frame / 3600) as usize % shows.len()];
        assert_no_alloc(|| {
            pilot.update(&PilotTick {
                dt: DT,
                beat: music(frame, 0.9),
                show: Some(show),
                settings: &settings,
                traffic: &traffic,
                overtakes: u32::from(frame % 900 == 0),
            });
        });
    }
}
