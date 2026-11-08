//! Le séquenceur branché sur une application Bevy minimale.

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{Phase, RunOrder, RunPhase};

use super::{RunPlugin, RunStatus};
use crate::intro::IntroPlugin;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .configure_sets(Update, cf_render::CfSet::Simulate)
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
        .add_plugins((IntroPlugin, RunPlugin));
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
    run_seconds(&mut app, 17.0);
    assert_eq!(phase(&app), Phase::Running);
}

#[test]
fn stop_order_returns_to_waiting() {
    let mut app = app();
    app.world_mut().write_message(RunOrder::Start);
    run_seconds(&mut app, 20.0);
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
    run_seconds(&mut app, 20.0);
    let status = app.world().resource::<RunStatus>().0;
    assert_eq!(status.phase, Phase::Running);
    assert!(!app.world().resource::<crate::intro::IntroStatus>().0.running);
}
