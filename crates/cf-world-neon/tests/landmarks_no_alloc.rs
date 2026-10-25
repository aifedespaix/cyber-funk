//! Aucune allocation par image en régime établi (règle 4) : chaque immeuble-vitrine tourne sous `assert_no_alloc`, qui
//! remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime couvre le pas
//! de la scène, le pouls borné, les limiteurs de flashs, les lumières du champ, la partie d'arcade, les robots et la pose de
//! toutes les pièces, avec des kicks, des drops et les flashs permis.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::math::DVec3;
use cf_analysis::Section;
use cf_world_neon::landmarks::pass::{LandmarkPass, PassStart};
use cf_world_neon::landmarks::{LandmarkKind, LandmarkStage};
use cf_world_neon::traffic::{TrafficBeat, TrafficDrive, TrafficFrame, TrafficShow};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f32 = 1.0 / 60.0;

fn frame(i: u32, flashes: bool) -> TrafficFrame<'static> {
    let time = f64::from(i) * f64::from(DT);
    TrafficFrame {
        dt: DT,
        time,
        car: DVec3::new(0.0, 0.0, -time * 60.0),
        beat: TrafficBeat {
            kick: i.is_multiple_of(28),
            bass: 0.6,
            mid: 0.5,
            high: if i % 40 < 20 { 0.8 } else { 0.1 },
            pulse: if i % 28 < 4 { 1.0 } else { 0.0 },
            count: i / 28,
            bpm: Some(128.0),
            ..TrafficBeat::SILENT
        },
        drive: TrafficDrive {
            drop: i % 400 == 100,
            intensity: 0.7,
            section: if i % 900 < 450 { Section::Groove } else { Section::Peak },
        },
        show: Some(TrafficShow {
            level: 3,
            breathing: false,
            flashes,
            ground_traffic: 1.0,
            event_rate: 1.0,
            events: &[],
            city_pulse: 0.0,
            titles: false,
            dj_seq: 0,
            level_seq: 0,
        }),
        ..TrafficFrame::default()
    }
}

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let mut stage = LandmarkStage::new();
    for kind in LandmarkKind::ALL {
        let mut pass = LandmarkPass::begin(
            &mut stage,
            kind,
            0.0,
            PassStart {
                side: 1,
                tint: 1,
                scale: 1.0,
                debug_center: Some(100.0),
            },
        );
        // Quelques images hors du moniteur : la première image d'un passage peut remplir des tampons.
        for i in 0..30 {
            pass.update(&mut stage, &frame(i, true), DT);
        }
        assert_no_alloc(|| {
            // Dix secondes : plusieurs kicks, deux drops, un changement de section, flashs permis puis coupés.
            for i in 30..630 {
                pass.update(&mut stage, &frame(i, i < 400), DT);
            }
        });
        pass.end(&mut stage);
    }
}
