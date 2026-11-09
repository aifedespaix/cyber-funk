//! Le séquenceur branché sur une application Bevy minimale.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_cinema::{ShotOrder, TransitionKind};
use cf_render::{
    AudioCommand, AudioSequenceGain, ControlAction, CurrentShow, Phase, RunHandover, RunOrder, RunPhase, SheetSlot,
    ShowSheet,
};

use super::{RunPlugin, RunStatus};
use crate::intro::IntroPlugin;

#[derive(Resource, Default)]
struct Seen(Vec<ShotOrder>);

fn collect(mut orders: MessageReader<ShotOrder>, mut seen: ResMut<Seen>) {
    seen.0.extend(orders.read().copied());
}

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .configure_sets(Update, cf_render::CfSet::Simulate)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
        .init_resource::<Seen>()
        .add_plugins((IntroPlugin, RunPlugin))
        .add_systems(Update, collect.after(cf_render::CfSet::Simulate));
    app.update();
    app
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 10.0).round() as u32 {
        app.update();
    }
}

fn phase(app: &App) -> Phase {
    app.world().resource::<RunPhase>().phase
}

#[test]
fn the_world_starts_in_waiting() {
    let app = app();
    assert_eq!(phase(&app), Phase::Waiting);
}

#[test]
fn start_order_moves_phase_to_running() {
    let mut app = app();
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 2.0);
    assert_eq!(phase(&app), Phase::Starting);
    run_seconds(&mut app, 28.0);
    assert_eq!(phase(&app), Phase::Running);
}

#[test]
fn stop_order_returns_to_waiting() {
    let mut app = app();
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 31.0);
    app.world_mut().write_message(RunOrder::Stop);
    run_seconds(&mut app, 2.0);
    assert_eq!(phase(&app), Phase::Stopping);
    run_seconds(&mut app, 8.0);
    assert_eq!(phase(&app), Phase::Waiting);
}

#[test]
fn intro_is_inert_when_sequencer_installed() {
    let mut app = app();
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 31.0);
    let status = app.world().resource::<RunStatus>().0;
    assert_eq!(status.phase, Phase::Running);
    assert!(!app.world().resource::<crate::intro::IntroStatus>().0.running);
}

#[test]
fn camera_orders_follow_the_sequence() {
    let mut app = app();
    run_seconds(&mut app, 1.0);
    assert_eq!(app.world().resource::<Seen>().0, vec![ShotOrder::Hold("waiting")]);
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 31.0);
    assert_eq!(
        app.world().resource::<Seen>().0,
        vec![ShotOrder::Hold("waiting"), ShotOrder::Release],
        "relâchée dès que la voiture est apparue, une seule fois"
    );
    app.world_mut().write_message(RunOrder::Stop);
    run_seconds(&mut app, 10.0);
    assert_eq!(
        app.world().resource::<Seen>().0.last(),
        Some(&ShotOrder::HoldWith("waiting", TransitionKind::Blend))
    );
    assert_eq!(app.world().resource::<Seen>().0.len(), 3);
}

#[test]
fn the_car_is_held_still_while_waiting() {
    let app = app();
    assert_eq!(app.world().resource::<RunStatus>().0.speed_cap_mps, Some(0.0));
}

#[derive(Resource, Default)]
struct Commands(Vec<AudioCommand>);

fn collect_audio(mut orders: MessageReader<AudioCommand>, mut seen: ResMut<Commands>) {
    seen.0.extend(orders.read().cloned());
}

#[test]
fn music_follows_the_sequence_and_pauses_at_the_end_of_stopping() {
    let mut app = app();
    app.init_resource::<Commands>()
        .add_systems(Update, collect_audio.after(cf_render::CfSet::Simulate));
    let gain = |app: &App| app.world().resource::<AudioSequenceGain>().0;
    assert_eq!(gain(&app), 0.0, "silence en attente");
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 2.0);
    assert!(gain(&app) > 0.0 && gain(&app) < 1.0, "la musique monte");
    run_seconds(&mut app, 30.0);
    assert_eq!(gain(&app), 1.0);
    assert!(
        app.world().resource::<Commands>().0.is_empty(),
        "le lecteur est lancé par le reset, pas par une commande"
    );
    app.world_mut().write_message(RunOrder::Stop);
    run_seconds(&mut app, 4.0);
    assert!(gain(&app) < 1.0 && gain(&app) > 0.0, "la musique descend");
    run_seconds(&mut app, 6.0);
    assert_eq!(gain(&app), 0.0);
    assert_eq!(app.world().resource::<Commands>().0, vec![AudioCommand::DemoPause]);
}

// --- Changement de DJ ---

#[derive(Resource, Default)]
struct Controls(Vec<ControlAction>);

fn collect_controls(mut orders: MessageReader<ControlAction>, mut seen: ResMut<Controls>) {
    seen.0.extend(orders.read().cloned());
}

fn dj_sheet(count: usize) -> ShowSheet {
    ShowSheet {
        event_name: "Démesure".to_owned(),
        dj_sign: true,
        slots: (0..count)
            .map(|i| SheetSlot {
                dj: format!("DJ {i}"),
                theme: None,
                logo: None,
                schedule: None,
                cues: Vec::new(),
            })
            .collect(),
    }
}

/// Un monde en cours de show avec `count` DJs, le chef du spectacle sur le DJ `current`.
fn running_app(count: usize, current: usize) -> App {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = cf_show::load::LookLibrary::load_dir(&assets.join("looks")).expect("looks");
    let show = cf_show::load::LoadedShow::load(&assets.join("shows/demo.ron"), &looks).expect("show");
    let director = cf_show::director::ShowDirector::<rand_pcg::Pcg32>::new_at(
        &show,
        cf_show::drop::DropTitles::empty(),
        1,
        current,
    )
    .expect("chef");
    let mut app = app();
    app.init_resource::<Controls>()
        .add_systems(Update, collect_controls.after(cf_render::CfSet::Simulate))
        .insert_resource(dj_sheet(count))
        .insert_resource(CurrentShow(director.state().clone()));
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 31.0);
    assert_eq!(phase(&app), Phase::Running);
    app.world_mut().resource_mut::<Seen>().0.clear();
    app
}

#[test]
fn a_handover_asks_the_director_for_the_next_dj_once_and_publishes_the_target() {
    let mut app = running_app(3, 0);
    app.world_mut().write_message(RunOrder::Handover { slot: None });
    run_seconds(&mut app, 2.0);
    assert_eq!(phase(&app), Phase::Handover);
    assert_eq!(app.world().resource::<RunHandover>().0, Some(1));
    let actions: Vec<_> = app
        .world()
        .resource::<Controls>()
        .0
        .iter()
        .map(|c| c.0.action.clone())
        .collect();
    assert_eq!(
        actions,
        vec![
            cf_control::Action::ArmDj(cf_control::DjIndex(1)),
            cf_control::Action::NextDj
        ]
    );
    run_seconds(&mut app, 53.0);
    assert_eq!(phase(&app), Phase::Running);
    assert_eq!(app.world().resource::<RunHandover>().0, None);
    assert_eq!(app.world().resource::<Controls>().0.len(), 2, "une seule demande");
}

#[test]
fn a_handover_toward_a_chosen_dj_asks_for_that_dj() {
    let mut app = running_app(3, 0);
    app.world_mut().write_message(RunOrder::Handover { slot: Some(2) });
    run_seconds(&mut app, 1.0);
    let first = app.world().resource::<Controls>().0[0].0.action.clone();
    assert_eq!(first, cf_control::Action::ArmDj(cf_control::DjIndex(2)));
}

#[test]
fn nothing_happens_when_there_is_no_other_dj_to_hand_over_to() {
    // Le show de démonstration a deux DJs : le DJ 1 est le dernier.
    let mut app = running_app(2, 1);
    app.world_mut().write_message(RunOrder::Handover { slot: None });
    run_seconds(&mut app, 2.0);
    assert_eq!(phase(&app), Phase::Running, "dernier DJ : pas de passation");
    let mut app = running_app(1, 0);
    app.world_mut().write_message(RunOrder::Handover { slot: None });
    run_seconds(&mut app, 2.0);
    assert_eq!(phase(&app), Phase::Running, "un seul DJ : pas de passation");
    assert_eq!(app.world().resource::<Controls>().0, Vec::<ControlAction>::new());
}

#[test]
fn the_camera_holds_the_handover_shot_from_the_swap_to_the_overtake_then_lets_the_editing_back() {
    let mut app = running_app(3, 0);
    app.world_mut().write_message(RunOrder::Handover { slot: None });
    run_seconds(&mut app, 36.0);
    assert_eq!(
        app.world().resource::<Seen>().0,
        vec![ShotOrder::Hold("handover"), ShotOrder::Release],
        "tenu devant la voiture seule jusqu'à sa disparition, une seule fois"
    );
}
