//! Le pas de lecture de l'application : un élément du flux MIDI → ce qu'il faut en faire.

use super::{Learner, MidiEvent, MidiItem, PortEvent};
use crate::mapper::{Mapper, TimedAction};
use crate::profile::Binding;

/// Ce qu'un élément du flux MIDI devient.
#[derive(Debug, Clone, PartialEq)]
pub enum Dispatch {
    /// Une action pour `cf-show`.
    Action(TimedAction),
    /// Le MIDI learn a reconnu un geste : le binding à ranger dans le profil. L'entrée n'a déclenché aucune action.
    Learned(Binding),
    /// Un contrôleur apparaît ou disparaît (le mapper en a déjà été informé), pour l'interface.
    Port(PortEvent),
    /// Un message qui n'est pas une commande de show (horloge, transport, position de morceau, pitch bend…) : à
    /// donner à [`MidiClock::handle`](crate::MidiClock::handle) et aux consommateurs de molettes.
    Other(MidiEvent),
    /// Une commande qu'aucun binding ne reconnaît, ou qui ne change rien.
    Ignored,
}

/// Traite un élément de [`MidiInput::try_recv`](super::MidiInput::try_recv) : tient le mapper au courant des ports,
/// alimente le MIDI learn s'il écoute, sinon traduit en action.
pub fn dispatch(item: MidiItem, mapper: &mut Mapper, learner: &mut Learner) -> Dispatch {
    match item {
        MidiItem::Port(event) => {
            match &event {
                PortEvent::Connected { port, name } => mapper.set_port_name(*port, Some(name)),
                PortEvent::Disconnected { port, .. } => mapper.set_port_name(*port, None),
            }
            Dispatch::Port(event)
        }
        MidiItem::Message(event) => {
            let Some(raw) = event.to_raw() else {
                return Dispatch::Other(event);
            };
            if learner.is_learning() {
                // Pendant l'écoute, un geste ne doit rien déclencher dans le show.
                return learner
                    .offer(&raw, mapper.port_name(event.port))
                    .map_or(Dispatch::Ignored, Dispatch::Learned);
            }
            mapper.map(&raw).map_or(Dispatch::Ignored, Dispatch::Action)
        }
    }
}
