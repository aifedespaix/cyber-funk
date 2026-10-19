//! Profils de contrôle : une table de bindings par contrôleur (ou pour le clavier), lue et écrite en RON.
//!
//! Un profil est validé à chaque chargement : un binding mal formé ou deux bindings qui se disputent le même geste
//! sont refusés avec la liste complète des problèmes, pour que l'opérateur les corrige tous d'un coup, à la maison,
//! et pas un par un le soir du show.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::action::MACRO_COUNT;
use crate::error::{Error, Result};
use crate::source::{CC14_MSB_MAX, InputSource, MIDI_CHANNEL_MAX, MIDI_DATA_MAX};
use crate::template::{ActionKind, ActionTemplate, Mode};

/// Une ligne de la table : tel geste sur telle source produit telle action, lue selon tel mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Binding {
    pub source: InputSource,
    pub action: ActionTemplate,
    pub mode: Mode,
}

impl Binding {
    /// Un binding dans le mode par défaut de son action (voir [`ActionKind::default_mode`](crate::ActionKind)).
    #[must_use]
    pub fn new(source: InputSource, action: ActionTemplate) -> Self {
        let mode = action.kind().default_mode();
        Self { source, action, mode }
    }
}

/// Un profil de contrôle : le mapping d'un contrôleur, attachable à l'événement ou au créneau d'un DJ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlProfile {
    /// Nom affiché (« Clavier (Sillage) », « DDJ-FLX4 de KRMN »).
    pub name: String,
    pub bindings: Vec<Binding>,
}

/// Un problème trouvé dans un profil. `binding` est le rang de la ligne fautive dans `bindings`, à partir de 0.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileIssue {
    /// Deux lignes du même profil réagissent au même geste.
    Conflict {
        first: usize,
        second: usize,
        source: InputSource,
    },
    /// Le mode n'a pas de sens pour cette action (bascule sur `Go`, position sur `Blackout`…).
    ModeNotForAction {
        binding: usize,
        action: ActionTemplate,
        mode: Mode,
    },
    /// Le mode relatif exige une commande continue (CC ou OSC), pas une touche ni une note.
    ModeNotForSource { binding: usize, mode: Mode },
    /// Canal, note ou CC hors des données MIDI 7 bits.
    MidiOutOfRange { binding: usize },
    /// Macro au-delà de [`MACRO_COUNT`].
    MacroOutOfRange { binding: usize, index: u8 },
    /// Adresse OSC vide ou qui ne commence pas par `/`.
    BadOscAddress { binding: usize },
    /// CC 14 bits : l'octet de poids fort doit être un CC de 0 à 31 et le mode la position absolue.
    WideCc { binding: usize },
    /// Un Program Change n'a pas de relâchement : seuls conviennent une impulsion, une bascule ou un pas.
    ProgramHasNoRelease { binding: usize, mode: Mode },
    /// Identifiant de titre ou de déclencheur vide.
    EmptyId { binding: usize },
    /// Pas de `NudgePhase` nul ou non fini.
    BadNudgeStep { binding: usize },
}

impl fmt::Display for ProfileIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Les lignes sont numérotées à partir de 1 pour l'opérateur.
        match self {
            Self::Conflict { first, second, source } => {
                write!(
                    f,
                    "lignes {} et {} : « {source} » déclencherait deux actions",
                    first + 1,
                    second + 1
                )
            }
            Self::ModeNotForAction { binding, action, mode } => {
                write!(
                    f,
                    "ligne {} : le mode {mode:?} ne convient pas à l'action {action:?}",
                    binding + 1
                )
            }
            Self::ModeNotForSource { binding, mode } => {
                write!(
                    f,
                    "ligne {} : le mode {mode:?} exige un potard, un encodeur ou une adresse OSC",
                    binding + 1
                )
            }
            Self::MidiOutOfRange { binding } => {
                write!(
                    f,
                    "ligne {} : canal (0 à 15), note ou CC (0 à 127) hors limites",
                    binding + 1
                )
            }
            Self::MacroOutOfRange { binding, index } => {
                write!(
                    f,
                    "ligne {} : macro {index} inexistante (0 à {})",
                    binding + 1,
                    MACRO_COUNT - 1
                )
            }
            Self::WideCc { binding } => {
                write!(
                    f,
                    "ligne {} : un CC 14 bits est lu en position absolue, avec un octet de poids fort de 0 à {CC14_MSB_MAX}",
                    binding + 1
                )
            }
            Self::ProgramHasNoRelease { binding, mode } => {
                write!(
                    f,
                    "ligne {} : un Program Change n'a pas de relâchement, le mode {mode:?} ne convient pas",
                    binding + 1
                )
            }
            Self::BadOscAddress { binding } => {
                write!(f, "ligne {} : une adresse OSC commence par « / »", binding + 1)
            }
            Self::EmptyId { binding } => write!(f, "ligne {} : identifiant vide", binding + 1),
            Self::BadNudgeStep { binding } => {
                write!(
                    f,
                    "ligne {} : le pas de NudgePhase doit être un nombre non nul",
                    binding + 1
                )
            }
        }
    }
}

impl ControlProfile {
    /// Un profil vide.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            bindings: Vec::new(),
        }
    }

    /// Lit un profil RON et le valide.
    ///
    /// # Errors
    /// [`Error::Parse`] si le texte n'est pas un profil RON, [`Error::InvalidProfile`] s'il contient des bindings
    /// invalides ou en conflit.
    pub fn from_ron(text: &str) -> Result<Self> {
        let profile: Self = ron::from_str(text)?;
        profile.validate()?;
        Ok(profile)
    }

    /// Écrit le profil en RON lisible (noms de structures, une ligne par champ).
    ///
    /// # Errors
    /// [`Error::Serialize`] si la sérialisation échoue (ne devrait pas arriver pour un profil valide).
    pub fn to_ron(&self) -> Result<String> {
        let config = ron::ser::PrettyConfig::new().struct_names(true);
        Ok(ron::ser::to_string_pretty(self, config)?)
    }

    /// Pose un binding (MIDI learn, édition à la main) : les bindings du profil qui réagissaient au même geste sont
    /// retirés et rendus, pour que l'interface dise ce qui a été remplacé. Le binding est ajouté à la fin.
    pub fn set_binding(&mut self, binding: Binding) -> Vec<Binding> {
        let mut replaced = Vec::new();
        let mut i = 0;
        while i < self.bindings.len() {
            if self.bindings[i].source.overlaps(&binding.source) {
                replaced.push(self.bindings.remove(i));
            } else {
                i += 1;
            }
        }
        self.bindings.push(binding);
        replaced
    }

    /// Tous les problèmes du profil, dans l'ordre des lignes ; vide si le profil est bon.
    #[must_use]
    pub fn issues(&self) -> Vec<ProfileIssue> {
        let mut issues = Vec::new();
        for (i, binding) in self.bindings.iter().enumerate() {
            check_binding(i, binding, &mut issues);
        }
        for (i, a) in self.bindings.iter().enumerate() {
            for (j, b) in self.bindings.iter().enumerate().skip(i + 1) {
                if a.source.overlaps(&b.source) {
                    issues.push(ProfileIssue::Conflict {
                        first: i,
                        second: j,
                        source: a.source.clone(),
                    });
                }
            }
        }
        issues
    }

    /// Valide le profil.
    ///
    /// # Errors
    /// [`Error::InvalidProfile`] avec tous les problèmes trouvés.
    pub fn validate(&self) -> Result<()> {
        let issues = self.issues();
        if issues.is_empty() {
            Ok(())
        } else {
            Err(Error::InvalidProfile {
                profile: self.name.clone(),
                issues,
            })
        }
    }
}

/// Les vérifications propres à une ligne (les conflits, eux, se jugent à deux).
fn check_binding(i: usize, binding: &Binding, issues: &mut Vec<ProfileIssue>) {
    let kind = binding.action.kind();
    if !kind.accepts(binding.mode) {
        issues.push(ProfileIssue::ModeNotForAction {
            binding: i,
            action: binding.action.clone(),
            mode: binding.mode,
        });
    }
    if binding.mode == Mode::Relative && binding.source.is_button() {
        issues.push(ProfileIssue::ModeNotForSource {
            binding: i,
            mode: binding.mode,
        });
    }
    match &binding.source {
        InputSource::MidiNote {
            channel, note: data, ..
        }
        | InputSource::MidiCc { channel, cc: data, .. }
        | InputSource::MidiCc14 { channel, cc: data, .. }
        | InputSource::MidiProgram {
            channel, program: data, ..
        } if *channel > MIDI_CHANNEL_MAX || *data > MIDI_DATA_MAX =>
        {
            issues.push(ProfileIssue::MidiOutOfRange { binding: i });
        }
        InputSource::Osc { address } if !address.starts_with('/') => {
            issues.push(ProfileIssue::BadOscAddress { binding: i });
        }
        _ => {}
    }
    match &binding.source {
        InputSource::MidiCc14 { cc, .. } if *cc > CC14_MSB_MAX || binding.mode != Mode::Absolute => {
            issues.push(ProfileIssue::WideCc { binding: i });
        }
        // Un mode que l'action n'accepte pas est déjà signalé plus haut : ne pas le répéter.
        InputSource::MidiProgram { .. }
            if kind.accepts(binding.mode)
                && binding.mode != Mode::Relative
                && !matches!(
                    (kind, binding.mode),
                    (ActionKind::Impulse | ActionKind::Amount, Mode::Momentary) | (ActionKind::Switch, Mode::Toggle)
                ) =>
        {
            issues.push(ProfileIssue::ProgramHasNoRelease {
                binding: i,
                mode: binding.mode,
            });
        }
        _ => {}
    }
    match &binding.action {
        ActionTemplate::Macro(index) if index.0 >= MACRO_COUNT => {
            issues.push(ProfileIssue::MacroOutOfRange {
                binding: i,
                index: index.0,
            });
        }
        ActionTemplate::Title(id) if id.0.trim().is_empty() => issues.push(ProfileIssue::EmptyId { binding: i }),
        ActionTemplate::Trigger(id) if id.0.trim().is_empty() => issues.push(ProfileIssue::EmptyId { binding: i }),
        ActionTemplate::NudgePhase { beats } if !beats.is_finite() || *beats == 0.0 => {
            issues.push(ProfileIssue::BadNudgeStep { binding: i });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::MacroIndex;
    use crate::keyboard::{Key, KeyCode};
    use crate::source::RelativeEncoding;

    fn key(c: char) -> InputSource {
        InputSource::key(Key::Char(c))
    }

    #[test]
    fn conflicting_bindings_are_all_reported() {
        let mut profile = ControlProfile::new("test");
        profile.bindings.push(Binding::new(key('d'), ActionTemplate::ForceDrop));
        profile.bindings.push(Binding::new(key('D'), ActionTemplate::Go));
        profile.bindings.push(Binding::new(
            InputSource::key(Key::Code(KeyCode::Space)),
            ActionTemplate::Back,
        ));
        profile.bindings.push(Binding::new(
            InputSource::key(Key::Code(KeyCode::Space)),
            ActionTemplate::Hold,
        ));
        let issues = profile.issues();
        assert_eq!(issues.len(), 2, "{issues:?}");
        assert!(matches!(
            issues[0],
            ProfileIssue::Conflict {
                first: 0,
                second: 1,
                ..
            }
        ));
        assert!(matches!(
            issues[1],
            ProfileIssue::Conflict {
                first: 2,
                second: 3,
                ..
            }
        ));
        assert!(matches!(profile.validate(), Err(Error::InvalidProfile { .. })));
    }

    #[test]
    fn malformed_bindings_are_reported() {
        let mut profile = ControlProfile::new("test");
        let b = |source, action, mode| Binding { source, action, mode };
        profile.bindings = vec![
            b(key('a'), ActionTemplate::Go, Mode::Toggle),
            b(key('b'), ActionTemplate::Master, Mode::Relative),
            b(
                InputSource::MidiCc {
                    port: None,
                    channel: 16,
                    cc: 1,
                    encoding: RelativeEncoding::default(),
                },
                ActionTemplate::Master,
                Mode::Absolute,
            ),
            b(
                InputSource::Osc {
                    address: "cf/go".into(),
                },
                ActionTemplate::Go,
                Mode::Momentary,
            ),
            b(key('c'), ActionTemplate::Macro(MacroIndex(8)), Mode::Absolute),
            b(key('e'), ActionTemplate::NudgePhase { beats: 0.0 }, Mode::Momentary),
        ];
        let issues = profile.issues();
        assert_eq!(
            issues,
            vec![
                ProfileIssue::ModeNotForAction {
                    binding: 0,
                    action: ActionTemplate::Go,
                    mode: Mode::Toggle
                },
                ProfileIssue::ModeNotForSource {
                    binding: 1,
                    mode: Mode::Relative
                },
                ProfileIssue::MidiOutOfRange { binding: 2 },
                ProfileIssue::BadOscAddress { binding: 3 },
                ProfileIssue::MacroOutOfRange { binding: 4, index: 8 },
                ProfileIssue::BadNudgeStep { binding: 5 },
            ]
        );
    }
}
