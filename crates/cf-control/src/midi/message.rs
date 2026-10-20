//! Messages MIDI et leur analyseur d'octets.
//!
//! [`MidiParser`] tourne dans le callback `midir` : il ne tient qu'un petit état (statut courant, deux octets de
//! données) et ne touche jamais au tas. Il suit le flux d'octets MIDI plutôt que de supposer « un message par
//! appel », parce que les hôtes diffèrent : certains remettent des messages complets, d'autres des paquets avec
//! plusieurs messages ou un statut courant (*running status*) sous-entendu.

/// Un message MIDI décodé. Canaux 0..=15 (le « canal 1 » des fiches techniques est 0 ici), données 0..=127.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidiMessage {
    /// Appui d'une note. `velocity` 1..=127 : un Note On de vélocité 0 est un relâchement, voir [`Self::NoteOff`].
    NoteOn { channel: u8, note: u8, velocity: u8 },
    /// Relâchement d'une note : vrai Note Off (vélocité de relâchement) ou Note On de vélocité 0 (alors `velocity` = 0).
    NoteOff { channel: u8, note: u8, velocity: u8 },
    /// Pression par note.
    PolyPressure { channel: u8, note: u8, pressure: u8 },
    /// Control Change, valeur 7 bits. Les paires 14 bits (MSB/LSB) ne sont pas recomposées ici : seul le profil sait
    /// quels CC forment une paire (voir [`RawEvent::MidiCc`](crate::RawEvent::MidiCc)).
    ControlChange { channel: u8, controller: u8, value: u8 },
    /// Changement de programme.
    ProgramChange { channel: u8, program: u8 },
    /// Pression de canal.
    ChannelPressure { channel: u8, pressure: u8 },
    /// Pitch bend, 0..=16383 (8192 au centre).
    PitchBend { channel: u8, value: u16 },
    /// Position dans le morceau, en doubles croches (1 double croche = 6 impulsions d'horloge). Sert à caler la mesure
    /// (J3-03).
    SongPosition { sixteenths: u16 },
    /// Sélection de morceau.
    SongSelect { song: u8 },
    /// Impulsion d'horloge (24 par noire).
    TimingClock,
    /// Départ de la séquence, depuis son début.
    Start,
    /// Reprise de la séquence là où elle s'était arrêtée.
    Continue,
    /// Arrêt de la séquence.
    Stop,
    /// Réinitialisation du matériel.
    SystemReset,
}

/// Analyseur d'octets MIDI avec statut courant.
///
/// Ce qui est ignoré sans bruit : les octets de données sans statut, les messages exclusifs système (`SysEx`) en entier,
/// le quart de trame MTC, l'accord d'accordage, les octets indéfinis et l'Active Sensing. Un statut qui arrive au
/// milieu d'un message l'abandonne. Les messages temps réel (0xF8 à 0xFF) peuvent s'intercaler n'importe où sans
/// perturber le message en cours ni le statut courant.
#[derive(Debug, Clone, Copy, Default)]
pub struct MidiParser {
    /// Statut courant (0x80..=0xF3), 0 si aucun. Les messages système communs l'annulent à la fin.
    status: u8,
    data: [u8; 2],
    received: u8,
    in_sysex: bool,
}

impl MidiParser {
    /// Un analyseur sans état.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            status: 0,
            data: [0; 2],
            received: 0,
            in_sysex: false,
        }
    }

    /// Oublie tout (à l'ouverture d'un port : un message coupé par un débranchement ne doit pas contaminer le suivant).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Analyse un bloc d'octets ; `emit` reçoit chaque message complet, dans l'ordre.
    pub fn parse(&mut self, bytes: &[u8], mut emit: impl FnMut(MidiMessage)) {
        for &byte in bytes {
            if let Some(message) = self.push(byte) {
                emit(message);
            }
        }
    }

    /// Intègre un octet ; rend le message qu'il complète, s'il y en a un.
    pub fn push(&mut self, byte: u8) -> Option<MidiMessage> {
        if byte >= 0xF8 {
            // Temps réel : n'interrompt rien.
            return realtime(byte);
        }
        if byte & 0x80 == 0 {
            return self.push_data(byte);
        }
        // Un statut abandonne le message en cours.
        self.received = 0;
        self.in_sysex = false;
        self.status = match byte {
            0xF0 => {
                self.in_sysex = true;
                0
            }
            // Fin de SysEx, accordage, octets indéfinis : annulent le statut courant, ne produisent rien.
            0xF4..=0xF7 => 0,
            _ => byte,
        };
        None
    }

    fn push_data(&mut self, byte: u8) -> Option<MidiMessage> {
        if self.in_sysex || self.status == 0 {
            return None;
        }
        self.data[usize::from(self.received)] = byte;
        self.received += 1;
        if self.received < data_len(self.status) {
            return None;
        }
        self.received = 0;
        let message = build(self.status, self.data);
        // Seuls les messages de canal gardent leur statut pour l'octet suivant.
        if self.status >= 0xF0 {
            self.status = 0;
        }
        message
    }
}

/// Nombre d'octets de données qui suivent ce statut (0x80..=0xF3).
fn data_len(status: u8) -> u8 {
    match status {
        0xC0..=0xDF | 0xF1 | 0xF3 => 1,
        _ => 2,
    }
}

fn build(status: u8, [d0, d1]: [u8; 2]) -> Option<MidiMessage> {
    let channel = status & 0x0F;
    let wide = u16::from(d0) | u16::from(d1) << 7;
    Some(match status & 0xF0 {
        0x80 => MidiMessage::NoteOff {
            channel,
            note: d0,
            velocity: d1,
        },
        // Note On de vélocité 0 = Note Off (de nombreux contrôleurs n'envoient que ça pour relâcher).
        0x90 if d1 == 0 => MidiMessage::NoteOff {
            channel,
            note: d0,
            velocity: 0,
        },
        0x90 => MidiMessage::NoteOn {
            channel,
            note: d0,
            velocity: d1,
        },
        0xA0 => MidiMessage::PolyPressure {
            channel,
            note: d0,
            pressure: d1,
        },
        0xB0 => MidiMessage::ControlChange {
            channel,
            controller: d0,
            value: d1,
        },
        0xC0 => MidiMessage::ProgramChange { channel, program: d0 },
        0xD0 => MidiMessage::ChannelPressure { channel, pressure: d0 },
        0xE0 => MidiMessage::PitchBend { channel, value: wide },
        _ => match status {
            0xF2 => MidiMessage::SongPosition { sixteenths: wide },
            0xF3 => MidiMessage::SongSelect { song: d0 },
            // 0xF1 : quart de trame MTC, ignoré.
            _ => return None,
        },
    })
}

fn realtime(byte: u8) -> Option<MidiMessage> {
    match byte {
        0xF8 => Some(MidiMessage::TimingClock),
        0xFA => Some(MidiMessage::Start),
        0xFB => Some(MidiMessage::Continue),
        0xFC => Some(MidiMessage::Stop),
        0xFF => Some(MidiMessage::SystemReset),
        // 0xF9 et 0xFD indéfinis, 0xFE Active Sensing (toutes les 300 ms, sans intérêt pour le show).
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(bytes: &[u8]) -> Vec<MidiMessage> {
        let mut parser = MidiParser::new();
        let mut out = Vec::new();
        parser.parse(bytes, |m| out.push(m));
        out
    }

    #[test]
    fn note_on_and_off_byte_by_byte() {
        assert_eq!(
            parse_all(&[0x90, 60, 100]),
            [MidiMessage::NoteOn {
                channel: 0,
                note: 60,
                velocity: 100
            }]
        );
        assert_eq!(
            parse_all(&[0x83, 61, 40]),
            [MidiMessage::NoteOff {
                channel: 3,
                note: 61,
                velocity: 40
            }]
        );
    }

    #[test]
    fn note_on_with_velocity_zero_is_a_note_off() {
        assert_eq!(
            parse_all(&[0x9F, 36, 0]),
            [MidiMessage::NoteOff {
                channel: 15,
                note: 36,
                velocity: 0
            }]
        );
    }

    #[test]
    fn running_status_repeats_the_last_channel_status() {
        // Un seul statut, trois notes, la dernière de vélocité 0.
        let got = parse_all(&[0x91, 60, 100, 62, 90, 60, 0]);
        assert_eq!(
            got,
            [
                MidiMessage::NoteOn {
                    channel: 1,
                    note: 60,
                    velocity: 100
                },
                MidiMessage::NoteOn {
                    channel: 1,
                    note: 62,
                    velocity: 90
                },
                MidiMessage::NoteOff {
                    channel: 1,
                    note: 60,
                    velocity: 0
                },
            ]
        );
    }

    #[test]
    fn running_status_survives_realtime_bytes_in_the_middle() {
        // Horloge au milieu d'un message, puis entre deux messages en statut courant.
        let got = parse_all(&[0xB0, 0xF8, 7, 0xF8, 64, 8, 0xF8, 65]);
        assert_eq!(
            got,
            [
                MidiMessage::TimingClock,
                MidiMessage::TimingClock,
                MidiMessage::ControlChange {
                    channel: 0,
                    controller: 7,
                    value: 64
                },
                MidiMessage::TimingClock,
                MidiMessage::ControlChange {
                    channel: 0,
                    controller: 8,
                    value: 65
                },
            ]
        );
    }

    #[test]
    fn control_change_program_change_and_pressure() {
        assert_eq!(
            parse_all(&[0xB2, 74, 127, 0xC2, 5, 0xD2, 33]),
            [
                MidiMessage::ControlChange {
                    channel: 2,
                    controller: 74,
                    value: 127
                },
                MidiMessage::ProgramChange { channel: 2, program: 5 },
                MidiMessage::ChannelPressure {
                    channel: 2,
                    pressure: 33
                },
            ]
        );
        // Program Change en statut courant : un octet de données suffit.
        assert_eq!(
            parse_all(&[0xC0, 1, 2, 3]),
            [
                MidiMessage::ProgramChange { channel: 0, program: 1 },
                MidiMessage::ProgramChange { channel: 0, program: 2 },
                MidiMessage::ProgramChange { channel: 0, program: 3 },
            ]
        );
    }

    #[test]
    fn pitch_bend_combines_lsb_then_msb() {
        assert_eq!(
            parse_all(&[0xE0, 0x00, 0x40]),
            [MidiMessage::PitchBend {
                channel: 0,
                value: 8192
            }]
        );
        assert_eq!(
            parse_all(&[0xE1, 0x7F, 0x7F]),
            [MidiMessage::PitchBend {
                channel: 1,
                value: 16383
            }]
        );
    }

    #[test]
    fn system_realtime_messages() {
        assert_eq!(
            parse_all(&[0xF8, 0xFA, 0xFB, 0xFC, 0xFF]),
            [
                MidiMessage::TimingClock,
                MidiMessage::Start,
                MidiMessage::Continue,
                MidiMessage::Stop,
                MidiMessage::SystemReset
            ]
        );
        // Active Sensing et octets indéfinis : rien.
        assert_eq!(parse_all(&[0xFE, 0xF9, 0xFD]), Vec::new());
    }

    #[test]
    fn song_position_and_select_cancel_running_status() {
        // SPP : LSB puis MSB. Les octets qui suivent n'ont plus de statut : ignorés.
        assert_eq!(
            parse_all(&[0xF2, 0x10, 0x01, 7, 8]),
            [MidiMessage::SongPosition { sixteenths: 0x90 }]
        );
        assert_eq!(parse_all(&[0xF3, 4]), [MidiMessage::SongSelect { song: 4 }]);
        // Après un message de canal, un message système commun coupe le statut courant.
        assert_eq!(
            parse_all(&[0x90, 60, 100, 0xF3, 2, 61, 100]),
            [
                MidiMessage::NoteOn {
                    channel: 0,
                    note: 60,
                    velocity: 100
                },
                MidiMessage::SongSelect { song: 2 }
            ]
        );
    }

    #[test]
    fn sysex_is_swallowed_whole_and_clears_running_status() {
        let got = parse_all(&[0x90, 60, 100, 0xF0, 0x7E, 0x7F, 0x06, 0x01, 0xF7, 61, 100, 0x90, 62, 90]);
        assert_eq!(
            got,
            [
                MidiMessage::NoteOn {
                    channel: 0,
                    note: 60,
                    velocity: 100
                },
                MidiMessage::NoteOn {
                    channel: 0,
                    note: 62,
                    velocity: 90
                }
            ]
        );
    }

    #[test]
    fn realtime_inside_sysex_does_not_end_it() {
        let got = parse_all(&[0xF0, 1, 2, 0xF8, 3, 0xF7]);
        assert_eq!(got, [MidiMessage::TimingClock]);
    }

    #[test]
    fn mtc_quarter_frame_is_ignored() {
        assert_eq!(parse_all(&[0xF1, 0x23, 0xF1, 0x34]), Vec::new());
    }

    #[test]
    fn data_without_status_is_ignored() {
        assert_eq!(parse_all(&[60, 100, 12]), Vec::new());
    }

    #[test]
    fn new_status_aborts_an_incomplete_message() {
        // Le Note On est coupé après sa note ; le CC suivant est intact.
        assert_eq!(
            parse_all(&[0x90, 60, 0xB0, 7, 99]),
            [MidiMessage::ControlChange {
                channel: 0,
                controller: 7,
                value: 99
            }]
        );
    }

    #[test]
    fn messages_split_across_calls_are_reassembled() {
        let mut parser = MidiParser::new();
        let mut out = Vec::new();
        for chunk in [&[0x90][..], &[60][..], &[100, 0xB0][..], &[1, 2][..]] {
            parser.parse(chunk, |m| out.push(m));
        }
        assert_eq!(out.len(), 2);
        parser.reset();
        parser.parse(&[10], |m| out.push(m));
        assert_eq!(out.len(), 2, "reset oublie le statut courant");
    }
}
