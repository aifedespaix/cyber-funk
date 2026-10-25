//! Aucune allocation par image en régime établi (règle 4) : la route d'enseignes, les bandeaux, les limiteurs de flashs et la
//! structure tournent sous `assert_no_alloc`, qui remplace l'allocateur de ce binaire de test et fait avorter le
//! processus à la moindre allocation. Le parcours couvre 60 km de route (des centaines de poses, de fondus, de bugs de
//! portiques, de radars) avec un spectacle actif qui annonce le DJ.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_world_neon::pilot::flight::Obstruction;
use cf_world_neon::route::is_near_bridge;
use cf_world_neon::signs::audio::SignAudio;
use cf_world_neon::signs::flash::{DisplayGate, SignFlash};
use cf_world_neon::signs::road::{RoadFrame, SignRoad};
use cf_world_neon::signs::show_signs::{SignShow, SlotNames};
use cf_world_neon::signs::structure;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let show = SignShow {
        active: true,
        dj_on_ads: true,
        slot: 0,
        slots: vec![SlotNames {
            name: "KRMN".into(),
            event: "DÉMESURE".into(),
        }],
    };
    let mut road = SignRoad::default();
    let mut flash = SignFlash::default();
    let mut gate = DisplayGate::default();
    let mut obstructions: Vec<Obstruction> = Vec::with_capacity(16);
    let audio = SignAudio {
        dt: 1.0 / 60.0,
        mid: 0.4,
        high: 0.5,
        bass: 0.6,
        kick: true,
        snare: true,
        ..SignAudio::default()
    };
    let step = |road: &mut SignRoad,
                flash: &mut SignFlash,
                gate: &mut DisplayGate,
                obstructions: &mut Vec<Obstruction>,
                i: u32| {
        let z = -f64::from(i) * 0.9;
        let time = f64::from(i) / 60.0;
        road.update(
            &RoadFrame {
                car_z: z,
                // La voiture monte et descend : les portiques fantômes buguent quand elle passe à leur hauteur.
                altitude: 12.0 + 3.0 * (time * 0.7).sin(),
                time,
                show: &show,
            },
            is_near_bridge,
        );
        for unit in &mut road.units {
            if unit.state.is_some() {
                unit.ticker.set(1_000.0);
                unit.ticker.step(&audio);
                let _ = unit.ticker.position(&audio, 1.0);
                let _ = gate.step(1.0 / 60.0, time, unit.display.on, unit.display.passing, true);
            }
            if let Some(format) = unit.format {
                let f = format.format();
                let _ = structure::fit(
                    f.mount,
                    unit.width,
                    unit.height,
                    unit.side,
                    unit.layout.y,
                    true,
                    true,
                    unit.round,
                );
            }
            unit.fresh = false;
        }
        flash.update(
            1.0 / 60.0,
            time,
            Some((true, false)),
            if i % 20 < 3 { 1.0 } else { 0.0 },
            i.is_multiple_of(50),
        );
        obstructions.clear();
        road.obstructions(z, obstructions);
    };
    // Échauffement : les tampons grandissent une fois.
    for i in 0..600 {
        step(&mut road, &mut flash, &mut gate, &mut obstructions, i);
    }
    for i in 600..(600 + 60 * 1_000) {
        assert_no_alloc(|| step(&mut road, &mut flash, &mut gate, &mut obstructions, i));
    }
}
