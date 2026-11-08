//! Le double visuel de l'ancienne voiture pendant un changement de DJ, et la mise en scène des deux voitures.
//!
//! La voiture réelle (pilotée, porteuse du `StreamFocus`) devient la **nouvelle** voiture ; l'ancienne est un double sans
//! pilote ni foyer de streaming, qui prend sa place à l'instant de l'échange (invisible : même pose, mêmes couleurs).
//! La frise mène les deux voitures jusqu'au dépassement (`handover_motion`), puis rend la réelle au pilote.
//!
//! Règle 8 : le double est créé une fois, dès que le modèle est décodé (au chargement, avant tout show), et reste dessiné
//! à une échelle invisible quand il est éteint, comme la voiture. Il garde les couleurs de l'ancien DJ ([`PaletteOverride`])
//! pendant que la scène change de thème. Son clignotement est géométrique : aucune variation de lumière (règle 7).

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, Phase, ScenePalette, WorldLayer};

use super::glow::CarPalette;
use super::handover_motion::HandoverMotion;
use super::presence::{MIN_SCALE, tv_scale};
use super::rig::{Car, CarRig, CarState};
use super::spawn::{CarAssets, spawn_car};
use super::switch::{ActiveCar, CarModels};
use super::systems::{apply_pose, car_palette, measure};
use crate::pilot::PilotBrain;
use crate::run::RunStatus;
use crate::run::handover::HANDOVER_S;

/// Marque la racine du double de l'ancienne voiture.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct DoubleCar;

/// Les couleurs que garde le double : celles du DJ qui s'en va, figées au début de la passation.
#[derive(Component, Debug, Clone, Copy)]
pub struct PaletteOverride(pub CarPalette);

/// La racine du double, une fois créée.
#[derive(Resource, Debug, Clone, Copy)]
pub struct DoubleRoot(pub Entity);

/// L'état de la mise en scène des deux voitures.
#[derive(Resource, Debug, Default)]
pub struct HandoverCars {
    motion: HandoverMotion,
    active: bool,
}

/// La voiture réelle : la racine pilotée, pas son double.
type RealCar = (With<Car>, Without<DoubleCar>);
/// Le double, pas la voiture réelle.
type DoubleOnly = (With<DoubleCar>, Without<Car>);

/// Amplitude du bégaiement géométrique du double qui s'éteint (largeur relative).
const GLITCH_SCALE: f32 = 0.18;

/// `CfSet::Simulate` : crée le double dès que le modèle est décodé et que la voiture est posée.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn build_double(
    mut commands: Commands,
    existing: Option<Res<DoubleRoot>>,
    models: Res<CarModels>,
    active: Res<ActiveCar>,
    scene: Res<ScenePalette>,
    mut assets: CarAssets,
    placed: Query<(), (With<Car>, With<CarRig>)>,
) {
    if existing.is_some() || placed.is_empty() {
        return;
    }
    let Some(model) = models.copy_of(active.0) else {
        return;
    };
    let palette = car_palette(&scene);
    let root = commands
        .spawn((
            Name::new("double de l'ancienne voiture"),
            DoubleCar,
            WorldLayer,
            AbsolutePosition::default(),
            Transform::from_scale(Vec3::splat(MIN_SCALE)),
            Visibility::Inherited,
        ))
        .id();
    let rig = spawn_car(&mut commands, root, model, &mut assets, &palette, palette.violet);
    commands
        .entity(root)
        .insert((rig, CarState::new(palette.violet), PaletteOverride(palette)));
    commands.insert_resource(DoubleRoot(root));
}

/// `CfSet::Simulate`, après le pilote et avant les portes du monde : mène les deux voitures pendant une passation et
/// rend la réelle au pilote au dépassement.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn drive_handover(
    time: Res<Time>,
    status: Res<RunStatus>,
    scene: Res<ScenePalette>,
    root: Option<Res<DoubleRoot>>,
    mut cars: ResMut<HandoverCars>,
    mut brain: ResMut<PilotBrain>,
    mut real: Query<(&mut AbsolutePosition, &CarState), RealCar>,
    mut doubles: Query<(&mut AbsolutePosition, &mut PaletteOverride), DoubleOnly>,
) {
    let out = status.0;
    let active = out.phase == Phase::Handover;
    let Some(root) = root else { return };
    let Ok((mut double_position, mut own_palette)) = doubles.get_mut(root.0) else {
        return;
    };
    if !active {
        if cars.active {
            cars.motion.reset();
            cars.active = false;
        }
        return;
    }
    // Au tout début de la passation, le double retient les couleurs de l'ancien DJ (le fondu de thème n'a pas démarré).
    if !cars.active {
        cars.active = true;
        own_palette.0 = car_palette(&scene);
    }
    let Ok((mut position, state)) = real.single_mut() else {
        return;
    };
    let t_s = out.progress * HANDOVER_S;
    let moved = cars.motion.update(
        t_s,
        &out.double,
        &out.arrival,
        (position.0.x, position.0.z, state.speed_mps),
        time.delta_secs(),
    );
    if let Some((x, z)) = moved.double {
        let target = DVec3::new(x, 0.0, z);
        if double_position.0 != target {
            double_position.0 = target;
        }
    }
    if let Some((x, z, speed)) = moved.real {
        let target = DVec3::new(x, position.0.y, z);
        if position.0 != target {
            position.0 = target;
        }
        brain.0.warp(x, z, f64::from(speed));
    }
}

/// `CfSet::Simulate`, après `drive_car` : pose les roues et la caisse du double, et son échelle (extinction « télé »,
/// bégaiement géométrique).
pub(super) fn drive_double_pose(
    time: Res<Time>,
    status: Res<RunStatus>,
    mut doubles: Query<(&AbsolutePosition, &CarRig, &mut CarState, &mut Transform), With<DoubleCar>>,
    mut transforms: Query<&mut Transform, Without<DoubleCar>>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs_f64();
    let stages = status.0.double;
    for (position, rig, mut state, mut root) in &mut doubles {
        measure(&mut state, position.0, dt);
        let input = super::motion::MotionInput {
            dt_s: dt,
            speed_mps: state.speed_mps,
            speed_ratio: (state.speed_mps / super::systems::CRUISE_SPEED_MPS).max(0.0),
            lateral_mps: state.lateral_mps,
            accel_mps2: state.accel_mps2,
            steer: 0.0,
        };
        let pose = state.motion.update(&input);
        apply_pose(&pose, 0.0, rig, &mut root, &mut transforms);
        let mut scale = tv_scale(stages.presence);
        if stages.glitch > 0.0 {
            // 10 Hz, quantifié : la largeur saute, la lumière ne bouge pas.
            let step = (now * 10.0).floor() as i64;
            let h = (step * 7919).rem_euclid(1000) as f32 / 1000.0 - 0.5;
            scale.x *= 1.0 + GLITCH_SCALE * h * stages.glitch;
        }
        let scale = scale.max(Vec3::splat(MIN_SCALE));
        if root.scale != scale {
            root.scale = scale;
        }
    }
}
