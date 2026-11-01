//! Le chemin Bevy du moteur de quartiers (`take_district_triggers` puis `tick_districts`, paramètres système compris)
//! n'alloue pas en régime établi (règle 4). Les deux systèmes tournent dans un `Schedule` mono-fil sur un `World` nu, sous
//! `assert_no_alloc` : régime sans changement, puis une transition forcée (approche, zone, fin). L'ordre de forçage
//! lui-même (qui allonge l'historique du gouverneur) est un événement, hors du régime mesuré.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Duration;

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::ecs::message::Messages;
use bevy::ecs::schedule::SingleThreadedExecutor;
use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, AudioFeatures, DistrictGradeState, DistrictOrder, DistrictState, StreamFocus};
use cf_show::district::{DistrictId, DistrictKind, Governor, TransitionKind};
use cf_world_neon::NeonWorld;
use cf_world_neon::districts::{DistrictBrain, DistrictDriver, take_district_triggers, tick_districts};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f64 = 1.0 / 60.0;

/// Une image : la voiture avance de 1 m (60 m/s), le temps et l'horloge audio suivent, puis les deux systèmes.
fn step(world: &mut World, schedule: &mut Schedule, cars: &mut QueryState<&mut AbsolutePosition, With<StreamFocus>>) {
    for mut position in cars.iter_mut(world) {
        position.0.z -= 1.0;
    }
    world.resource_mut::<Time>().advance_by(Duration::from_secs_f64(DT));
    let elapsed = world.resource::<Time>().elapsed_secs_f64();
    world.resource_mut::<AudioFeatures>().0.time_s = elapsed;
    schedule.run(world);
}

#[test]
fn le_chemin_bevy_n_alloue_pas() {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.init_resource::<Messages<DistrictOrder>>();
    world.insert_resource(NeonWorld::default());
    world.insert_resource(DistrictGradeState::default());
    world.init_resource::<DistrictState>();
    world.init_resource::<DistrictDriver>();
    world.init_resource::<AudioFeatures>();
    world.insert_resource(DistrictBrain(Governor::new(7, [6, 6, 6])));
    world.spawn((StreamFocus, AbsolutePosition(DVec3::new(0.0, 0.0, -400_000.0))));
    let mut cars = world.query_filtered::<&mut AbsolutePosition, With<StreamFocus>>();

    let mut schedule = Schedule::default();
    schedule.set_executor(SingleThreadedExecutor::new());
    schedule.add_systems((take_district_triggers, tick_districts).chain());

    // Échauffement : initialisation du schedule et des paramètres système.
    for _ in 0..60 {
        step(&mut world, &mut schedule, &mut cars);
    }
    assert_no_alloc(|| {
        for _ in 0..600 {
            step(&mut world, &mut schedule, &mut cars);
        }
    });

    let to = DistrictId {
        kind: DistrictKind::Residential,
        variant: 2,
    };
    world.write_message(DistrictOrder::Force {
        to,
        transition: TransitionKind::Gradient,
    });
    // L'image du forçage : le segment est posé dans le plan (événement, il peut allouer).
    step(&mut world, &mut schedule, &mut cars);
    assert_eq!(world.resource::<DistrictState>().target, Some(to));
    // Approche (~49 rangées à 60 m/s : ~21 s), zone, fin : 70 s sans allocation.
    assert_no_alloc(|| {
        for _ in 0..(60 * 70) {
            step(&mut world, &mut schedule, &mut cars);
        }
    });
    let state = *world.resource::<DistrictState>();
    assert_eq!(state.target, None);
    assert_eq!(state.current, to);
}
