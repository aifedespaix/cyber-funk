//! La mince couche Bevy du pilote : crée la voiture, lit l'audio et le spectacle, écrit la position, les commandes et
//! l'animation. Toute la décision est dans [`Pilot`].

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, CfSet, CurrentShow, WorldSet};

use super::animation::CarAnimation;
use super::drive::{Pilot, PilotSettings, PilotTick};
use super::input::{BeatInput, PilotTraffic, ShowInput};
use crate::car::{Car, CarControls, CarRig};

/// Marque la voiture que conduit le pilote.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct PilotCar;

/// Le conducteur, en ressource : un seul propriétaire, le système [`drive_pilot`].
#[derive(Resource, Debug, Clone)]
pub struct PilotBrain(pub Pilot);

/// Ensemble du système du pilote : le trafic (J5-08) y place ses écritures de [`PilotTraffic`] avant lui.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PilotSet;

/// `Startup` : crée la voiture. Elle porte `StreamFocus` (via [`Car`]) : le monde, la route et l'origine flottante la
/// suivent. Un seul porteur de `StreamFocus` doit exister : pas de point suivi provisoire avec le pilote.
pub(super) fn spawn_car(mut commands: Commands, settings: Res<PilotSettings>) {
    commands.spawn((
        Name::new("voiture"),
        Car,
        PilotCar,
        AbsolutePosition(DVec3::new(0.0, 0.0, settings.start_z_m)),
    ));
}

/// `CfSet::Simulate`, avant les portes du monde : décide, puis écrit la position absolue, les commandes de la voiture
/// (volant, freinage) et son animation de vol.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn drive_pilot(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    settings: Res<PilotSettings>,
    mut brain: ResMut<PilotBrain>,
    mut traffic: ResMut<PilotTraffic>,
    mut controls: ResMut<CarControls>,
    mut animation: ResMut<CarAnimation>,
    mut cars: Query<&mut AbsolutePosition, With<PilotCar>>,
) {
    let Ok(mut position) = cars.single_mut() else { return };
    let beat = features.map_or_else(BeatInput::default, |f| BeatInput::from(&f.0));
    // Un dépassement consommé par le pilote : ne pas marquer la ressource modifiée quand il n'y en a pas.
    let overtakes = if traffic.overtakes > 0 {
        std::mem::take(&mut traffic.overtakes)
    } else {
        0
    };
    let frame = brain.0.update(&PilotTick {
        dt: time.delta_secs_f64(),
        beat,
        show: settings.show.or_else(|| show.map(|s| ShowInput::of(&s.0))),
        settings: &settings,
        traffic: &traffic,
        overtakes,
    });

    let next = DVec3::new(frame.car_x, f64::from(frame.animation.altitude), frame.car_z);
    if position.0 != next {
        position.0 = next;
    }
    controls.set_if_neq(CarControls {
        steer: frame.steer as f32,
        brake: frame.brake as f32,
        lit: controls.lit,
        light_scale: controls.light_scale,
    });
    animation.set_if_neq(frame.animation);
}

/// `CfSet::Present` : incline la voiture (montée, esquive, figures). Elle passe après `drive_car`, qui réécrit le lacet
/// de la voiture à chaque image : l'orientation de vol s'y compose sans s'accumuler.
pub(super) fn apply_animation(
    animation: Res<CarAnimation>,
    mut cars: Query<&mut Transform, (With<PilotCar>, With<CarRig>)>,
) {
    for mut root in &mut cars {
        let tilted = root.rotation * animation.orientation();
        root.rotation = tilted;
    }
}

/// Branche le pilote sur l'application.
pub(super) fn build(app: &mut App, settings: PilotSettings) {
    app.insert_resource(PilotBrain(Pilot::new(settings.seed, settings.start_z_m)))
        .insert_resource(settings)
        .init_resource::<PilotTraffic>()
        .init_resource::<CarAnimation>()
        .init_resource::<CarControls>()
        .add_systems(Startup, spawn_car)
        .add_systems(
            Update,
            drive_pilot
                .in_set(PilotSet)
                .in_set(CfSet::Simulate)
                .before(WorldSet::Gates),
        )
        .add_systems(Update, apply_animation.in_set(CfSet::Present));
}
