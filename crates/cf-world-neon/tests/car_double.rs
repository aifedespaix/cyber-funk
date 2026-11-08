//! Le double de l'ancienne voiture dans une vraie `App` Bevy : créé une fois quand le modèle est décodé, jamais masqué
//! (règle 8), sans foyer de streaming, mené par la frise pendant une passation puis rendu au pilote.

#![allow(clippy::expect_used, clippy::panic)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, AudioFeatures, CorePlugin, CurrentShowFx, Phase, ShowFxInput, StreamFocus};
use cf_world_neon::car::double::{DoubleCar, DoubleRoot, PaletteOverride};
use cf_world_neon::car::handover_motion::PULL_RIGHT_M;
use cf_world_neon::car::{Car, CarPlugin, CarRig, CarVariant, ModelQuality};
use cf_world_neon::pilot::{Pilot, PilotBrain};
use cf_world_neon::run::handover::{HANDOVER_S, HANDOVER_SWAP_S, OVERTAKE_S, handover_stages};
use cf_world_neon::run::{RunSequencer, RunStatus};
use cf_world_neon::{ChasePlugin, NeonWorldPlugin, RunnerConfig};

const FRAME: Duration = Duration::from_micros(16_667);

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        TransformPlugin,
        CorePlugin,
        NeonWorldPlugin::embedded(),
        ChasePlugin {
            config: RunnerConfig {
                start_z_m: -1000.0,
                speed_mps: 0.0,
            },
        },
        CarPlugin {
            quality: ModelQuality::Low,
            variant: CarVariant::Dirty,
            assets_dir: "../../assets".into(),
        },
    ))
    .init_asset::<Image>()
    .init_asset::<Mesh>()
    .init_asset::<StandardMaterial>()
    .insert_resource(AudioFeatures(cf_analysis::AudioFeatures::NEUTRAL))
    .insert_resource(CurrentShowFx(ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    }))
    .insert_resource(PilotBrain(Pilot::new(1, -1000.0)))
    .insert_resource(RunStatus(RunSequencer::new().update(None, 0.0, false)))
    .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

/// Fait tourner jusqu'à ce que la voiture et son double soient posés (le décodage se fait dans un fil à part).
fn built() -> App {
    let mut app = app();
    for _ in 0..4000 {
        app.update();
        if app.world().get_resource::<DoubleRoot>().is_some() {
            for _ in 0..3 {
                app.update();
            }
            return app;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("le double n'a pas été construit : le modèle ne s'est pas chargé");
}

fn set_handover(app: &mut App, t: f32) {
    let (stages, double, arrival) = handover_stages(t, false);
    let mut out = RunSequencer::new().update(None, 0.0, false);
    out.phase = Phase::Handover;
    out.progress = t / HANDOVER_S;
    out.stages = stages;
    out.double = double;
    out.arrival = arrival;
    app.world_mut().resource_mut::<RunStatus>().0 = out;
}

fn double_position(app: &mut App) -> Vec3Like {
    let root = app.world().resource::<DoubleRoot>().0;
    let p = app.world().get::<AbsolutePosition>(root).expect("position du double").0;
    Vec3Like { x: p.x, z: p.z }
}

struct Vec3Like {
    x: f64,
    z: f64,
}

#[test]
fn the_double_exists_after_the_model_is_decoded_and_is_not_a_car() {
    let mut app = built();
    let root = app.world().resource::<DoubleRoot>().0;
    assert!(app.world().get::<DoubleCar>(root).is_some() && app.world().get::<CarRig>(root).is_some());
    assert!(
        app.world().get::<Car>(root).is_none(),
        "ce n'est pas la voiture pilotée"
    );
    assert!(app.world().get::<PaletteOverride>(root).is_some());
    let mut focus = app.world_mut().query_filtered::<Entity, With<StreamFocus>>();
    assert_eq!(focus.iter(app.world()).count(), 1, "un seul foyer de streaming");
    assert!(app.world().get::<StreamFocus>(root).is_none());
}

#[test]
fn the_absent_double_is_tiny_but_still_drawn() {
    let app = built();
    let root = app.world().resource::<DoubleRoot>().0;
    let scale = app.world().get::<Transform>(root).expect("transform").scale;
    // Comme la voiture éteinte : une ligne d'un dix-millième de large, invisible mais dessinée.
    assert!(
        scale.x <= 1e-3 && scale.z <= 1e-3 && scale.min_element() > 0.0,
        "{scale:?}"
    );
    assert_ne!(
        *app.world().get::<Visibility>(root).expect("visibilité"),
        Visibility::Hidden
    );
}

#[test]
fn during_a_handover_the_double_takes_the_old_car_place_and_the_new_car_is_warped_behind() {
    let mut app = built();
    let before = {
        let mut q = app.world_mut().query_filtered::<&AbsolutePosition, With<Car>>();
        q.single(app.world()).expect("voiture").0.z
    };
    set_handover(&mut app, 0.5);
    app.update();
    set_handover(&mut app, HANDOVER_SWAP_S + 0.05);
    app.update();
    let double = double_position(&mut app);
    assert!(
        (double.z - before).abs() < 5.0,
        "le double reprend la place de l'ancienne : {} / {before}",
        double.z
    );
    let warped = app.world().resource::<PilotBrain>().0.frame().car_z;
    assert!(
        (warped - (double.z + 60.0)).abs() < 5.0,
        "la nouvelle est ~60 m derrière : {warped}"
    );
    let root = app.world().resource::<DoubleRoot>().0;
    assert!(
        app.world()
            .get::<Transform>(root)
            .expect("transform")
            .scale
            .min_element()
            > 0.0
    );
}

#[test]
fn the_old_car_ends_stopped_on_the_right_and_the_pilot_gets_the_new_car_back_at_the_overtake() {
    let mut app = built();
    let mut x_start = None;
    let mut released_z = None;
    let steps = (HANDOVER_S * 60.0) as u32;
    for k in 0..steps {
        let t = k as f32 / 60.0;
        set_handover(&mut app, t);
        app.update();
        if t >= HANDOVER_SWAP_S + 0.1 && x_start.is_none() {
            x_start = Some(double_position(&mut app).x);
        }
        if t > OVERTAKE_S + 0.5 && released_z.is_none() {
            released_z = Some(app.world().resource::<PilotBrain>().0.frame().car_z);
        }
    }
    let x_start = x_start.expect("l'échange a eu lieu");
    let end = double_position(&mut app);
    assert!(
        (end.x - x_start - PULL_RIGHT_M).abs() < 1.0,
        "déportée à droite : {} -> {}",
        x_start,
        end.x
    );
    assert!(released_z.is_some());
}
