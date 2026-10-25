//! Ce que les enseignes publient aux autres : les structures que le pilote doit esquiver.

use bevy::prelude::*;

use super::systems::SignObstructions;

/// `CfSet::Simulate`, après la publication du trafic et avant le pilote : ajoute les portiques, potences et panneaux
/// suspendus à ce que le pilote doit esquiver (`PilotTraffic::obstructions`). Les structures sont celles de l'image
/// précédente : un retard d'une image sans effet, elles ne bougent pas.
pub fn publish_sign_obstructions(signs: Res<SignObstructions>, mut traffic: ResMut<crate::pilot::PilotTraffic>) {
    traffic.obstructions.extend_from_slice(&signs.0);
}
