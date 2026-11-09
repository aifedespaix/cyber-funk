//! La mince couche Bevy du pilote : crée la voiture, lit l'audio et le spectacle, écrit la position, les commandes et
//! l'animation. Toute la décision est dans [`Pilot`].

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{
    AbsolutePosition, AudioFeatures, CfSet, CurrentShow, FlightStatus, ManualDrive, ManualOrder, WorldSet,
};

use super::animation::CarAnimation;
use super::drive::{Pilot, PilotSettings, PilotTick};
use super::flight::{FlightCommand, FlightPhase};
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
    mut manual: Option<ResMut<ManualDrive>>,
    mut cars: Query<&mut AbsolutePosition, With<PilotCar>>,
) {
    let Ok(mut position) = cars.single_mut() else { return };
    if let Some(manual) = manual.as_deref_mut() {
        relay_manual(&mut brain.0, manual);
    }
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
        presence: controls.presence,
        hover: frame.animation.hover_amount,
        bounce_m: frame.animation.bounce,
        wet: controls.wet,
    });
    animation.set_if_neq(frame.animation);
    if let Some(manual) = manual.as_deref_mut() {
        publish_status(&brain.0, manual);
    }
}

/// L'ordre manuel de l'interface, en ordre de vol du pilote.
const fn flight_command(order: ManualOrder) -> FlightCommand {
    match order {
        ManualOrder::TakeOff | ManualOrder::Up => FlightCommand::Up,
        ManualOrder::Down => FlightCommand::Down,
        ManualOrder::Land => FlightCommand::Land,
        ManualOrder::RollLeft => FlightCommand::Roll(-1),
        ManualOrder::RollRight => FlightCommand::Roll(1),
        ManualOrder::SpinLeft => FlightCommand::Spin(-1),
        ManualOrder::SpinRight => FlightCommand::Spin(1),
        ManualOrder::Backflip => FlightCommand::Flip(1),
        ManualOrder::Frontflip => FlightCommand::Flip(-1),
    }
}

/// Reporte sur le pilote l'engagement, la voie visée et les ordres de l'interface. La ressource n'est écrite (donc
/// réveillée) que quand un ordre part : lire ne la marque pas modifiée.
fn relay_manual(pilot: &mut Pilot, manual: &mut ManualDrive) {
    let lane = manual.engaged().then(|| f64::from(manual.lane()));
    if pilot.is_manual() != lane.is_some() || lane.is_some() {
        pilot.set_manual(lane);
    }
    while !manual.pending().is_empty() {
        let Some(order) = manual.pop() else { break };
        // « Décoller » depuis le vol ne monte pas d'un étage de plus : c'est « Monter » qui le fait.
        if order == ManualOrder::TakeOff && pilot.flight().flying() {
            continue;
        }
        pilot.command(flight_command(order));
    }
}

/// Publie l'état du vol pour les boutons de l'interface, seulement s'il a changé.
fn publish_status(pilot: &Pilot, manual: &mut ManualDrive) {
    let flight = pilot.flight();
    let status = FlightStatus {
        flying: flight.phase() == FlightPhase::Flying,
        airborne: flight.flying(),
        tricking: flight.tricking(),
        floor: u8::try_from(flight.floor().max(0)).unwrap_or(0),
    };
    if manual.would_change(status) {
        manual.set_status(status);
    }
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
