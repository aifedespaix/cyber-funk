//! `Row::generate` n'alloue pas, quartiers actifs et zone de dégradé comprise (règle 4).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_show::district::{DistrictId, DistrictKind};
use cf_world_neon::SceneryConfig;
use cf_world_neon::scenery::layout::Row;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn generate_with_districts_does_not_allocate() {
    let mut config = SceneryConfig::default();
    let to = |kind| DistrictId { kind, variant: 1 };
    assert!(config.push_district(0, 50, to(DistrictKind::Suburb), 20));
    assert!(config.push_district(80, 120, to(DistrictKind::Residential), 20));
    config.set_districts_active(true);
    let mut parts = 0;
    for index in 0..200 {
        assert_no_alloc(|| parts += Row::generate(index, &config).parts().len());
    }
    assert!(parts > 0);
}

/// Le tick du moteur de quartiers (gouverneur, vitesse, fin d'événement, avancement de la transition, copie de l'état)
/// n'alloue pas en régime établi : ni en Peak (aucun changement), ni pendant une transition. Un changement est un
/// événement (le gouverneur y allonge son historique), hors du régime mesuré.
#[test]
fn tick_districts_does_not_allocate() {
    use cf_analysis::Section;
    use cf_render::{DistrictGradeState, DistrictOrder, DistrictState};
    use cf_show::district::{Governor, TransitionKind};
    use cf_world_neon::districts::{DistrictDriver, DistrictInput};

    let mut governor = Governor::new(7, [6, 6, 6]);
    let mut driver = DistrictDriver::default();
    let mut state = DistrictState::default();
    let mut grade = DistrictGradeState::default();
    let input = |i: u32, section| DistrictInput {
        now_s: f64::from(i) / 60.0,
        dt_s: 1.0 / 60.0,
        section,
        section_changed: false,
        intensity: 0.9,
        bass: 0.5,
        high: 0.4,
        bpm: Some(128.0),
        drop_imminent: false,
        drop_now: i.is_multiple_of(1200),
        show_present: true,
        active_events: Some((i / 600 % 3) as usize),
        car_z: Some(-400_000.0 - f64::from(i)),
    };
    for i in 0..60 {
        driver.tick(&mut governor, &input(i, Section::Peak), Some(&mut grade), &mut state);
    }
    assert_no_alloc(|| {
        for i in 60..(60 * 60 * 20) {
            assert!(
                driver
                    .tick(&mut governor, &input(i, Section::Peak), Some(&mut grade), &mut state)
                    .is_none()
            );
        }
    });
    // Un forçage (événement) puis la transition jusqu'au bout, sans allocation.
    let order = DistrictOrder::Force {
        to: DistrictId {
            kind: DistrictKind::Suburb,
            variant: 2,
        },
        transition: TransitionKind::Gradient,
    };
    let base = 60 * 60 * 20;
    let change = driver
        .apply_order(&mut governor, order, f64::from(base) / 60.0, &mut state)
        .unwrap();
    // Le segment commence 5 rangées devant la voiture : approche, entrée dans la zone (l'étalonnage part), traversée.
    let car_row = ((400_000.0 + f64::from(base)) / 26.0).floor() as i64;
    driver.begin(&change, Some(car_row + 5), &mut state);
    assert_no_alloc(|| {
        for i in base..(base + 60 * 30) {
            driver.tick(&mut governor, &input(i, Section::Peak), Some(&mut grade), &mut state);
            // Ce que fait `drive_palette` chaque image : libérer la cible sur un temps fort, avancer le fondu.
            grade.tick_pending(1.0 / 60.0, i.is_multiple_of(28), i / 28);
            grade.update(1.0 / 60.0);
        }
    });
    assert_eq!(state.target, None);
    assert_eq!(state.current.kind, DistrictKind::Suburb);
}
