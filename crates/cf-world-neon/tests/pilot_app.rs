//! Le pilote dans une vraie `App` Bevy, sans rendu : il crée la voiture (qui porte `StreamFocus`), l'avance sur la
//! route, écrit les commandes et l'animation, et la voiture décolle quand rien ne l'en empêche.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use cf_render::{AbsolutePosition, AudioFeatures, CorePlugin, StreamFocus};
use cf_world_neon::car::{Car, CarControls};
use cf_world_neon::pilot::PilotCar;
use cf_world_neon::{CarAnimation, PilotPlugin, PilotSettings};

const FRAME: Duration = Duration::from_micros(16_667);

fn app(settings: PilotSettings) -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, TransformPlugin, CorePlugin, PilotPlugin { settings }))
        .insert_resource(AudioFeatures(cf_analysis::AudioFeatures {
            live: true,
            loud: 0.6,
            bass: 0.5,
            intensity: 0.8,
            bpm: Some(128.0),
            ..cf_analysis::AudioFeatures::NEUTRAL
        }))
        .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME));
    app
}

#[test]
fn le_pilote_cree_la_voiture_et_la_fait_avancer() {
    let mut app = app(PilotSettings {
        start_z_m: -400_000.0,
        ..PilotSettings::default()
    });
    for _ in 0..600 {
        app.update();
    }
    let world = app.world_mut();
    let mut cars = world.query_filtered::<(&AbsolutePosition, Has<Car>, Has<StreamFocus>), With<PilotCar>>();
    let all: Vec<_> = cars.iter(world).map(|(p, c, f)| (p.0, c, f)).collect();
    assert_eq!(all.len(), 1, "une seule voiture");
    let (position, is_car, focus) = all[0];
    assert!(is_car && focus, "elle porte `Car` et `StreamFocus`");
    assert!(position.z < -400_000.0 - 100.0, "elle avance vers −z : {}", position.z);
    assert!((position.z + 400_000.0).abs() < 2_000.0);
}

#[test]
fn les_commandes_et_l_animation_sont_publiees_et_la_voiture_decolle() {
    let mut app = app(PilotSettings::default());
    let mut took_off = false;
    for _ in 0..(60 * 90) {
        app.update();
        let animation = *app.world().resource::<CarAnimation>();
        took_off |= animation.airborne() && animation.altitude > 1.0;
        assert!(app.world().resource::<CarControls>().steer.abs() <= 1.0);
        assert!(animation.altitude >= 0.0 && animation.altitude < 63.0);
    }
    assert!(
        took_off,
        "sans spectacle les portes sont ouvertes : elle finit par voler"
    );
    // L'altitude de l'animation est celle de la position absolue : la voiture, la traînée et la caméra la suivent.
    let animation = *app.world().resource::<CarAnimation>();
    let world = app.world_mut();
    let mut positions = world.query_filtered::<&AbsolutePosition, With<PilotCar>>();
    let y = positions.single(world).expect("la voiture").0.y;
    assert!(
        (y - f64::from(animation.altitude)).abs() < 1e-3,
        "{y} {}",
        animation.altitude
    );
}
