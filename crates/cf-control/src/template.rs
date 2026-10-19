//! Ce qu'un binding produit : une [`Action`] « à trous », dont la valeur (état de bascule, position, décalage) vient
//! de l'entrée, et le [`Mode`] qui dit comment la lire.

use serde::{Deserialize, Serialize};

use crate::action::{Action, ActionFamily, CueIndex, DjIndex, Layer, Level, MacroIndex, Switch, TitleId, TriggerId};

/// Décalage d'une grandeur continue par cran d'encodeur en mode [`Mode::Relative`], en fraction de la course :
/// 128 crans pour la course entière, la résolution d'un potard 7 bits.
pub const RELATIVE_LEVEL_STEP: f32 = 1.0 / 128.0;

/// Comment lire l'entrée.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mode {
    /// Bouton momentané : l'appui agit ; pour une bascule, appui = `On` et relâchement = `Off` (on tient le pad pour
    /// garder le noir) ; pour une grandeur, appui = 1 et relâchement = 0.
    Momentary,
    /// Bouton bascule : chaque appui inverse la bascule (`Switch::Toggle`), le relâchement ne fait rien.
    Toggle,
    /// Position absolue d'un potard ou d'un fader, 0..1 ; pour une bascule, `On` au-dessus de la moitié.
    Absolute,
    /// Décalage d'un encodeur sans fin (CC relatif ou valeur OSC signée).
    Relative,
}

/// La forme de valeur qu'attend une action, qui décide des modes possibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActionKind {
    /// Ordre ponctuel, sans valeur (`Go`, `ForceDrop`…).
    Impulse,
    /// Bascule (`Blackout`, `Hold`…).
    Switch,
    /// Grandeur continue 0..1 (`Master`, `Macro`…).
    Level,
    /// Quantité signée par appui ou par cran (`NudgePhase`).
    Amount,
}

impl ActionKind {
    /// Les modes qui ont un sens pour cette forme de valeur.
    #[must_use]
    pub fn accepts(self, mode: Mode) -> bool {
        match self {
            Self::Impulse => mode == Mode::Momentary,
            Self::Switch => matches!(mode, Mode::Momentary | Mode::Toggle | Mode::Absolute),
            Self::Level => matches!(mode, Mode::Momentary | Mode::Absolute | Mode::Relative),
            Self::Amount => matches!(mode, Mode::Momentary | Mode::Relative),
        }
    }

    /// Le mode proposé par défaut (MIDI learn, profil écrit à la main) : bascule pour une bascule, position pour
    /// une grandeur, momentané sinon.
    #[must_use]
    pub fn default_mode(self) -> Mode {
        match self {
            Self::Impulse | Self::Amount => Mode::Momentary,
            Self::Switch => Mode::Toggle,
            Self::Level => Mode::Absolute,
        }
    }
}

/// L'action visée par un binding, sans sa valeur. Mêmes variantes que [`Action`] ; les champs qui restent sont ceux
/// que l'entrée ne peut pas donner (quel cue, quelle macro, quel titre).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ActionTemplate {
    Go,
    Back,
    GotoCue(CueIndex),
    Hold,
    NextDj,
    PrevDj,
    ArmDj(DjIndex),
    ForceDrop,
    SkipShot,
    ShowDj,
    Title(TitleId),
    Trigger(TriggerId),
    RandomEvent,
    Ascension,
    Macro(MacroIndex),
    Master,
    LayerFade(Layer),
    Blackout,
    SafeLook,
    FlashKill,
    Breathing,
    TapTempo,
    Resync,
    /// `beats` : décalage par appui (mode momentané) ou par cran (mode relatif), en temps, signé.
    NudgePhase {
        beats: f32,
    },
    Playback,
    OperatorOverlay,
}

impl ActionTemplate {
    /// La forme de valeur attendue.
    #[must_use]
    pub fn kind(&self) -> ActionKind {
        match self {
            Self::Hold
            | Self::Blackout
            | Self::FlashKill
            | Self::Breathing
            | Self::Playback
            | Self::OperatorOverlay => ActionKind::Switch,
            Self::Macro(_) | Self::Master | Self::LayerFade(_) => ActionKind::Level,
            Self::NudgePhase { .. } => ActionKind::Amount,
            _ => ActionKind::Impulse,
        }
    }

    /// La famille de l'action produite, pour l'interface.
    #[must_use]
    pub fn family(&self) -> ActionFamily {
        match self {
            Self::Go | Self::Back | Self::GotoCue(_) | Self::Hold | Self::NextDj | Self::PrevDj | Self::ArmDj(_) => {
                ActionFamily::Conduct
            }
            Self::ForceDrop
            | Self::SkipShot
            | Self::ShowDj
            | Self::Title(_)
            | Self::Trigger(_)
            | Self::RandomEvent
            | Self::Ascension => ActionFamily::OneShot,
            Self::Macro(_) | Self::Master | Self::LayerFade(_) => ActionFamily::Continuous,
            Self::Blackout | Self::SafeLook | Self::FlashKill | Self::Breathing => ActionFamily::Safety,
            Self::TapTempo | Self::Resync | Self::NudgePhase { .. } => ActionFamily::Tempo,
            Self::Playback | Self::OperatorOverlay => ActionFamily::App,
        }
    }

    /// L'action ponctuelle, si c'en est une.
    #[must_use]
    pub fn impulse(&self) -> Option<Action> {
        Some(match self {
            Self::Go => Action::Go,
            Self::Back => Action::Back,
            Self::GotoCue(i) => Action::GotoCue(*i),
            Self::NextDj => Action::NextDj,
            Self::PrevDj => Action::PrevDj,
            Self::ArmDj(i) => Action::ArmDj(*i),
            Self::ForceDrop => Action::ForceDrop,
            Self::SkipShot => Action::SkipShot,
            Self::ShowDj => Action::ShowDj,
            Self::Title(id) => Action::Title(id.clone()),
            Self::Trigger(id) => Action::Trigger(id.clone()),
            Self::RandomEvent => Action::RandomEvent,
            Self::Ascension => Action::Ascension,
            Self::SafeLook => Action::SafeLook,
            Self::TapTempo => Action::TapTempo,
            Self::Resync => Action::Resync,
            _ => return None,
        })
    }

    /// L'action de bascule avec cet ordre, si c'est une bascule.
    #[must_use]
    pub fn with_switch(&self, switch: Switch) -> Option<Action> {
        Some(match self {
            Self::Hold => Action::Hold(switch),
            Self::Blackout => Action::Blackout(switch),
            Self::FlashKill => Action::FlashKill(switch),
            Self::Breathing => Action::Breathing(switch),
            Self::Playback => Action::Playback(switch),
            Self::OperatorOverlay => Action::OperatorOverlay(switch),
            _ => return None,
        })
    }

    /// L'action continue avec cette valeur, si c'est une grandeur continue.
    #[must_use]
    pub fn with_level(&self, value: Level) -> Option<Action> {
        Some(match self {
            Self::Macro(index) => Action::Macro { index: *index, value },
            Self::Master => Action::Master(value),
            Self::LayerFade(layer) => Action::LayerFade { layer: *layer, value },
            _ => return None,
        })
    }

    /// Le décalage de phase pour `steps` appuis ou crans (négatif : en arrière), si c'est un `NudgePhase`.
    #[must_use]
    pub fn with_amount(&self, steps: f32) -> Option<Action> {
        match self {
            Self::NudgePhase { beats } => Some(Action::NudgePhase { beats: beats * steps }),
            _ => None,
        }
    }
}

impl From<&Action> for ActionTemplate {
    /// Le gabarit d'une action (sa valeur est oubliée ; pour `NudgePhase`, le décalage devient le pas).
    fn from(action: &Action) -> Self {
        match action {
            Action::Go => Self::Go,
            Action::Back => Self::Back,
            Action::GotoCue(i) => Self::GotoCue(*i),
            Action::Hold(_) => Self::Hold,
            Action::NextDj => Self::NextDj,
            Action::PrevDj => Self::PrevDj,
            Action::ArmDj(i) => Self::ArmDj(*i),
            Action::ForceDrop => Self::ForceDrop,
            Action::SkipShot => Self::SkipShot,
            Action::ShowDj => Self::ShowDj,
            Action::Title(id) => Self::Title(id.clone()),
            Action::Trigger(id) => Self::Trigger(id.clone()),
            Action::RandomEvent => Self::RandomEvent,
            Action::Ascension => Self::Ascension,
            Action::Macro { index, .. } => Self::Macro(*index),
            Action::Master(_) => Self::Master,
            Action::LayerFade { layer, .. } => Self::LayerFade(*layer),
            Action::Blackout(_) => Self::Blackout,
            Action::SafeLook => Self::SafeLook,
            Action::FlashKill(_) => Self::FlashKill,
            Action::Breathing(_) => Self::Breathing,
            Action::TapTempo => Self::TapTempo,
            Action::Resync => Self::Resync,
            Action::NudgePhase { beats } => Self::NudgePhase { beats: *beats },
            Action::Playback(_) => Self::Playback,
            Action::OperatorOverlay(_) => Self::OperatorOverlay,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_kind_has_a_default_mode_it_accepts() {
        for kind in [
            ActionKind::Impulse,
            ActionKind::Switch,
            ActionKind::Level,
            ActionKind::Amount,
        ] {
            assert!(kind.accepts(kind.default_mode()), "{kind:?}");
        }
        assert!(!ActionKind::Impulse.accepts(Mode::Toggle));
        assert!(!ActionKind::Switch.accepts(Mode::Relative));
    }

    #[test]
    fn templates_fill_values() {
        assert_eq!(
            ActionTemplate::Blackout.with_switch(Switch::On),
            Some(Action::Blackout(Switch::On))
        );
        assert_eq!(
            ActionTemplate::Macro(MacroIndex(2)).with_level(Level::Set(0.5)),
            Some(Action::Macro {
                index: MacroIndex(2),
                value: Level::Set(0.5)
            })
        );
        assert_eq!(
            ActionTemplate::NudgePhase { beats: 0.25 }.with_amount(-2.0),
            Some(Action::NudgePhase { beats: -0.5 })
        );
        assert_eq!(ActionTemplate::Go.with_level(Level::Set(1.0)), None);
        assert_eq!(ActionTemplate::Master.impulse(), None);
    }

    fn all_templates() -> Vec<ActionTemplate> {
        vec![
            ActionTemplate::Go,
            ActionTemplate::Back,
            ActionTemplate::GotoCue(CueIndex(3)),
            ActionTemplate::Hold,
            ActionTemplate::NextDj,
            ActionTemplate::PrevDj,
            ActionTemplate::ArmDj(DjIndex(1)),
            ActionTemplate::ForceDrop,
            ActionTemplate::SkipShot,
            ActionTemplate::ShowDj,
            ActionTemplate::Title(TitleId::new("merci")),
            ActionTemplate::Trigger(TriggerId::new("storm")),
            ActionTemplate::RandomEvent,
            ActionTemplate::Ascension,
            ActionTemplate::Macro(MacroIndex(0)),
            ActionTemplate::Master,
            ActionTemplate::LayerFade(Layer::Background),
            ActionTemplate::Blackout,
            ActionTemplate::SafeLook,
            ActionTemplate::FlashKill,
            ActionTemplate::Breathing,
            ActionTemplate::TapTempo,
            ActionTemplate::Resync,
            ActionTemplate::NudgePhase { beats: 0.5 },
            ActionTemplate::Playback,
            ActionTemplate::OperatorOverlay,
        ]
    }

    /// Chaque gabarit se remplit par exactement un chemin, celui de sa forme, et l'action produite redonne le même
    /// gabarit et la même famille.
    #[test]
    fn every_template_fills_through_its_kind_only() {
        for template in all_templates() {
            let filled = [
                (ActionKind::Impulse, template.impulse()),
                (ActionKind::Switch, template.with_switch(Switch::On)),
                (ActionKind::Level, template.with_level(Level::Set(0.5))),
                (ActionKind::Amount, template.with_amount(1.0)),
            ];
            let produced: Vec<_> = filled.into_iter().filter_map(|(k, a)| a.map(|a| (k, a))).collect();
            assert_eq!(produced.len(), 1, "{template:?}");
            let (kind, action) = &produced[0];
            assert_eq!(*kind, template.kind(), "{template:?}");
            assert_eq!(ActionTemplate::from(action), template);
            assert_eq!(action.family(), template.family(), "{template:?}");
        }
    }

    #[test]
    fn families_follow_the_action() {
        assert_eq!(ActionTemplate::Blackout.family(), ActionFamily::Safety);
        assert_eq!(ActionTemplate::Macro(MacroIndex(0)).family(), ActionFamily::Continuous);
        assert_eq!(ActionTemplate::Playback.family(), ActionFamily::App);
    }
}
