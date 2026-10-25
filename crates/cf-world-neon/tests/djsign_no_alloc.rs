//! Aucune allocation par image en régime établi (règle 4) : le chef de l'enseigne du DJ tourne sous `assert_no_alloc`, qui
//! remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime couvre
//! l'enseigne posée (allumage, vague, grésillement, pouls borné), un changement de DJ pendant l'allumage (la chute) et son
//! retour au repos. Une pose ou une reconstruction (nouveau nom) alloue : c'est un événement, pas le régime établi.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_render::ScenePalette;
use cf_world_neon::djsign::{DjSignDirector, ShowView, SignContext};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f32 = 1.0 / 60.0;

fn context(i: u32, dj: &str, dj_seq: u32) -> SignContext<'_> {
    let time = f64::from(i) * f64::from(DT);
    SignContext {
        show: Some(ShowView {
            active: true,
            dj_sign: true,
            dj,
            dj_seq,
            dj_show_seq: 0,
            level: 3,
        }),
        dt: DT,
        time,
        car_z: -time * 60.0,
        kick: i.is_multiple_of(28),
        pulse: if i % 28 < 4 { 1.0 } else { 0.0 },
        bpm: Some(128.0),
        mood: [0.1, 0.4, 0.8],
        intro: None,
        flashes: !(100..200).contains(&i),
    }
}

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let palette = ScenePalette::default();
    let mut director = DjSignDirector::new(Box::new(|_| 0.7), false, 7);
    // L'enseigne vient au premier changement de DJ ; quelques images hors du moniteur pour la pose.
    for i in 0..5 {
        director.update(&context(i, "KRMN", 1), &palette);
    }
    assert!(director.visible());
    assert_no_alloc(|| {
        // Cinq secondes : allumage lettre à lettre, vague, grésillement, flashs permis puis coupés puis permis.
        for i in 5..300 {
            director.update(&context(i, "KRMN", 1), &palette);
        }
    });
    assert!(director.sign().lit() > 0.5);

    // Un nom qui change est un événement : sa reconstruction alloue. Mais la chute qui précède n'alloue pas.
    // L'enseigne est encore loin devant la voiture (520 m) : ses lettres tombent au lieu d'être reposées.
    director.update(&context(300, "NOVA KAITO", 2), &palette);
    assert!(director.sign().falling());
    assert_no_alloc(|| {
        for i in 301..340 {
            director.update(&context(i, "NOVA KAITO", 2), &palette);
        }
    });
}
