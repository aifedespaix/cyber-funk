//! Tests de la machine de vol.

use super::super::obstacles::{Band, Obstruction};
use super::*;

const EMPTY: FlightEnv<'static> = FlightEnv {
    dt: 1.0 / 60.0,
    car_z: -1000.0,
    speed: 40.0,
    calm: false,
    obstructions: &[],
    bands: &[],
};

fn run(flight: &mut Flight, seconds: f64, env: &FlightEnv<'_>) -> FlightFrame {
    let mut frame = FlightFrame::default();
    for _ in 0..(seconds * 60.0) as usize {
        frame = flight.update(env);
    }
    frame
}

#[test]
fn decolle_monte_a_l_etage_puis_se_pose() {
    let mut f = Flight::default();
    assert!(f.up(&EMPTY));
    assert_eq!(f.phase(), FlightPhase::Deploying);
    let frame = run(&mut f, 6.0, &EMPTY);
    assert_eq!(frame.phase, FlightPhase::Flying);
    assert!((frame.altitude - 3.5).abs() < 0.1, "{}", frame.altitude);
    assert!(f.up(&EMPTY));
    let frame = run(&mut f, 4.0, &EMPTY);
    assert!((frame.altitude - 8.0).abs() < 0.2, "{}", frame.altitude);
    assert!(f.down(&EMPTY) && f.down(&EMPTY));
    let frame = run(&mut f, 8.0, &EMPTY);
    assert_eq!(frame.phase, FlightPhase::Ground);
    assert!(frame.altitude < 0.06);
}

#[test]
fn une_figure_dure_sa_duree_et_un_salto_change_d_etage() {
    let mut f = Flight::default();
    f.up(&EMPTY);
    run(&mut f, 6.0, &EMPTY);
    f.up(&EMPTY);
    run(&mut f, 3.0, &EMPTY);
    assert_eq!(f.floor(), 2);
    assert!(f.do_trick(TrickKind::Backflip, &EMPTY));
    assert_eq!(f.floor(), 3, "le salto arrière monte");
    assert!(!f.do_trick(TrickKind::RollLeft, &EMPTY), "une figure à la fois");
    let mid = run(&mut f, 0.6, &EMPTY);
    assert!(mid.trick_pitch.abs() > 0.5 && mid.trick == Some(TrickKind::Backflip));
    let end = run(&mut f, 0.8, &EMPTY);
    assert!(end.trick.is_none() && end.trick_pitch.abs() < f64::EPSILON);
    assert_eq!(f.events.trick_done, Some(TrickKind::Backflip));
}

#[test]
fn pas_de_figure_en_mode_calme() {
    let mut f = Flight::default();
    f.up(&EMPTY);
    run(&mut f, 6.0, &EMPTY);
    let calm = FlightEnv { calm: true, ..EMPTY };
    assert!(!f.do_trick(TrickKind::RollRight, &calm));
    assert_eq!(f.refusal, Some(Refusal::Calm));
}

#[test]
fn un_vehicule_sur_l_etage_retarde_l_ordre_puis_le_refuse() {
    let mut f = Flight::default();
    f.up(&EMPTY);
    run(&mut f, 6.0, &EMPTY);
    let band = [Band { lo: 6.0, hi: 10.0 }];
    let busy = FlightEnv { bands: &band, ..EMPTY };
    assert!(!f.up(&busy), "étage 2 occupé");
    run(&mut f, 2.0, &busy);
    assert_eq!(f.floor(), 1);
    assert_eq!(f.refusal, Some(Refusal::Traffic));
}

#[test]
fn l_etage_cinq_esquive_le_tablier_de_l_arche() {
    let span = crate::route::bridge_span();
    let deck = [Obstruction {
        z_near: -1000.0 + span.half_depth,
        z_far: -1000.0 - span.half_depth,
        lo: span.lo,
        hi: span.hi,
    }];
    let mut f = Flight::default();
    f.up(&EMPTY);
    run(&mut f, 6.0, &EMPTY);
    for _ in 0..4 {
        f.up(&EMPTY);
        run(&mut f, 2.5, &EMPTY);
    }
    assert_eq!(f.floor(), 5);
    let env = FlightEnv {
        obstructions: &deck,
        car_z: -1000.0,
        ..EMPTY
    };
    let frame = run(&mut f, 2.0, &env);
    assert!(frame.altitude > 33.0, "passe au-dessus du tablier : {}", frame.altitude);
    assert_eq!(frame.dodge, DodgeSide::Over);
}
