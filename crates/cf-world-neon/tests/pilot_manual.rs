//! Le pilotage manuel de l'onglet Debug : voie visée, volant, décollage, figures (tonneau, vrille), atterrissage.
//! Logique seule, comme `pilot.rs` : pas d'`App`, pas de GPU.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pilot_rig;

use cf_world_neon::pilot::flight::FlightCommand;
use cf_world_neon::pilot::{FlightPhase, Pilot, PilotFrame, PilotTick, TrickKind};
use pilot_rig::{DT, music, quiet, settings, show};

/// Fait avancer `seconds` secondes, au niveau 5 (tout permis) ; rend la dernière image et ce qui s'est passé.
fn run(pilot: &mut Pilot, seconds: f64, playing: bool) -> (PilotFrame, Vec<TrickKind>) {
    let settings = settings(3);
    let traffic = quiet();
    let mut tricks = Vec::new();
    let mut last = *pilot.frame();
    for frame in 0..(seconds / DT) as u64 {
        let mut beat = music(frame, 0.5);
        if !playing {
            beat.live = false;
            beat.kick = false;
        }
        last = pilot.update(&PilotTick {
            dt: DT,
            beat,
            show: Some(show(5, 0.5)),
            settings: &settings,
            traffic: &traffic,
            overtakes: 0,
        });
        tricks.extend(last.events.trick_started);
    }
    (last, tricks)
}

#[test]
fn la_voiture_va_a_la_voie_demandee_et_le_volant_tourne_dans_le_bon_sens() {
    let mut pilot = Pilot::new(3, 0.0);
    run(&mut pilot, 3.0, true);
    pilot.set_manual(Some(1.0));
    let (early, _) = run(&mut pilot, 0.3, true);
    assert!(early.steer > 0.1, "le volant braque à droite : {}", early.steer);
    let (late, _) = run(&mut pilot, 6.0, true);
    assert!(late.car_x > 4.0, "elle a rejoint le bord droit : {}", late.car_x);
    assert!(
        late.steer.abs() < 0.2,
        "le volant se redresse à l'arrivée : {}",
        late.steer
    );
    pilot.set_manual(Some(-1.0));
    let (left, _) = run(&mut pilot, 8.0, true);
    assert!(left.car_x < -4.0, "puis le bord gauche : {}", left.car_x);
}

#[test]
fn meme_sans_musique_la_voiture_se_decale() {
    let mut pilot = Pilot::new(3, 0.0);
    pilot.set_manual(Some(-1.0));
    let (frame, _) = run(&mut pilot, 6.0, false);
    assert!(frame.speed < 1.0, "elle est à l'arrêt : {}", frame.speed);
    assert!(
        frame.car_x < -1.0,
        "mais elle glisse vers la voie demandée : {}",
        frame.car_x
    );
}

#[test]
fn en_manuel_le_pilote_automatique_ne_decolle_jamais() {
    let mut pilot = Pilot::new(7, 0.0);
    pilot.set_manual(Some(0.0));
    let mut airborne = false;
    for _ in 0..30 {
        let (frame, _) = run(&mut pilot, 20.0, true);
        airborne |= frame.animation.airborne();
    }
    assert!(!airborne, "sans ordre, la voiture reste au sol");
}

#[test]
fn decoller_monter_figures_puis_atterrir() {
    let mut pilot = Pilot::new(3, 0.0);
    pilot.set_manual(Some(0.0));
    run(&mut pilot, 1.0, true);
    pilot.command(FlightCommand::Up);
    let (frame, _) = run(&mut pilot, 6.0, true);
    assert_eq!(frame.animation.phase, FlightPhase::Flying);
    assert!(
        frame.animation.altitude > 1.0,
        "elle a décollé : {}",
        frame.animation.altitude
    );

    pilot.command(FlightCommand::Up);
    let (higher, _) = run(&mut pilot, 5.0, true);
    assert!(higher.animation.altitude > frame.animation.altitude, "un étage de plus");
    pilot.command(FlightCommand::Down);
    let (lower, _) = run(&mut pilot, 5.0, true);
    assert!(
        lower.animation.altitude < higher.animation.altitude,
        "un étage de moins"
    );

    for (command, wanted) in [
        (FlightCommand::Roll(-1), TrickKind::RollLeft),
        (FlightCommand::Roll(1), TrickKind::RollRight),
        (FlightCommand::Spin(-1), TrickKind::SpinLeft),
        (FlightCommand::Spin(1), TrickKind::SpinRight),
    ] {
        pilot.command(command);
        let (_, tricks) = run(&mut pilot, 2.5, true);
        assert_eq!(tricks, [wanted], "{command:?}");
    }

    pilot.command(FlightCommand::Land);
    let (down, _) = run(&mut pilot, 12.0, true);
    assert_eq!(down.animation.phase, FlightPhase::Ground);
    assert!(down.animation.altitude < 0.05);
}

#[test]
fn une_vrille_tourne_autour_de_la_verticale_et_revient_droite() {
    let mut pilot = Pilot::new(3, 0.0);
    pilot.set_manual(Some(0.0));
    pilot.command(FlightCommand::Up);
    run(&mut pilot, 6.0, true);
    pilot.command(FlightCommand::Spin(1));
    let settings = settings(3);
    let traffic = quiet();
    let mut widest = 0.0_f32;
    for frame in 0..180 {
        let out = pilot.update(&PilotTick {
            dt: DT,
            beat: music(frame, 0.5),
            show: Some(show(5, 0.5)),
            settings: &settings,
            traffic: &traffic,
            overtakes: 0,
        });
        widest = widest.max(out.animation.trick_yaw.abs());
    }
    assert!(widest > 3.0, "plus d'un demi-tour : {widest}");
    let (done, _) = run(&mut pilot, 1.0, true);
    assert!(
        done.animation.trick_yaw.abs() < 1e-6,
        "fini, la voiture est de nouveau droite"
    );
}

#[test]
fn les_ordres_au_sol_ou_hors_manuel_sont_ignores() {
    let mut pilot = Pilot::new(3, 0.0);
    pilot.command(FlightCommand::Up);
    let (frame, _) = run(&mut pilot, 3.0, true);
    assert!(!frame.animation.airborne() || pilot.is_manual());
    pilot.set_manual(Some(0.0));
    pilot.command(FlightCommand::Roll(1));
    let (_, tricks) = run(&mut pilot, 3.0, true);
    assert!(tricks.is_empty(), "pas de figure au sol");
}
