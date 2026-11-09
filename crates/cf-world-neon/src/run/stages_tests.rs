//! La frise de l'intro (voiture, portique, quartier) et de la passation de DJ : valeurs de la spec.

use super::handover::{
    ArrivalStages, DoubleStages, HANDOVER_FIRE_S, HANDOVER_S, HANDOVER_SWAP_S, OVERTAKE_S, handover_stages,
};
use super::timeline::{GANTRY_END_S, GANTRY_START_S, RunStages, START_S, starting_stages, stopping_stages};

const DT: f32 = 1.0 / 60.0;

fn unit(v: f32) -> bool {
    v.is_finite() && (0.0..=1.0).contains(&v)
}

fn all_unit(s: &RunStages) -> bool {
    [
        s.road,
        s.lights,
        s.build,
        s.windows,
        s.sky,
        s.sign,
        s.drive,
        s.collapse,
        s.car_presence,
        s.music,
        s.gantry,
        s.name_glitch,
    ]
    .into_iter()
    .all(unit)
}

#[test]
fn the_intro_lasts_29_seconds_with_the_gantry_between_12_and_17() {
    assert!((START_S - 29.0).abs() < f32::EPSILON);
    assert!((GANTRY_START_S - 12.0).abs() < f32::EPSILON && (GANTRY_END_S - 17.0).abs() < f32::EPSILON);
}

#[test]
fn at_the_start_the_car_is_absent_and_the_district_too() {
    let s = starting_stages(0.0, false);
    assert!(s.car_presence == 0.0 && s.build == 0.0 && s.gantry == 0.0);
}

#[test]
fn the_car_is_there_before_the_gantry_and_the_district_comes_after_it() {
    let before = starting_stages(GANTRY_START_S, false);
    assert!(before.car_presence >= 0.999 && before.build == 0.0);
    let mid = starting_stages(GANTRY_START_S.midpoint(GANTRY_END_S), false);
    assert!(mid.gantry > 0.4 && mid.build == 0.0);
    let after = starting_stages(GANTRY_END_S + 1.0, false);
    assert!(after.build > 0.0, "le quartier commence après le portique");
    assert!(starting_stages(START_S, false).build >= 0.999);
}

#[test]
fn intro_stages_stay_in_range_and_never_go_backwards() {
    let mut prev = starting_stages(0.0, false);
    for k in 0..=(START_S / DT) as u32 {
        let s = starting_stages(k as f32 * DT, false);
        assert!(all_unit(&s), "{s:?}");
        assert!(
            s.road >= prev.road - 1e-6 && s.build >= prev.build - 1e-6 && s.car_presence >= prev.car_presence - 1e-6
        );
        assert!(s.gantry >= prev.gantry - 1e-6 && s.sky >= prev.sky - 1e-6);
        prev = s;
    }
}

#[test]
fn the_name_glitch_decays_and_reduced_motion_removes_it() {
    let glitchy = starting_stages(GANTRY_START_S + 1.0, false).name_glitch;
    assert!(glitchy > 0.0);
    assert!(starting_stages(GANTRY_END_S, false).name_glitch < glitchy);
    for k in 0..=(START_S as u32) {
        assert_eq!(starting_stages(k as f32, true).name_glitch, 0.0);
    }
}

#[test]
fn stopping_has_no_gantry_and_no_glitch() {
    for k in 0..=90 {
        let s = stopping_stages(k as f32 / 10.0);
        assert!(s.gantry == 0.0 && s.name_glitch == 0.0 && !s.camera_handover);
    }
}

#[test]
fn the_handover_lasts_27_seconds_and_fires_the_director_at_its_start() {
    assert!((HANDOVER_S - 27.0).abs() < f32::EPSILON);
    assert!(HANDOVER_FIRE_S.abs() < f32::EPSILON);
}

#[test]
fn the_district_folds_then_rebuilds_only_after_the_gantry() {
    let (start, ..) = handover_stages(0.0, false);
    assert!(start.build >= 0.999 && start.collapse == 0.0 && start.windows >= 0.999);
    let (folding, ..) = handover_stages(OVERTAKE_S - 2.0, false);
    assert!(folding.collapse > 0.0 && folding.build >= 0.999);
    let (hidden, ..) = handover_stages(12.0, false);
    assert!(
        hidden.build == 0.0 && hidden.collapse == 0.0,
        "caché : rendu à l'état d'attente"
    );
    let (under_gantry, ..) = handover_stages(GANTRY_START_S.midpoint(GANTRY_END_S) + 1.0, false);
    assert!(under_gantry.build == 0.0 && under_gantry.gantry > 0.0);
    let (end, ..) = handover_stages(HANDOVER_S, false);
    assert!(end.build >= 0.999 && end.collapse == 0.0 && end.gantry >= 0.999);
}

#[test]
fn the_cars_swap_when_the_double_is_in_place_and_the_arrival_is_far_behind() {
    let (before, d0, _) = handover_stages(HANDOVER_SWAP_S - 0.05, false);
    assert!(before.car_presence >= 0.999 && d0.presence == 0.0);
    let (after, d1, a1) = handover_stages(HANDOVER_SWAP_S + 0.05, false);
    assert!(d1.presence >= 0.999, "le double a pris la place de l'ancienne");
    assert!(after.car_presence < 0.5, "la voiture réelle est partie");
    assert!(a1.gap_behind_m > 30.0, "elle arrive de loin derrière : {a1:?}");
}

#[test]
fn the_new_car_overtakes_and_the_old_one_dies_afterwards() {
    let (_, d, a) = handover_stages(OVERTAKE_S - 4.0, false);
    assert!(a.gap_behind_m > 0.0 && d.presence >= 0.999);
    let (_, _, a) = handover_stages(OVERTAKE_S, false);
    assert!(a.gap_behind_m < 0.0, "la nouvelle voiture est passée devant");
    let (_, d, _) = handover_stages(HANDOVER_S, false);
    assert!(d.presence == 0.0, "l'ancienne est éteinte à la fin");
}

#[test]
fn the_camera_holds_the_handover_shot_from_the_swap_to_the_overtake() {
    assert!(!handover_stages(0.0, false).0.camera_handover);
    assert!(handover_stages(HANDOVER_SWAP_S + 0.5, false).0.camera_handover);
    assert!(!handover_stages(OVERTAKE_S + 1.0, false).0.camera_handover);
}

#[test]
fn handover_stages_stay_finite_in_range_and_the_glitch_is_removed_by_reduced_motion() {
    let mut max_glitch = 0.0_f32;
    for k in 0..=(HANDOVER_S / DT) as u32 {
        let t = k as f32 * DT;
        let (s, d, a) = handover_stages(t, false);
        assert!(all_unit(&s), "{t}: {s:?}");
        assert!(
            [d.presence, d.pull_right, d.brake, d.glitch].into_iter().all(unit),
            "{t}: {d:?}"
        );
        assert!(
            a.gap_behind_m.is_finite() && a.lateral_m.is_finite() && unit(a.boost),
            "{t}: {a:?}"
        );
        max_glitch = max_glitch.max(d.glitch);
        let (s, d, _) = handover_stages(t, true);
        assert!(s.name_glitch == 0.0 && d.glitch == 0.0);
    }
    assert!(max_glitch > 0.5, "le glitch existe sans reduce_motion");
}

#[test]
fn defaults_describe_a_neutral_double_and_arrival() {
    assert_eq!(DoubleStages::NONE.presence, 0.0);
    assert_eq!(ArrivalStages::NONE.boost, 0.0);
}
