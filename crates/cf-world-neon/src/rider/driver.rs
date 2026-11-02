//! Le conducteur de la voiture : le manul, assis au volant de la « dirty car », figé dans sa pose (`pose_car` de
//! `tools/char/pose.py`, GLB `characters/driver-car.glb`). Il est posé sur la caisse : il suit la suspension.
//!
//! Il n'est posé que sur la dirty car : la pose est calée sur son siège, son volant et ses pédales (la `CyberCar` a une autre
//! habitacle). Un changement de voiture détruit la caisse et son conducteur ; le suivant est reposé sur la nouvelle caisse.
//!
//! Décodé une fois à l'ouverture (règle 8), puis cloné à chaque pose de la voiture. Sans fichier, la voiture roule sans
//! conducteur : le show continue.

use std::path::PathBuf;

use bevy::ecs::system::{Local, Query, Res};
use bevy::math::{EulerRot, Quat, Vec3};
use bevy::prelude::{Commands, Component, Entity, Resource, Time, Transform, With, Without};

use super::arm_ik::turned_grip;
use super::gearbox::{Gear, LEVER_LENGTH_M, Shift, lever_angles};
use super::spawn::{DriverNodes, RiderAssets, spawn_model};
use crate::car::model::CarModelAsset;
use crate::car::{ActiveCar, Car, CarRig, CarState, CarVariant};

/// Chemin du GLB du conducteur (dans le dossier d'assets de la voiture).
#[derive(Resource, Debug, Clone)]
pub struct DriverSource(pub PathBuf);

/// Le conducteur décodé : un exemplaire intact, cloné à chaque pose de la voiture.
#[derive(Resource, Debug)]
pub struct DriverModel(pub CarModelAsset);

/// Marque l'entité racine du conducteur.
#[derive(Component, Debug, Clone, Copy)]
pub struct Driver;

/// `Startup` : lit et décode le conducteur. Un fichier absent ou illisible est dit au journal, sans arrêter le show.
pub(crate) fn load_driver(mut commands: Commands, source: Res<DriverSource>) {
    let decoded = std::fs::read(&source.0)
        .map_err(|error| error.to_string())
        .and_then(|bytes| CarModelAsset::from_glb(&bytes).map_err(|error| error.to_string()));
    match decoded {
        Ok(model) => commands.insert_resource(DriverModel(model)),
        Err(error) => {
            tracing::warn!(%error, path = %source.0.display(), "conducteur illisible : la voiture roule sans lui");
        }
    }
}

/// `CfSet::Simulate`, après le choix de la voiture : pose le conducteur sur la caisse de la dirty car quand elle vient
/// d'être posée (la caisse change d'entité à chaque changement de voiture).
pub(crate) fn attach_driver(
    mut commands: Commands,
    cars: Query<&CarRig, With<Car>>,
    active: Res<ActiveCar>,
    model: Option<Res<DriverModel>>,
    mut assets: RiderAssets,
    mut equipped: Local<Option<Entity>>,
) {
    let (Some(model), Ok(rig)) = (model, cars.single()) else {
        return;
    };
    if active.0 != CarVariant::Dirty {
        *equipped = None;
        return;
    }
    if *equipped == Some(rig.chassis) {
        return;
    }
    let (root, spawned) = spawn_model(&mut commands, rig.chassis, model.0.clone(), &mut assets.refs(), None);
    commands
        .entity(root)
        .insert((Driver, DriverRig(spawned.driver), Gearshift::default()));
    *equipped = Some(rig.chassis);
}

/// Les bras du conducteur à animer : posés avec lui, lus par [`animate_driver`].
#[derive(Component, Debug, Clone, Copy)]
pub struct DriverRig(pub DriverNodes);

/// La boîte de vitesses du conducteur : le rapport engagé et le geste en cours. `None` tant que la voiture n'a pas de
/// vitesse mesurée (elle peut démarrer lancée : le rapport est alors posé d'emblée, sans geste).
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct Gearshift {
    /// Rapport engagé (celui de la case où repose le levier).
    pub gear: Option<Gear>,
    /// Changement de rapport en cours.
    pub shift: Option<Shift>,
}

impl Gearshift {
    /// Avance la boîte de `dt_s` pour une voiture à `speed_mps` : un geste se déroule, puis le suivant peut commencer.
    pub fn advance(&mut self, speed_mps: f32, dt_s: f32) {
        let Some(gear) = self.gear else {
            self.gear = Some(Gear::settled(speed_mps));
            return;
        };
        if let Some(shift) = self.shift.as_mut() {
            shift.elapsed_s += dt_s;
            if shift.is_done() {
                self.gear = Some(shift.to);
                self.shift = None;
            }
        }
        if self.shift.is_none() {
            let next = gear_after(self.gear.unwrap_or(gear), speed_mps);
            if let Some(next) = next {
                self.shift = Some(Shift::new(self.gear.unwrap_or(gear), next));
            }
        }
    }

    /// Où est le pommeau (m, dans la grille) et part de la main qui y est (0 volant, 1 pommeau).
    #[must_use]
    pub fn knob_and_hand(&self) -> ((f32, f32), f32) {
        match (self.shift, self.gear) {
            (Some(shift), _) => (shift.knob(), shift.hand_on_lever()),
            (None, Some(gear)) => (gear.gate(), 0.0),
            (None, None) => (Gear::Neutral.gate(), 0.0),
        }
    }
}

/// Le prochain rapport s'il diffère de l'actuel.
fn gear_after(gear: Gear, speed_mps: f32) -> Option<Gear> {
    let next = gear.next(speed_mps);
    (next != gear).then_some(next)
}

/// Un point de l'axe de la colonne de direction (le centre du volant), repère du modèle de la voiture (`steering_column`).
pub const WHEEL_CENTRE: Vec3 = Vec3::new(0.492, 0.997, 0.695);
/// Direction de cet axe, vers le conducteur (Y local du pivot `steering_pivot`, déjà unitaire à 1e-3 près).
pub const WHEEL_AXIS: Vec3 = Vec3::new(0.0, 0.312, -0.951);
/// Pivot du levier de vitesses (`pivot_gear_lever`), repère du modèle de la voiture.
pub const LEVER_PIVOT: Vec3 = Vec3::new(0.001_267, 0.550_308, 0.585_242);
/// Distance du poignet au centre du pommeau quand la main le tient (m) : la paume entoure le pommeau, le poignet reste en
/// retrait, du côté de l'épaule.
pub const GRIP_SETBACK_M: f32 = 0.11;

/// Rotation du pivot du levier pour un pommeau en (`lateral_m`, `fore_m`) : ordre XYZ de `CyberCar.ts` (`rotation.x` =
/// tangage, `rotation.z` = −roulis, car un +Z de glTF envoie le pommeau vers −X).
#[must_use]
pub fn lever_rotation(knob: (f32, f32)) -> Quat {
    let (pitch, roll) = lever_angles(knob.0, knob.1);
    Quat::from_euler(EulerRot::XYZ, pitch, 0.0, -roll)
}

/// Centre du pommeau dans le repère du modèle.
#[must_use]
pub fn knob_position(knob: (f32, f32)) -> Vec3 {
    LEVER_PIVOT + lever_rotation(knob) * Vec3::new(0.0, LEVER_LENGTH_M, 0.0)
}

/// `CfSet::Simulate`, après `drive_car` : le conducteur suit le volant. Ses poignets tournent avec la prise (les mains
/// restent sur la jante), ses bras se replient par cinématique inverse. Quand la vitesse change de rapport, la main droite
/// quitte la jante, pousse le levier dans sa grille et revient. Aucune allocation, aucune entité créée.
pub(crate) fn animate_driver(
    time: Res<Time>,
    cars: Query<(&CarRig, &CarState), With<Car>>,
    mut drivers: Query<(&DriverRig, &mut Gearshift), With<Driver>>,
    mut transforms: Query<&mut Transform, Without<Car>>,
) {
    let (Ok((rig, state)), Ok((driver, mut gearshift))) = (cars.single(), drivers.single_mut()) else {
        return;
    };
    // Pas de vitesse mesurée avant la deuxième image : la boîte attend, le levier reste au point mort.
    if state.warm {
        gearshift.advance(state.speed_mps, time.delta_secs());
    }
    let (knob, on_lever) = gearshift.knob_and_hand();
    if let Some(mut lever) = rig.gear_lever.and_then(|e| transforms.get_mut(e).ok()) {
        lever.rotation = lever_rotation(knob);
    }
    let Some(angle) = rig
        .steering
        .and_then(|e| transforms.get(e).ok())
        .map(|t| 2.0 * t.rotation.y.atan2(t.rotation.w))
    else {
        return;
    };
    let angle = if angle > std::f32::consts::PI {
        angle - std::f32::consts::TAU
    } else {
        angle
    };
    let axis = WHEEL_AXIS.normalize();
    for (index, arm) in [driver.0.left, driver.0.right].into_iter().enumerate() {
        let Some(arm) = arm else { continue };
        let mut target = turned_grip(arm.geometry.wrist, WHEEL_CENTRE, axis, angle);
        // Seule la main droite (la plus proche du levier) le manœuvre.
        if index == 1 && on_lever > 0.0 {
            let knob = knob_position(knob);
            let grip = knob + (arm.geometry.shoulder - knob).normalize_or_zero() * GRIP_SETBACK_M;
            target = target.lerp(grip, on_lever);
        }
        let (up, low) = arm.geometry.solve(target);
        if let Ok(mut t) = transforms.get_mut(arm.up) {
            t.rotation = up;
        }
        if let Ok(mut t) = transforms.get_mut(arm.low) {
            t.rotation = low;
        }
    }
}
