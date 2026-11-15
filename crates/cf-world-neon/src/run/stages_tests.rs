//! La frise de l'ouverture (voiture, portique, quartier), de la clôture et de la passation de DJ.

use super::handover::{ArrivalStages, DoubleStages, HANDOVER_MAX_S, handover_stages};
use super::timeline::{
    BUILD_END_S, BUILD_START_S, CameraPlan, FADE_START_S, FOLDED_S, GANTRY_END_S, GANTRY_RISE_S, GANTRY_START_S,
    GONE_S, LAUNCH_S, OPENING_MPS, RunStages, START_S, STOP_S, starting_speed, starting_stages, stopping_stages,
};
use super::variant::HandoverVariant;

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
fn the_opening_is_ordered_car_then_gantry_then_district_then_show() {
    const { assert!(LAUNCH_S < GANTRY_RISE_S && GANTRY_RISE_S < GANTRY_START_S) };
    const { assert!(GANTRY_START_S < BUILD_START_S && BUILD_START_S < GANTRY_END_S) };
    // Le quartier est en place avant le premier cue.
    const { assert!(BUILD_END_S < START_S) };
}

#[test]
fn at_the_start_the_car_is_absent_and_the_district_too() {
    let s = starting_stages(0.0, false);
    assert!(s.car_presence == 0.0 && s.build == 0.0 && s.gantry == 0.0);
}

#[test]
fn the_car_is_fully_there_before_it_moves_so_it_never_appears_on_screen() {
    // Elle prend sa taille et sa couleur à l'arrêt, derrière la caméra ; elle ne démarre qu'ensuite.
    assert!(starting_stages(LAUNCH_S, false).car_presence >= 0.999);
    assert_eq!(starting_speed(LAUNCH_S), 0.0);
    assert!((starting_speed(GANTRY_START_S) - OPENING_MPS).abs() < 1e-3);
}

#[test]
fn the_gantry_reaches_the_car_when_the_district_starts() {
    // Le portique est posé 160 m devant la voiture à `GANTRY_RISE_S` : à la vitesse d'ouverture elle y passe au moment
    // où le quartier commence à se lever (à une seconde près).
    let pass = GANTRY_RISE_S + 160.0 / OPENING_MPS;
    assert!((pass - BUILD_START_S).abs() < 1.0, "passage à {pass} s");
    // Et le portique est encore debout à ce moment-là (il ne redescend que dans le dernier cinquième de son étape).
    assert!(starting_stages(pass + 1.0, false).gantry < 0.8);
}

#[test]
fn the_district_comes_after_the_gantry_and_is_complete_before_the_show() {
    let before = starting_stages(GANTRY_START_S, false);
    assert!(before.car_presence >= 0.999 && before.build == 0.0);
    assert!(starting_stages(BUILD_START_S, false).build <= 0.0);
    assert!(starting_stages(BUILD_START_S + 2.0, false).build > 0.0);
    assert!(starting_stages(BUILD_END_S, false).build >= 0.999);
    assert!(
        starting_stages(START_S, false).gantry >= 0.999,
        "le portique est rentré"
    );
}

#[test]
fn the_camera_is_scripted_through_the_whole_opening_and_closing() {
    for k in 0..=(START_S as u32) {
        assert_eq!(starting_stages(k as f32, false).camera, CameraPlan::Opening);
    }
    for k in 0..=(STOP_S as u32) {
        assert_eq!(stopping_stages(k as f32).camera, CameraPlan::Closing);
    }
}

#[test]
fn at_the_close_the_car_leaves_before_the_district_is_gone_and_everything_returns_to_waiting() {
    const { assert!(FADE_START_S < GONE_S && GONE_S < FOLDED_S && FOLDED_S < STOP_S) };
    assert!(stopping_stages(FADE_START_S).car_presence >= 0.999);
    assert_eq!(stopping_stages(GONE_S).car_presence, 0.0);
    let end = stopping_stages(STOP_S);
    let wait = RunStages::WAITING;
    assert!((end.road - wait.road).abs() < 1e-6 && (end.sky - wait.sky).abs() < 1e-6);
    assert!(end.build <= wait.build && end.collapse == 0.0 && end.sign == 0.0 && end.music == 0.0);
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
    for k in 0..=(STOP_S as u32 * 10) {
        let s = stopping_stages(k as f32 / 10.0);
        assert!(s.gantry == 0.0 && s.name_glitch == 0.0 && s.camera == CameraPlan::Closing);
    }
}

fn variant(number: u32) -> HandoverVariant {
    HandoverVariant::new(number, 1, None)
}

#[test]
fn a_handover_lasts_about_forty_five_seconds_and_never_more_than_the_ceiling() {
    for number in 0..200 {
        let d = variant(number).duration_s();
        assert!((40.0..=HANDOVER_MAX_S).contains(&d), "{d}");
    }
}

#[test]
fn the_district_stumbles_for_a_while_then_sinks_and_comes_back_only_after_the_gantry() {
    let v = variant(0);
    let m = v.marks();
    let (start, ..) = handover_stages(0.0, &v, false);
    assert!(start.build >= 0.999 && start.collapse == 0.0 && start.windows >= 0.999);
    // Le repli démarre lentement : bien avant la moitié du temps, moins du quart est tombé (les tours buguent d'abord).
    let (early, ..) = handover_stages(m.folded * 0.4, &v, false);
    assert!(
        early.collapse > 0.0 && early.collapse < 0.35 && early.build >= 0.999,
        "{early:?}"
    );
    let (late, ..) = handover_stages(m.folded - 0.5, &v, false);
    assert!(late.collapse > 0.85, "{late:?}");
    let (hidden, ..) = handover_stages(m.folded + 0.1, &v, false);
    assert!(
        hidden.build == 0.0 && hidden.collapse == 0.0,
        "caché : rendu à l'état d'attente"
    );
    let (under_gantry, ..) = handover_stages(m.gantry + 2.5, &v, false);
    assert!(under_gantry.build == 0.0 && under_gantry.gantry > 0.0);
    let (end, ..) = handover_stages(m.end, &v, false);
    assert!(end.build >= 0.999 && end.collapse == 0.0 && end.gantry >= 0.999);
}

#[test]
fn the_old_car_is_on_screen_at_once_and_the_new_one_is_absent_far_behind() {
    let v = variant(0);
    let (s, d, a) = handover_stages(0.1, &v, false);
    assert!(d.presence >= 0.999, "le double a pris la place de l'ancienne");
    assert!(s.car_presence == 0.0, "la voiture réelle est partie");
    assert!(
        a.gap_behind_m > 100.0 && a.scripted,
        "elle arrivera de loin derrière : {a:?}"
    );
}

#[test]
fn the_new_car_synchronises_while_it_catches_up_and_the_old_one_desynchronises_once_side_by_side() {
    let var = variant(3);
    let m = var.marks();
    let (sync_stage, _, arr) = handover_stages(m.arrival + 1.0, &var, false);
    assert!(sync_stage.car_presence > 0.0 && sync_stage.car_presence < 1.0 && arr.gap_behind_m > 0.0);
    let (sync_stage, dbl, arr) = handover_stages(m.abreast + 0.2, &var, false);
    assert!(
        sync_stage.car_presence >= 0.999 && dbl.presence >= 0.999 && arr.gap_behind_m < 5.0,
        "{arr:?}"
    );
    assert!(dbl.lateral >= 0.999, "l'ancienne s'est rangée sur le côté");
    let (_, dbl, _) = handover_stages(m.fading + 1.0, &var, false);
    assert!(dbl.presence < 1.0 && dbl.presence > 0.0 && dbl.fade > 0.0);
    let (_, dbl, _) = handover_stages(m.gone, &var, false);
    assert!(dbl.presence == 0.0, "l'ancienne a disparu avant le portique");
    assert!(m.gone < m.gantry);
}

#[test]
fn the_camera_goes_in_front_of_the_lone_car_then_lets_go_after_it_is_gone() {
    let v = variant(0);
    let m = v.marks();
    let camera = |t: f32| handover_stages(t, &v, false).0.camera;
    assert_eq!(camera(0.5), CameraPlan::Free);
    assert_eq!(camera(m.camera + 0.5), CameraPlan::Handover);
    assert_eq!(camera(m.fading), CameraPlan::Handover);
    assert_ne!(camera(m.gone + 1.1), CameraPlan::Handover);
    // La présentation du nouveau DJ reprend le plan d'ouverture : portique puis grue sur la ville qui se relève.
    assert_eq!(camera(m.gantry), CameraPlan::Opening);
    assert_eq!(camera(m.end - 0.1), CameraPlan::Opening);
}

#[test]
fn the_pilot_gets_the_new_car_back_before_the_gantry() {
    let v = variant(0);
    let m = v.marks();
    assert!(handover_stages(m.release - 0.1, &v, false).2.scripted);
    assert!(!handover_stages(m.release + 0.1, &v, false).2.scripted);
    assert!(m.release < m.gantry);
}

#[test]
fn handover_stages_stay_finite_in_range_and_the_glitch_is_removed_by_reduced_motion() {
    for number in 0..20 {
        let var = variant(number);
        let mut max_glitch = 0.0_f32;
        for k in 0..=(var.duration_s() / DT) as u32 {
            let time = k as f32 * DT;
            let (stage, dbl, arr) = handover_stages(time, &var, false);
            assert!(all_unit(&stage), "{time}: {stage:?}");
            assert!(
                [dbl.presence, dbl.slow, dbl.lateral, dbl.fade].into_iter().all(unit),
                "{time}: {dbl:?}"
            );
            assert!(
                arr.gap_behind_m.is_finite() && arr.gap_behind_m >= 0.0,
                "{time}: {arr:?}"
            );
            max_glitch = max_glitch.max(stage.name_glitch);
            let (calm, ..) = handover_stages(time, &var, true);
            assert!(calm.name_glitch.abs() < f32::EPSILON);
        }
        assert!(max_glitch > 0.5, "le glitch du nom existe sans reduce_motion");
    }
}

#[test]
fn the_stages_never_go_backwards_where_they_should_not() {
    let v = variant(5);
    let mut prev = handover_stages(0.0, &v, false);
    for k in 1..=(v.duration_s() / DT) as u32 {
        let cur = handover_stages(k as f32 * DT, &v, false);
        assert!(cur.1.slow >= prev.1.slow - 1e-6 && cur.1.lateral >= prev.1.lateral - 1e-6);
        assert!(
            cur.2.gap_behind_m <= prev.2.gap_behind_m + 1e-4,
            "la nouvelle ne s'éloigne jamais"
        );
        prev = cur;
    }
}

#[test]
fn defaults_describe_a_neutral_double_and_arrival() {
    assert_eq!(DoubleStages::NONE.presence, 0.0);
    const { assert!(!ArrivalStages::NONE.scripted) };
}
