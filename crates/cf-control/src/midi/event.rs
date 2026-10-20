//! L'événement qui traverse le canal borné entre le callback MIDI et le reste de l'application.

use super::MidiMessage;
use crate::raw::{PortId, RawEvent, RawInput};

/// Un message MIDI reçu, horodaté et attribué à un port. `Copy`, sans tas : c'est ce que le callback pousse dans le
/// canal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidiEvent {
    /// Instant de réception, en secondes depuis l'`epoch` de la [`MidiConfig`](super::MidiConfig) (l'horloge monotone
    /// commune de l'application), pris dans le callback et non à la lecture du canal.
    pub time_s: f64,
    /// Le port d'où vient le message, tel que le déclare [`PortEvent::Connected`](super::PortEvent::Connected).
    pub port: PortId,
    pub message: MidiMessage,
}

impl MidiEvent {
    /// L'entrée brute à donner au [`Mapper`](crate::Mapper), pour les messages qu'un binding peut viser (notes, CC,
    /// Program Change). `None` pour le reste (horloge, pitch bend…), qui n'est pas une commande de show : l'horloge est
    /// pour J3-03.
    #[must_use]
    pub fn to_raw(&self) -> Option<RawInput> {
        let port = self.port;
        let event = match self.message {
            MidiMessage::NoteOn {
                channel,
                note,
                velocity,
            } => RawEvent::MidiNote {
                port,
                channel,
                note,
                velocity,
            },
            // Le relâchement est une vélocité 0 pour le mapper, quelle que soit la vélocité de relâchement envoyée.
            MidiMessage::NoteOff { channel, note, .. } => RawEvent::MidiNote {
                port,
                channel,
                note,
                velocity: 0,
            },
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => RawEvent::MidiCc {
                port,
                channel,
                cc: controller,
                value,
            },
            MidiMessage::ProgramChange { channel, program } => RawEvent::MidiProgram { port, channel, program },
            _ => return None,
        };
        Some(RawInput {
            time_s: self.time_s,
            event,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(message: MidiMessage) -> MidiEvent {
        MidiEvent {
            time_s: 1.5,
            port: PortId(3),
            message,
        }
    }

    #[test]
    fn commands_become_raw_inputs_with_their_timestamp() {
        let raw = event(MidiMessage::NoteOff {
            channel: 2,
            note: 40,
            velocity: 64,
        })
        .to_raw();
        assert_eq!(
            raw,
            Some(RawInput {
                time_s: 1.5,
                event: RawEvent::MidiNote {
                    port: PortId(3),
                    channel: 2,
                    note: 40,
                    velocity: 0
                }
            })
        );
        assert!(matches!(
            event(MidiMessage::ProgramChange { channel: 0, program: 9 })
                .to_raw()
                .map(|r| r.event),
            Some(RawEvent::MidiProgram { program: 9, .. })
        ));
    }

    #[test]
    fn non_commands_have_no_raw_form() {
        assert!(event(MidiMessage::TimingClock).to_raw().is_none());
        assert!(
            event(MidiMessage::PitchBend {
                channel: 0,
                value: 8192
            })
            .to_raw()
            .is_none()
        );
    }
}
