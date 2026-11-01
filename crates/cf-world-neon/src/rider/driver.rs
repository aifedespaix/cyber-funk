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
use bevy::prelude::{Commands, Component, Entity, Resource, With};

use super::spawn::{RiderAssets, spawn_model};
use crate::car::model::CarModelAsset;
use crate::car::{ActiveCar, Car, CarRig, CarVariant};

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
    let (root, _) = spawn_model(&mut commands, rig.chassis, model.0.clone(), &mut assets.refs(), None);
    commands.entity(root).insert(Driver);
    *equipped = Some(rig.chassis);
}
