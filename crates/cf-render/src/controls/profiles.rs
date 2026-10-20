//! Les profils de contrôle suivent le créneau : à `SlotLoaded`, la pile est rechargée.

use bevy::prelude::*;
use cf_control::{ControlProfile, TimedAction};
use cf_show::director::ShowEvent;

use super::bridge::{ControlBridge, Emitter};
use super::{ControlAction, ControlsStatus};
use crate::show::{ShowControls, ShowEventMessage};

/// Quand le chef du spectacle charge un créneau :
/// - la table de contrôle devient « clavier par défaut + profils de l'événement + profils du créneau » (le dernier
///   gagne). `Mapper::reload` relâche d'abord tout ce qui est tenu (un Blackout tenu pendant la passation s'éteint) ;
///   ces relâchements repartent en [`ControlAction`] et le chef du spectacle les lit à l'image suivante ;
/// - la source de tempo préférée du créneau est donnée à l'arbitre ;
/// - l'horloge MIDI est oubliée : un autre DJ, un autre logiciel, une autre horloge (elle se recale en un temps).
///
/// Un profil désigné mais absent de la bibliothèque, ou invalide, est signalé dans le panneau ; la table en place
/// reste alors intacte (le show ne doit jamais perdre ses contrôles à cause d'un fichier).
pub(crate) fn apply_slot_profiles(
    mut bridge: ResMut<ControlBridge>,
    mut status: ResMut<ControlsStatus>,
    controls: Res<ShowControls>,
    mut facts: MessageReader<ShowEventMessage>,
    mut out: MessageWriter<ControlAction>,
) {
    for fact in facts.read() {
        let ShowEvent::SlotLoaded { slot, .. } = &fact.0 else {
            continue;
        };
        let slot = *slot;
        let now = bridge.now_s();
        let mut emitter = Emitter {
            bridge: &mut bridge,
            status: &mut status,
            out: &mut out,
        };
        let mut released = Vec::new();
        let reloaded = reload(&mut emitter, &controls, slot, now, &mut released);
        for action in released {
            emitter.emit(action);
        }
        if let Err(note) = reloaded {
            tracing::warn!(%note, slot, "contrôles : profils du créneau refusés, table précédente conservée");
            if !emitter.status.notes.contains(&note) {
                emitter.status.notes.push(note);
            }
        }
        let preferred = controls.tempo.get(slot).copied().flatten();
        emitter.bridge.arbiter.set_preferred(preferred);
        emitter.bridge.clock.reset();
    }
}

/// Recharge la table du créneau `slot` ; rend la raison en clair si un profil manque ou est refusé.
fn reload(
    emitter: &mut Emitter<'_, '_>,
    controls: &ShowControls,
    slot: usize,
    now_s: f64,
    released: &mut Vec<TimedAction>,
) -> Result<(), String> {
    let bridge = &mut *emitter.bridge;
    let mut stack: Vec<&ControlProfile> = vec![&bridge.keyboard];
    let wanted = controls
        .event
        .iter()
        .chain(controls.slots.get(slot).into_iter().flatten());
    for id in wanted {
        let profile = bridge
            .library
            .get(id.as_str())
            .ok_or_else(|| format!("profil de contrôle « {id} » introuvable dans assets/controllers"))?;
        stack.push(profile);
    }
    let overrides = bridge
        .mapper
        .reload(&stack, now_s, released)
        .map_err(|error| error.to_string())?;
    tracing::debug!(slot, profils = stack.len(), remplacements = overrides.len(), "contrôles : table rechargée");
    Ok(())
}
