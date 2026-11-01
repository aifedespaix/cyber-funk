//! Anime les motos du trafic à chaque image (X-10) : roues, plongée au freinage, pilote. La logique est dans
//! `rider::anim` ; ce système ne fait que la nourrir de la flotte et écrire les transformations des nœuds du modèle.

use bevy::prelude::*;

use super::TrafficEntities;
use crate::rider::anim::{MotoRig, Reaction, world_radius};
use crate::rider::moto::fit_scale;
use crate::traffic::director::TrafficDirector;
use crate::traffic::variant::BIKE;

/// `TrafficSet::Sync`, après la recopie des poses : fait tourner les roues des motos vivantes, fait plonger leur modèle et
/// bouger leur pilote. Rien n'est alloué : les entités et l'état sont préparés au démarrage.
pub(crate) fn animate_motos(
    time: Res<Time>,
    director: Res<TrafficDirector>,
    entities: Res<TrafficEntities>,
    mut rigs: Query<&mut MotoRig>,
    mut nodes: Query<&mut Transform, Without<MotoRig>>,
) {
    let dt = time.delta_secs();
    let fit = fit_scale();
    for craft in director.fleet.crafts() {
        if !craft.active || craft.variant != BIKE {
            continue;
        }
        let Some(body) = entities.body(BIKE, craft.slot) else {
            continue;
        };
        let Ok(mut rig) = rigs.get_mut(body) else {
            continue;
        };
        let speed = craft.velocity.length();
        if rig.motion.generation != craft.generation {
            rig.motion.reset(craft.generation, speed, craft.flash);
        }
        let radii = [
            world_radius(rig.which, true, fit, craft.scale),
            world_radius(rig.which, false, fit, craft.scale),
        ];
        let reaction = rig.motion.step(dt, speed, craft.pitch, radii);
        if let Some(front) = rig.nodes.front
            && let Ok(mut transform) = nodes.get_mut(front)
        {
            transform.rotation = Reaction::wheel(reaction.front);
        }
        if let Some(rear) = rig.nodes.rear
            && let Ok(mut transform) = nodes.get_mut(rear)
        {
            transform.rotation = Reaction::wheel(reaction.rear);
        }
        if let Some((rider, rest)) = rig.nodes.rider
            && let Ok(mut transform) = nodes.get_mut(rider)
        {
            *transform = reaction.rider(rest);
        }
        if let Ok(mut transform) = nodes.get_mut(rig.model) {
            *transform = reaction.model(rig.fit);
        }
    }
}
