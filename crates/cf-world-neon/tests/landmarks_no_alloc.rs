//! Aucune allocation par image en régime établi (règle 4) : chaque immeuble-vitrine tourne sous `assert_no_alloc`, qui
//! remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime couvre le pas
//! de la scène, le pouls borné, les limiteurs de flashs, les lumières du champ, la partie d'arcade, les robots et la pose de
//! toutes les pièces, avec des kicks, des drops et les flashs permis.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_analysis::Section;
use cf_world_neon::landmarks::frame::FlashPolicy;
use cf_world_neon::traffic::grand::{Landmark, LandmarkFrame};
use cf_world_neon::traffic::{TrafficBeat, TrafficDrive};
use cf_world_neon::{LandmarkHandle, LandmarkKind, StageLandmark};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f32 = 1.0 / 60.0;

fn frame(i: u32) -> LandmarkFrame {
    let time = f64::from(i) * f64::from(DT);
    LandmarkFrame {
        time,
        dt: DT,
        beat: TrafficBeat {
            kick: i.is_multiple_of(28),
            bass: 0.6,
            mid: 0.5,
            high: if i % 40 < 20 { 0.8 } else { 0.1 },
            pulse: if i % 28 < 4 { 1.0 } else { 0.0 },
            count: i / 28,
            beat: f64::from(i) / 28.0,
            bpm: Some(128.0),
            ..TrafficBeat::SILENT
        },
        drive: TrafficDrive {
            drop: i % 400 == 100,
            intensity: 0.7,
            section: if i % 900 < 450 { Section::Groove } else { Section::Peak },
        },
        car: DVec3::new(0.0, 0.0, -time * 60.0),
        calm: false,
        phase: 0.5,
    }
}

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let handle = LandmarkHandle::new();
    for kind in LandmarkKind::ALL {
        let mut landmark = StageLandmark::of(&handle, kind);
        landmark.show(-100.0, 1, LinearRgba::WHITE);
        landmark.set_scale(1.1);
        let live = FlashPolicy {
            flashes: true,
            breathing: false,
        };
        // Quelques images hors du moniteur : la première image d'un passage peut remplir des tampons.
        for i in 0..30 {
            landmark.drive(&frame(i), live, None);
        }
        assert_no_alloc(|| {
            // Dix secondes : plusieurs kicks, deux drops, un changement de section, flashs permis puis coupés.
            for i in 30..630 {
                landmark.drive(&frame(i), if i < 400 { live } else { FlashPolicy::NONE }, Some(0.3));
            }
        });
        landmark.hide();
    }
}
