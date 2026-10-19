//! Les sources d'un binding : quelle touche, quelle note, quel potard, quelle adresse OSC.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::keyboard::{Key, KeyEvent, ShiftRule};
use crate::raw::{PortId, RawEvent};

/// Plus haut canal MIDI (canaux 0..=15).
pub const MIDI_CHANNEL_MAX: u8 = 15;
/// Plus haute note ou plus haut numéro de CC MIDI (données 7 bits).
pub const MIDI_DATA_MAX: u8 = 127;

/// Comment un encodeur sans fin code son décalage dans la valeur 7 bits d'un CC. Chaque fabricant a son habitude ;
/// le profil du contrôleur le précise. Sans effet hors du mode [`Mode::Relative`](crate::Mode::Relative).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum RelativeEncoding {
    /// 64 = immobile, 65 = +1, 63 = −1 (« Relative 1 », le plus répandu sur les contrôleurs DJ).
    #[default]
    BinaryOffset,
    /// 1 = +1, 127 = −1 (complément à deux sur 7 bits).
    TwosComplement,
    /// 1 = +1, 65 = −1 (bit 6 = signe, bits 0 à 5 = amplitude).
    SignMagnitude,
}

impl RelativeEncoding {
    /// Le décalage signé, en crans, codé par cette valeur de CC.
    #[must_use]
    pub fn decode(self, value: u8) -> i8 {
        // Masqué à 7 bits : 0..=127 tient dans un i8 sans débordement.
        let v = (value & 0x7F).cast_signed();
        match self {
            Self::BinaryOffset => v - 64,
            Self::TwosComplement => {
                if v < 64 {
                    v
                } else {
                    // 64..=127 → −64..=−1, en deux temps pour rester dans les bornes d'un i8.
                    (v - 64) - 64
                }
            }
            Self::SignMagnitude => {
                let magnitude = v & 0x3F;
                if v & 0x40 == 0 { magnitude } else { -magnitude }
            }
        }
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // signature imposée par `skip_serializing_if`
    pub(crate) fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// D'où vient un binding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputSource {
    /// Une touche du clavier, avec ses touches de modification. Ctrl, Alt et Méta doivent correspondre exactement :
    /// un binding sans Ctrl ne réagit pas à Ctrl+touche (Sillage ignorait toute touche accompagnée de Ctrl, Alt ou
    /// Méta, qui appartiennent au système).
    Key {
        key: Key,
        #[serde(default, skip_serializing_if = "ShiftRule::is_any")]
        shift: ShiftRule,
        #[serde(default, skip_serializing_if = "is_false")]
        ctrl: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        alt: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        meta: bool,
    },
    /// Une note MIDI (pad, bouton). `port` : nom exact du port, ou `None` pour accepter tous les ports.
    MidiNote {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<String>,
        channel: u8,
        note: u8,
    },
    /// Un Control Change MIDI (potard, fader, encodeur, bouton en CC).
    MidiCc {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        port: Option<String>,
        channel: u8,
        cc: u8,
        /// Codage des décalages, si la commande est un encodeur sans fin.
        #[serde(default, skip_serializing_if = "RelativeEncoding::is_default")]
        encoding: RelativeEncoding,
    },
    /// Une adresse OSC exacte (`/cf/go`).
    Osc { address: String },
}

#[allow(clippy::trivially_copy_pass_by_ref)] // signature imposée par `skip_serializing_if`
fn is_false(b: &bool) -> bool {
    !*b
}

impl InputSource {
    /// Une touche sans modificateur, Maj indifférente.
    #[must_use]
    pub fn key(key: Key) -> Self {
        Self::Key {
            key,
            shift: ShiftRule::Any,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// Une touche sans Ctrl/Alt/Méta, avec cette exigence sur Maj.
    #[must_use]
    pub fn key_with_shift(key: Key, shift: ShiftRule) -> Self {
        Self::Key {
            key,
            shift,
            ctrl: false,
            alt: false,
            meta: false,
        }
    }

    /// La source vient-elle d'un bouton (appui/relâchement) plutôt que d'une commande continue ? Les modes
    /// continus ([`Mode::Relative`](crate::Mode::Relative)) exigent une commande continue.
    #[must_use]
    pub fn is_button(&self) -> bool {
        matches!(self, Self::Key { .. } | Self::MidiNote { .. })
    }

    /// Un même geste peut-il déclencher les deux sources ? C'est la définition d'un conflit de binding.
    ///
    /// Limite connue : un binding par caractère et un binding par code peuvent viser la même touche physique
    /// (`Char('a')` et `Code(KeyQ)` sur un AZERTY) ; sans la disposition, impossible de le savoir, ils ne sont donc
    /// pas signalés.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Key {
                    key: k1,
                    shift: s1,
                    ctrl: c1,
                    alt: a1,
                    meta: m1,
                },
                Self::Key {
                    key: k2,
                    shift: s2,
                    ctrl: c2,
                    alt: a2,
                    meta: m2,
                },
            ) => k1.same_as(*k2) && s1.overlaps(*s2) && c1 == c2 && a1 == a2 && m1 == m2,
            (
                Self::MidiNote {
                    port: p1,
                    channel: ch1,
                    note: n1,
                },
                Self::MidiNote {
                    port: p2,
                    channel: ch2,
                    note: n2,
                },
            ) => ch1 == ch2 && n1 == n2 && ports_overlap(p1.as_deref(), p2.as_deref()),
            (
                Self::MidiCc {
                    port: p1,
                    channel: ch1,
                    cc: c1,
                    ..
                },
                Self::MidiCc {
                    port: p2,
                    channel: ch2,
                    cc: c2,
                    ..
                },
            ) => ch1 == ch2 && c1 == c2 && ports_overlap(p1.as_deref(), p2.as_deref()),
            (Self::Osc { address: a1 }, Self::Osc { address: a2 }) => a1 == a2,
            _ => false,
        }
    }

    /// L'entrée brute vient-elle de cette source ? `port_name` donne le nom d'un port MIDI ouvert.
    pub(crate) fn matches<'a>(&self, event: &RawEvent, port_name: impl Fn(PortId) -> Option<&'a str>) -> bool {
        match (self, event) {
            (
                Self::Key {
                    key,
                    shift,
                    ctrl,
                    alt,
                    meta,
                },
                RawEvent::Key(e),
            ) => key_matches(*key, *shift, [*ctrl, *alt, *meta], e),
            (
                Self::MidiNote { port, channel, note },
                RawEvent::MidiNote {
                    port: id,
                    channel: ch,
                    note: n,
                    ..
                },
            ) => channel == ch && note == n && port_matches(port.as_deref(), port_name(*id)),
            (
                Self::MidiCc { port, channel, cc, .. },
                RawEvent::MidiCc {
                    port: id,
                    channel: ch,
                    cc: c,
                    ..
                },
            ) => channel == ch && cc == c && port_matches(port.as_deref(), port_name(*id)),
            (Self::Osc { address }, RawEvent::Osc { address: a, .. }) => address == a,
            _ => false,
        }
    }
}

fn key_matches(key: Key, shift: ShiftRule, [ctrl, alt, meta]: [bool; 3], e: &KeyEvent) -> bool {
    key.matches(e) && shift.accepts(e.mods.shift) && e.mods.ctrl == ctrl && e.mods.alt == alt && e.mods.meta == meta
}

/// Deux exigences de port se recouvrent si l'une accepte tous les ports ou si elles nomment le même.
fn ports_overlap(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

/// Un binding sans port accepte tout ; un binding nommé exige un port ouvert de ce nom.
fn port_matches(wanted: Option<&str>, actual: Option<&str>) -> bool {
    wanted.is_none_or(|w| actual == Some(w))
}

impl fmt::Display for InputSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key {
                key,
                shift,
                ctrl,
                alt,
                meta,
            } => {
                for (on, name) in [
                    (*ctrl, "Ctrl+"),
                    (*alt, "Alt+"),
                    (*meta, "Méta+"),
                    (*shift == ShiftRule::Held, "Maj+"),
                ] {
                    if on {
                        f.write_str(name)?;
                    }
                }
                write!(f, "{key}")?;
                if *shift == ShiftRule::Released {
                    f.write_str(" (sans Maj)")?;
                }
                Ok(())
            }
            // Canaux affichés de 1 à 16, comme sur les fiches techniques des contrôleurs.
            Self::MidiNote { port, channel, note } => {
                write!(f, "note {note} canal {}", u16::from(*channel) + 1)?;
                write_port(f, port.as_deref())
            }
            Self::MidiCc { port, channel, cc, .. } => {
                write!(f, "CC {cc} canal {}", u16::from(*channel) + 1)?;
                write_port(f, port.as_deref())
            }
            Self::Osc { address } => write!(f, "OSC {address}"),
        }
    }
}

fn write_port(f: &mut fmt::Formatter<'_>, port: Option<&str>) -> fmt::Result {
    match port {
        Some(p) => write!(f, " ({p})"),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_encodings_decode_both_directions() {
        assert_eq!(RelativeEncoding::BinaryOffset.decode(65), 1);
        assert_eq!(RelativeEncoding::BinaryOffset.decode(60), -4);
        assert_eq!(RelativeEncoding::TwosComplement.decode(1), 1);
        assert_eq!(RelativeEncoding::TwosComplement.decode(127), -1);
        assert_eq!(RelativeEncoding::TwosComplement.decode(64), -64);
        assert_eq!(RelativeEncoding::SignMagnitude.decode(3), 3);
        assert_eq!(RelativeEncoding::SignMagnitude.decode(65), -1);
    }

    #[test]
    fn midi_sources_overlap_when_a_port_is_unnamed() {
        let any = InputSource::MidiNote {
            port: None,
            channel: 0,
            note: 36,
        };
        let named = InputSource::MidiNote {
            port: Some("X1".into()),
            channel: 0,
            note: 36,
        };
        let other = InputSource::MidiNote {
            port: Some("FLX4".into()),
            channel: 0,
            note: 36,
        };
        assert!(any.overlaps(&named));
        assert!(!named.overlaps(&other));
        assert!(!named.overlaps(&InputSource::MidiNote {
            port: Some("X1".into()),
            channel: 1,
            note: 36
        }));
    }

    #[test]
    fn display_is_readable() {
        let prev_dj = InputSource::key_with_shift(Key::Char('g'), ShiftRule::Held);
        assert_eq!(prev_dj.to_string(), "Maj+G");
        let pad = InputSource::MidiNote {
            port: Some("X1".into()),
            channel: 0,
            note: 36,
        };
        assert_eq!(pad.to_string(), "note 36 canal 1 (X1)");
    }
}
