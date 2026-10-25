//! Aucune allocation par image en régime établi (règle 4) : le pool d'astres, les explosions, les anneaux de brume et la voie
//! des éclairs tournent sous `assert_no_alloc`, qui remplace l'allocateur de ce binaire de test et fait avorter le
//! processus à la moindre allocation. Le parcours couvre dix minutes de spectacle : cadence libre, pluies sur drops,
//! murs qui font exploser les astres, flashs coupés puis rendus.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::math::DVec3;
use cf_world_neon::meteors::{FieldInput, Meteors, ShowGate};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    const DT: f32 = 1.0 / 60.0;
    let anchor = DVec3::new(0.0, 0.0, -20_000.0);
    // Des colonnes de mur de 40 m de large, une sur deux : certains astres les percutent, d'autres filent à la brume.
    let mut wall = |at: DVec3, _: f64| at.y < 45.0 && (at.x / 40.0).floor().rem_euclid(2.0) < 1.0;
    let mut meteors = Meteors::new(11);
    let mut blasts = 0;
    let mut splashes = 0;
    assert_no_alloc(|| {
        for i in 0..(600.0 / DT) as usize {
            let now = f64::from(DT) * i as f64;
            let input = FieldInput {
                dt_s: DT,
                now_s: now,
                anchor: anchor + DVec3::new(0.0, 0.0, -now * 40.0),
                calm: false,
                show: Some(ShowGate {
                    meteor_rate: 20.0,
                    meteor_shower: true,
                    breathing: false,
                    // Un drop toutes les 8 s : les pluies partent dès que la pause de 100 s le permet.
                    drop_now: i % 480 == 0,
                }),
                textures: 7,
            };
            // Les flashs sont coupés une minute sur trois.
            let policy = Some(((i / 3600) % 3 != 1, false));
            meteors.step(&input, policy, &mut wall);
            for event in meteors.field().events() {
                match event {
                    cf_world_neon::meteors::FieldEvent::Blast { .. } => blasts += 1,
                    cf_world_neon::meteors::FieldEvent::Splash { .. } => splashes += 1,
                }
            }
            // Ce que le rendu et les sujets lisent à chaque image.
            let mut subjects = 0;
            meteors.collect_subjects(|_| subjects += 1);
            let _ = (meteors.last_blast(), meteors.flash().level(), meteors.cinema_blast());
        }
    });
    assert!(blasts > 20, "des explosions ont eu lieu : {blasts}");
    assert!(splashes > 0, "des astres se sont éteints dans la brume : {splashes}");
    assert!(meteors.field().spawned() > 100, "beaucoup d'astres sont tombés");
}
