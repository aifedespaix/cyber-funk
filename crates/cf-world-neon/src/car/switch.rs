//! Changer de voiture à chaud : l'opérateur en demande une autre, la voiture posée est détruite et la suivante reposée
//! sur la même racine, avec le modèle déjà décodé (règle 8 : rien n'est chargé pendant un show).
//!
//! La racine (`Car`) garde sa position, son `CarState` (vitesse mesurée, clignotant, flashs déjà comptés par les
//! limiteurs : règle 7) et la traînée et la fumée, qui ne dépendent pas du modèle. Seuls les enfants marqués
//! [`CarPart`] et le [`CarRig`] sont remplacés.

use std::collections::HashMap;

use bevy::prelude::*;

use super::rig::{Car, CarPart, CarRig};
use super::spawn::{CarAssets, spawn_car};
use super::systems::{CarVariant, car_palette};
use cf_render::{CarSelection, ScenePalette};

/// La voiture que l'opérateur veut. Écrite par la fenêtre opérateur ; [`switch_car`] la met en œuvre.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct CarChoiceRequest(pub CarVariant);

/// La voiture posée en ce moment. Lue par la fenêtre opérateur pour marquer le bon bouton.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveCar(pub CarVariant);

/// Les voitures disponibles, c'est-à-dire décodées : un exemplaire intact de chacune.
#[derive(Resource, Debug, Default)]
pub struct CarModels {
    models: HashMap<CarVariant, super::model::CarModelAsset>,
}

impl CarModels {
    pub(super) fn store(&mut self, variant: CarVariant, model: super::model::CarModelAsset) {
        self.models.insert(variant, model);
    }

    /// Cette voiture est-elle décodée, donc prête à être posée ?
    #[must_use]
    pub fn has(&self, variant: CarVariant) -> bool {
        self.models.contains_key(&variant)
    }
}

/// `CfSet::Simulate`, avant [`switch_car`] : met `CarSelection` (lue par la fenêtre opérateur) à jour avec ce que le
/// monde sait, et traduit la demande de l'opérateur en [`CarChoiceRequest`].
pub(super) fn sync_selection(
    mut selection: ResMut<CarSelection>,
    active: Res<ActiveCar>,
    models: Res<CarModels>,
    mut request: ResMut<CarChoiceRequest>,
) {
    // L'opérateur a écrit la sélection (et nous seulement par `bypass_change_detection` ci-dessous) : sa demande passe.
    // Sans cela, une demande posée directement dans `CarChoiceRequest` serait écrasée par une sélection restée en arrière.
    if selection.is_changed()
        && let Some(wanted) = CarVariant::from_key(selection.requested)
        && wanted != request.0
    {
        request.0 = wanted;
    }
    let state = selection.bypass_change_detection();
    for option in &mut state.options {
        option.ready = CarVariant::from_key(option.key).is_some_and(|variant| models.has(variant));
    }
    state.active = active.0.key();
}

/// La voiture déjà posée : la racine `Car` qui porte un `CarRig`.
type WithPlacedCar = (With<Car>, With<CarRig>);

/// `CfSet::Simulate`, avant `drive_car` : si l'opérateur veut une autre voiture et qu'elle est prête, détruit l'ancienne
/// et pose la nouvelle. Une voiture pas encore décodée (ou refusée) laisse la demande en attente sans rien changer.
pub(super) fn switch_car(
    mut commands: Commands,
    request: Res<CarChoiceRequest>,
    mut active: ResMut<ActiveCar>,
    models: Res<CarModels>,
    scene: Res<ScenePalette>,
    mut assets: CarAssets,
    cars: Query<(Entity, &Children), WithPlacedCar>,
    parts: Query<(), With<CarPart>>,
) {
    if request.0 == active.0 {
        return;
    }
    let Some(model) = models.models.get(&request.0) else {
        return;
    };
    let Ok((root, children)) = cars.single() else {
        return;
    };
    for child in children.iter().filter(|child| parts.contains(*child)) {
        commands.entity(child).despawn();
    }
    let palette = car_palette(&scene);
    let rig = spawn_car(
        &mut commands,
        root,
        model.clone(),
        &mut assets,
        &palette,
        palette.violet,
    );
    commands.entity(root).insert(rig);
    active.0 = request.0;
}
