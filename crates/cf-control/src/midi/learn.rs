//! MIDI learn : l'opérateur choisit une action dans l'interface, bouge un potard ou frappe un pad, et le geste devient
//! le binding.
//!
//! Le [`Learner`] ne touche ni au mapper ni au profil : il rend un [`Binding`] que l'appelant range dans le profil
//! ([`ControlProfile::set_binding`](crate::ControlProfile::set_binding), qui dit ce qu'il remplace) puis recharge dans le
//! mapper. Tant qu'il écoute, les messages ne doivent pas non plus déclencher d'action (voir [`dispatch`](super::dispatch)).
//!
//! Ce que l'aide fait à la place de l'opérateur :
//! - un relâchement (Note Off, vélocité 0) n'est pas un geste : on attend l'appui ;
//! - le mode est choisi d'après l'action et la source (bascule : une bascule ; grandeur sur un pad : maintien ; etc.) ;
//! - un potard haute résolution est reconnu : si le CC `n` (0 à 31) et le CC `n + 32` arrivent coup sur coup sur le même
//!   canal, le binding est un CC 14 bits ;
//! - une source qui ne peut pas porter l'action (grandeur sur un Program Change…) est ignorée, on continue d'écouter ;
//! - le nom du port est enregistré débarrassé des numéros de l'hôte, pour retrouver le contrôleur après rebranchement.
//!
//! Ce qu'elle ne devine pas : le codage d'un encodeur sans fin (un cran = +1 ou −1 selon la marque), parce qu'une seule
//! valeur ne permet pas de trancher. Pour un encodeur, [`Learner::start_learn_as`] demande le mode et le codage.

use super::canonical_port_name;
use crate::profile::{Binding, ControlProfile};
use crate::raw::{RawEvent, RawInput};
use crate::source::{CC14_LSB_OFFSET, CC14_MSB_MAX, InputSource, RelativeEncoding};
use crate::template::{ActionKind, ActionTemplate, Mode};

/// Fenêtre pendant laquelle un CC est gardé en attente de sa moitié 14 bits, en secondes. Les deux moitiés d'un même
/// geste partent à moins d'une milliseconde l'une de l'autre ; 50 ms laisse de la marge sans que l'opérateur sente un
/// délai.
const PAIR_WINDOW_S: f64 = 0.05;

/// Le premier CC reçu, retenu le temps de savoir s'il a un compagnon 14 bits.
#[derive(Debug, Clone, PartialEq)]
struct PendingCc {
    time_s: f64,
    port: Option<String>,
    channel: u8,
    cc: u8,
}

#[derive(Debug, Clone, PartialEq)]
struct Target {
    template: ActionTemplate,
    /// Mode imposé par l'opérateur ; `None` : choisi d'après l'action et la source.
    mode: Option<Mode>,
    encoding: RelativeEncoding,
    pending: Option<PendingCc>,
}

/// Le mode écoute du MIDI learn.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Learner {
    target: Option<Target>,
}

impl Learner {
    /// Un apprenant au repos.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Passe en écoute : le prochain geste reconnu devient la source d'un binding vers `template`, dans le mode le
    /// plus naturel pour cette action. Remplace une écoute déjà en cours.
    pub fn start_learn(&mut self, template: ActionTemplate) {
        self.target = Some(Target {
            template,
            mode: None,
            encoding: RelativeEncoding::default(),
            pending: None,
        });
    }

    /// Comme [`Learner::start_learn`], avec le mode et, pour un encodeur, le codage imposés par l'opérateur.
    pub fn start_learn_as(&mut self, template: ActionTemplate, mode: Mode, encoding: RelativeEncoding) {
        self.target = Some(Target {
            template,
            mode: Some(mode),
            encoding,
            pending: None,
        });
    }

    /// Abandonne l'écoute sans rien apprendre.
    pub fn cancel(&mut self) {
        self.target = None;
    }

    /// Écoute-t-on ?
    #[must_use]
    pub fn is_learning(&self) -> bool {
        self.target.is_some()
    }

    /// L'action en cours d'apprentissage, pour l'afficher (« tournez le potard pour Master… »).
    #[must_use]
    pub fn target(&self) -> Option<&ActionTemplate> {
        self.target.as_ref().map(|t| &t.template)
    }

    /// Propose une entrée à l'apprenant. `port_name` : nom du port d'où elle vient
    /// ([`Mapper::port_name`](crate::Mapper::port_name)). Rend le binding appris, et l'écoute s'arrête ; `None` si
    /// l'entrée ne convient pas ou si elle attend encore sa moitié 14 bits (voir [`Learner::poll`]).
    pub fn offer(&mut self, input: &RawInput, port_name: Option<&str>) -> Option<Binding> {
        let target = self.target.as_mut()?;
        let port = port_name.map(|name| canonical_port_name(name).to_owned());
        let source = match input.event {
            RawEvent::MidiNote {
                channel,
                note,
                velocity,
                ..
            } if velocity > 0 => InputSource::MidiNote { port, channel, note },
            RawEvent::MidiProgram { channel, program, .. } => InputSource::MidiProgram { port, channel, program },
            RawEvent::MidiCc { channel, cc, .. } => target.offer_cc(input.time_s, port, channel, cc)?,
            _ => return None,
        };
        self.finish(source)
    }

    /// À appeler régulièrement (chaque image) : conclut un CC resté sans compagnon 14 bits après la fenêtre d'attente,
    /// comme CC 7 bits. `now_s` : même horloge que [`RawInput::time_s`].
    pub fn poll(&mut self, now_s: f64) -> Option<Binding> {
        let target = self.target.as_mut()?;
        let pending = target.pending.as_ref()?;
        if now_s - pending.time_s < PAIR_WINDOW_S {
            return None;
        }
        let pending = target.pending.take()?;
        let source = InputSource::MidiCc {
            port: pending.port,
            channel: pending.channel,
            cc: pending.cc,
            encoding: target.encoding,
        };
        self.finish(source)
    }

    /// Fabrique le binding si la source peut porter l'action ; sinon on continue d'écouter.
    fn finish(&mut self, source: InputSource) -> Option<Binding> {
        let target = self.target.as_ref()?;
        let binding = Binding {
            mode: target.mode_for(&source),
            action: target.template.clone(),
            source,
        };
        let valid = ControlProfile {
            name: String::new(),
            bindings: vec![binding.clone()],
        }
        .issues()
        .is_empty();
        if !valid {
            return None;
        }
        self.target = None;
        Some(binding)
    }
}

impl Target {
    /// Un CC arrive. Pour une grandeur en position absolue, on le retient une courte fenêtre : si son compagnon 14 bits
    /// suit, c'est un potard haute résolution. Dans les autres cas, le CC est la source tout de suite.
    fn offer_cc(&mut self, time_s: f64, port: Option<String>, channel: u8, cc: u8) -> Option<InputSource> {
        let may_be_wide = self.template.kind() == ActionKind::Level
            && self.mode.is_none_or(|m| m == Mode::Absolute)
            && cc <= CC14_MSB_MAX + CC14_LSB_OFFSET;
        if !may_be_wide {
            return Some(InputSource::MidiCc {
                port,
                channel,
                cc,
                encoding: self.encoding,
            });
        }
        match &self.pending {
            None => {
                self.pending = Some(PendingCc {
                    time_s,
                    port,
                    channel,
                    cc,
                });
                None
            }
            Some(first) if first.port == port && first.channel == channel && is_wide_pair(first.cc, cc) => {
                let msb = first.cc.min(cc);
                self.pending = None;
                Some(InputSource::MidiCc14 { port, channel, cc: msb })
            }
            // Autre commande, ou le même CC qui répète sa valeur : la première reste la candidate.
            Some(_) => None,
        }
    }

    fn mode_for(&self, source: &InputSource) -> Mode {
        if let Some(mode) = self.mode {
            return mode;
        }
        let kind = self.template.kind();
        if kind == ActionKind::Level && source.is_button() {
            // Une grandeur sur un pad : maintenu = 1, relâché = 0 (une position n'aurait pas de sens).
            Mode::Momentary
        } else {
            kind.default_mode()
        }
    }
}

/// Les CC `a` et `b` sont-ils les deux moitiés d'un même CC 14 bits, dans un ordre ou dans l'autre ?
fn is_wide_pair(a: u8, b: u8) -> bool {
    let (low, high) = (a.min(b), a.max(b));
    low <= CC14_MSB_MAX && high == low + CC14_LSB_OFFSET
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{Layer, MacroIndex};
    use crate::raw::PortId;

    fn cc(time_s: f64, channel: u8, cc: u8, value: u8) -> RawInput {
        RawInput {
            time_s,
            event: RawEvent::MidiCc {
                port: PortId(0),
                channel,
                cc,
                value,
            },
        }
    }

    fn note(channel: u8, note: u8, velocity: u8) -> RawInput {
        RawInput {
            time_s: 0.0,
            event: RawEvent::MidiNote {
                port: PortId(0),
                channel,
                note,
                velocity,
            },
        }
    }

    const PORT: &str = "DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0";

    #[test]
    fn idle_learner_ignores_everything() {
        let mut learner = Learner::new();
        assert!(learner.offer(&note(0, 36, 100), Some(PORT)).is_none());
        assert!(learner.poll(10.0).is_none());
    }

    #[test]
    fn next_note_press_becomes_the_binding_and_release_is_skipped() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Go);
        assert!(learner.is_learning());
        assert_eq!(learner.target(), Some(&ActionTemplate::Go));
        assert!(learner.offer(&note(0, 36, 0), Some(PORT)).is_none(), "relâchement ignoré");
        let learned = learner.offer(&note(9, 37, 90), Some(PORT)).unwrap();
        assert_eq!(
            learned,
            Binding {
                source: InputSource::MidiNote {
                    port: Some("DDJ-FLX4:DDJ-FLX4 MIDI 1".into()),
                    channel: 9,
                    note: 37
                },
                action: ActionTemplate::Go,
                mode: Mode::Momentary,
            }
        );
        assert!(!learner.is_learning());
    }

    #[test]
    fn unnamed_port_gives_a_binding_for_any_port() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Blackout);
        let learned = learner.offer(&note(0, 40, 127), None).unwrap();
        assert!(matches!(learned.source, InputSource::MidiNote { port: None, .. }));
        assert_eq!(learned.mode, Mode::Toggle, "une bascule par défaut");
    }

    #[test]
    fn level_on_a_pad_is_a_hold_not_a_position() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Master);
        assert_eq!(learner.offer(&note(0, 40, 127), None).unwrap().mode, Mode::Momentary);
    }

    #[test]
    fn lone_cc_on_a_level_is_absolute_seven_bit_after_the_window() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Macro(MacroIndex(2)));
        assert!(learner.offer(&cc(1.00, 0, 20, 20), Some(PORT)).is_none(), "attend un compagnon éventuel");
        assert!(learner.poll(1.01).is_none(), "fenêtre pas finie");
        let learned = learner.poll(1.06).unwrap();
        assert!(matches!(
            learned.source,
            InputSource::MidiCc {
                channel: 0,
                cc: 20,
                encoding: RelativeEncoding::BinaryOffset,
                ..
            }
        ));
        assert_eq!(learned.mode, Mode::Absolute);
    }

    #[test]
    fn msb_then_lsb_is_recognised_as_a_14_bit_cc() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::LayerFade(Layer::World));
        assert!(learner.offer(&cc(2.000, 1, 5, 64), Some(PORT)).is_none());
        let learned = learner.offer(&cc(2.001, 1, 37, 3), Some(PORT)).unwrap();
        assert!(matches!(learned.source, InputSource::MidiCc14 { channel: 1, cc: 5, .. }));
        assert_eq!(learned.mode, Mode::Absolute);
    }

    #[test]
    fn lsb_then_msb_is_recognised_too() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Master);
        assert!(learner.offer(&cc(2.000, 0, 33, 3), None).is_none());
        let learned = learner.offer(&cc(2.001, 0, 1, 64), None).unwrap();
        assert!(matches!(learned.source, InputSource::MidiCc14 { cc: 1, .. }));
    }

    #[test]
    fn cc_above_the_wide_range_is_immediate() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Master);
        assert!(matches!(
            learner.offer(&cc(0.0, 0, 90, 1), None).unwrap().source,
            InputSource::MidiCc { cc: 90, .. }
        ));
    }

    #[test]
    fn explicit_relative_mode_keeps_the_chosen_encoding() {
        let mut learner = Learner::new();
        learner.start_learn_as(ActionTemplate::Master, Mode::Relative, RelativeEncoding::TwosComplement);
        let learned = learner.offer(&cc(0.0, 0, 20, 1), None).unwrap();
        assert_eq!(learned.mode, Mode::Relative);
        assert!(matches!(
            learned.source,
            InputSource::MidiCc {
                encoding: RelativeEncoding::TwosComplement,
                ..
            }
        ));
    }

    #[test]
    fn a_cc_button_for_an_impulse_is_immediate() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::ForceDrop);
        assert!(learner.offer(&cc(0.0, 0, 20, 127), None).is_some());
    }

    #[test]
    fn program_change_cannot_carry_a_level_so_learning_continues() {
        let program = RawInput {
            time_s: 0.0,
            event: RawEvent::MidiProgram {
                port: PortId(0),
                channel: 0,
                program: 4,
            },
        };
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Master);
        assert!(learner.offer(&program, None).is_none());
        assert!(learner.is_learning());
        learner.start_learn(ActionTemplate::NextDj);
        let learned = learner.offer(&program, None).unwrap();
        assert!(matches!(learned.source, InputSource::MidiProgram { program: 4, .. }));
    }

    #[test]
    fn cancel_and_restart() {
        let mut learner = Learner::new();
        learner.start_learn(ActionTemplate::Go);
        learner.cancel();
        assert!(learner.offer(&note(0, 1, 1), None).is_none());
        learner.start_learn(ActionTemplate::Back);
        learner.start_learn(ActionTemplate::Go);
        assert_eq!(learner.target(), Some(&ActionTemplate::Go));
    }
}
