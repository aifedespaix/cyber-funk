//! Les commandes de la vue Live : une table unique qui dit, pour chaque bouton, son libellé, son raccourci et l'`Action`
//! qu'il produit.
//!
//! Le bouton à l'écran et la touche passent par la même ligne de cette table : impossible d'ajouter un bouton sans
//! raccourci (le test `chaque_commande_a_un_raccourci_affiche` le garde), ni d'afficher un raccourci qui ne fait rien.
//!
//! Deux sortes de raccourcis :
//! - [`Shortcut::Bridge`] : déjà lié par le clavier par défaut de `cf-control` (les touches de Sillage), lu par le pont
//!   des contrôles de `cf-render` dans les deux fenêtres. La vue ne fait que l'afficher, pour ne pas envoyer deux fois
//!   l'ordre ;
//! - [`Shortcut::Press`] / [`Shortcut::Hold`] : touches propres à la vue Live, lues dans la fenêtre opérateur
//!   ([`poll`], [`key_down`]). Aucune ne recoupe le clavier par défaut.
//!
//! Aucune commande n'écrit de luminance : tout passe par des `Action`, donc par le chef du spectacle, et les flashs
//! restent plafonnés par le limiteur photosensible de `cf-show` (règle 7), `FlashKill` ou non.

use bevy_egui::egui::{self, Event, Key};
use cf_control::{Action, DjIndex, Level, MacroIndex, Switch, TitleId};

/// Pas des potards au clavier, en fraction de la course.
pub(crate) const KNOB_STEP: f32 = 0.05;
/// Pas du décalage de phase, en temps (beats).
pub(crate) const NUDGE_BEATS: f32 = 0.25;
/// Nombre de macros que la vue pilote au clavier.
pub(crate) const MACROS: usize = 8;

/// Comment une commande se déclenche au clavier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortcut {
    /// Touche du clavier par défaut, lue par le pont des contrôles. Texte affiché.
    Bridge(&'static str),
    /// Appui unique, lu par la vue.
    Press {
        key: Key,
        shift: bool,
        ctrl: bool,
        /// La répétition automatique de la touche compte (balayage d'un potard).
        repeat: bool,
        /// Texte affiché.
        text: &'static str,
    },
    /// Maintien : actif tant que la touche est enfoncée.
    Hold { key: Key, text: &'static str },
}

/// Ce que la vue offre à l'opérateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveCommand {
    Go,
    Back,
    Hold,
    NextDj,
    PrevDj,
    ArmNext,
    ForceDrop,
    SkipShot,
    ShowDj,
    RandomEvent,
    Ascension,
    Title,
    Blackout,
    SafeLook,
    FlashKill,
    Breathing,
    MasterUp,
    MasterDown,
    MacroUp(u8),
    MacroDown(u8),
    TapTempo,
    Resync,
    NudgeBack,
    NudgeForward,
    SwitchMode,
}

/// Toutes les commandes, dans l'ordre de la vue.
pub(crate) const ALL: [LiveCommand; 39] = [
    LiveCommand::Go,
    LiveCommand::Back,
    LiveCommand::Hold,
    LiveCommand::NextDj,
    LiveCommand::PrevDj,
    LiveCommand::ArmNext,
    LiveCommand::ForceDrop,
    LiveCommand::SkipShot,
    LiveCommand::ShowDj,
    LiveCommand::RandomEvent,
    LiveCommand::Ascension,
    LiveCommand::Title,
    LiveCommand::Blackout,
    LiveCommand::SafeLook,
    LiveCommand::FlashKill,
    LiveCommand::Breathing,
    LiveCommand::MasterUp,
    LiveCommand::MasterDown,
    LiveCommand::MacroUp(0),
    LiveCommand::MacroUp(1),
    LiveCommand::MacroUp(2),
    LiveCommand::MacroUp(3),
    LiveCommand::MacroUp(4),
    LiveCommand::MacroUp(5),
    LiveCommand::MacroUp(6),
    LiveCommand::MacroUp(7),
    LiveCommand::TapTempo,
    LiveCommand::Resync,
    LiveCommand::NudgeBack,
    LiveCommand::NudgeForward,
    LiveCommand::SwitchMode,
    // Les potards vers le bas : même touche avec Maj.
    LiveCommand::MacroDown(0),
    LiveCommand::MacroDown(1),
    LiveCommand::MacroDown(2),
    LiveCommand::MacroDown(3),
    LiveCommand::MacroDown(4),
    LiveCommand::MacroDown(5),
    LiveCommand::MacroDown(6),
    LiveCommand::MacroDown(7),
];

/// Les touches de macro : F5 à F12 (F3 est le panneau de diagnostic).
const MACRO_KEYS: [(Key, &str, &str); MACROS] = [
    (Key::F5, "F5", "Maj+F5"),
    (Key::F6, "F6", "Maj+F6"),
    (Key::F7, "F7", "Maj+F7"),
    (Key::F8, "F8", "Maj+F8"),
    (Key::F9, "F9", "Maj+F9"),
    (Key::F10, "F10", "Maj+F10"),
    (Key::F11, "F11", "Maj+F11"),
    (Key::F12, "F12", "Maj+F12"),
];

const fn press(key: Key, text: &'static str) -> Shortcut {
    Shortcut::Press {
        key,
        shift: false,
        ctrl: false,
        repeat: false,
        text,
    }
}

/// Ce dont une commande a besoin pour devenir une `Action` : l'état du show, jamais une copie tenue par la vue.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CommandContext<'a> {
    /// Créneau en cours.
    pub slot: usize,
    /// Nombre de créneaux de l'événement.
    pub slot_count: usize,
    /// Créneau armé.
    pub armed: Option<usize>,
    /// Nom du cue à l'écran (le carton-titre « TITRE »).
    pub cue_name: &'a str,
    /// Nombre de macros du look en cours.
    pub macro_count: usize,
}

impl LiveCommand {
    /// Le libellé du bouton.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Go => "GO",
            Self::Back => "BACK",
            Self::Hold => "HOLD",
            Self::NextDj => "DJ suivant",
            Self::PrevDj => "DJ précédent",
            Self::ArmNext => "Armer le suivant",
            Self::ForceDrop => "DROP",
            Self::SkipShot => "PLAN SUIVANT",
            Self::ShowDj => "LOGO DJ",
            Self::RandomEvent => "ÉVÉNEMENT",
            Self::Ascension => "ASCENSION",
            Self::Title => "TITRE",
            Self::Blackout => "BLACKOUT",
            Self::SafeLook => "LOOK CALME",
            Self::FlashKill => "FLASHS",
            Self::Breathing => "RESPIRATION",
            Self::MasterUp => "Master +",
            Self::MasterDown => "Master −",
            Self::MacroUp(_) => "Macro +",
            Self::MacroDown(_) => "Macro −",
            Self::TapTempo => "TAP",
            Self::Resync => "RESYNC",
            Self::NudgeBack => "Phase −",
            Self::NudgeForward => "Phase +",
            Self::SwitchMode => "Mode",
        }
    }

    /// Le raccourci de la commande.
    pub(crate) fn shortcut(self) -> Shortcut {
        match self {
            Self::Go => Shortcut::Bridge("Droite"),
            Self::Back => Shortcut::Bridge("Gauche"),
            Self::Hold => press(Key::H, "H"),
            Self::NextDj => Shortcut::Bridge("G"),
            Self::PrevDj => Shortcut::Bridge("Maj+G"),
            Self::ArmNext => press(Key::J, "J"),
            Self::ForceDrop => Shortcut::Bridge("D"),
            Self::SkipShot => Shortcut::Bridge("N"),
            Self::ShowDj => Shortcut::Bridge("T"),
            Self::RandomEvent => Shortcut::Bridge("E"),
            Self::Ascension => Shortcut::Bridge("A"),
            Self::Title => press(Key::C, "C"),
            Self::Blackout => Shortcut::Hold {
                key: Key::K,
                text: "K (maintenir)",
            },
            Self::SafeLook => press(Key::L, "L"),
            Self::FlashKill => Shortcut::Bridge("X"),
            Self::Breathing => Shortcut::Bridge("B"),
            Self::MasterUp => Shortcut::Press {
                key: Key::PageUp,
                shift: false,
                ctrl: false,
                repeat: true,
                text: "PgUp",
            },
            Self::MasterDown => Shortcut::Press {
                key: Key::PageDown,
                shift: false,
                ctrl: false,
                repeat: true,
                text: "PgDn",
            },
            Self::MacroUp(i) | Self::MacroDown(i) => {
                let (key, up, down) = MACRO_KEYS[usize::from(i).min(MACROS - 1)];
                let is_down = matches!(self, Self::MacroDown(_));
                Shortcut::Press {
                    key,
                    shift: is_down,
                    ctrl: false,
                    repeat: true,
                    text: if is_down { down } else { up },
                }
            }
            Self::TapTempo => press(Key::P, "P"),
            Self::Resync => press(Key::R, "R"),
            Self::NudgeBack => press(Key::Comma, ","),
            Self::NudgeForward => press(Key::Period, "."),
            Self::SwitchMode => Shortcut::Press {
                key: Key::M,
                shift: false,
                ctrl: true,
                repeat: false,
                text: "Ctrl+M",
            },
        }
    }

    /// Le raccourci tel qu'on l'affiche sur le bouton.
    pub(crate) fn shortcut_text(self) -> &'static str {
        match self.shortcut() {
            Shortcut::Bridge(text) | Shortcut::Press { text, .. } | Shortcut::Hold { text, .. } => text,
        }
    }

    /// L'action produite par un clic ou une touche. `None` : rien à faire dans la situation présente (un seul
    /// créneau, pas de macro de ce rang, bouton de mode qui n'est pas une action du spectacle).
    pub(crate) fn action(self, ctx: &CommandContext<'_>) -> Option<Action> {
        Some(match self {
            Self::Go => Action::Go,
            Self::Back => Action::Back,
            Self::Hold => Action::Hold(Switch::Toggle),
            Self::NextDj => Action::NextDj,
            Self::PrevDj => Action::PrevDj,
            Self::ArmNext => Action::ArmDj(DjIndex(u16::try_from(next_slot(ctx)?).ok()?)),
            Self::ForceDrop => Action::ForceDrop,
            Self::SkipShot => Action::SkipShot,
            Self::ShowDj => Action::ShowDj,
            Self::RandomEvent => Action::RandomEvent,
            Self::Ascension => Action::Ascension,
            Self::Title => Action::Title(TitleId::new(ctx.cue_name)),
            // Le bouton verrouille (bascule résolue par le chef du spectacle) ; la touche, elle, se maintient.
            Self::Blackout => Action::Blackout(Switch::Toggle),
            Self::SafeLook => Action::SafeLook,
            Self::FlashKill => Action::FlashKill(Switch::Toggle),
            Self::Breathing => Action::Breathing(Switch::Toggle),
            Self::MasterUp => Action::Master(Level::Shift(KNOB_STEP)),
            Self::MasterDown => Action::Master(Level::Shift(-KNOB_STEP)),
            Self::MacroUp(i) => macro_shift(ctx, i, KNOB_STEP)?,
            Self::MacroDown(i) => macro_shift(ctx, i, -KNOB_STEP)?,
            Self::TapTempo => Action::TapTempo,
            Self::Resync => Action::Resync,
            Self::NudgeBack => Action::NudgePhase { beats: -NUDGE_BEATS },
            Self::NudgeForward => Action::NudgePhase { beats: NUDGE_BEATS },
            Self::SwitchMode => return None,
        })
    }
}

fn macro_shift(ctx: &CommandContext<'_>, index: u8, delta: f32) -> Option<Action> {
    (usize::from(index) < ctx.macro_count).then_some(Action::Macro {
        index: MacroIndex(index),
        value: Level::Shift(delta),
    })
}

/// Le créneau que « Armer le suivant » vise : celui d'après l'armé (ou d'après le courant), en boucle, jamais le
/// courant. `None` s'il n'y a qu'un créneau.
pub(crate) fn next_slot(ctx: &CommandContext<'_>) -> Option<usize> {
    if ctx.slot_count < 2 {
        return None;
    }
    let mut next = (ctx.armed.unwrap_or(ctx.slot) + 1) % ctx.slot_count;
    if next == ctx.slot {
        next = (next + 1) % ctx.slot_count;
    }
    Some(next)
}

/// Les modificateurs d'un appui valent-ils ceux du raccourci ? Alt n'est jamais utilisé : Alt+Tab ne doit rien
/// déclencher.
fn modifiers_match(modifiers: egui::Modifiers, shift: bool, ctrl: bool) -> bool {
    modifiers.shift == shift && (modifiers.command || modifiers.ctrl) == ctrl && !modifiers.alt
}

/// Les commandes à appui dont la touche vient d'être pressée dans cette image, ajoutées à `out`.
pub(crate) fn poll(input: &egui::InputState, out: &mut Vec<LiveCommand>) {
    for command in ALL {
        let Shortcut::Press {
            key,
            shift,
            ctrl,
            repeat,
            ..
        } = command.shortcut()
        else {
            continue;
        };
        let hit = input.events.iter().any(|event| {
            matches!(
                event,
                Event::Key { key: k, pressed: true, repeat: r, modifiers, .. }
                    if *k == key && (repeat || !*r) && modifiers_match(*modifiers, shift, ctrl)
            )
        });
        if hit {
            out.push(command);
        }
    }
}

/// La touche de maintien de cette commande est-elle enfoncée (sans Ctrl ni Alt) ?
pub(crate) fn key_down(input: &egui::InputState, command: LiveCommand) -> bool {
    let Shortcut::Hold { key, .. } = command.shortcut() else {
        return false;
    };
    input.key_down(key) && !input.modifiers.command && !input.modifiers.ctrl && !input.modifiers.alt
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(slot: usize, count: usize, armed: Option<usize>) -> CommandContext<'a> {
        CommandContext {
            slot,
            slot_count: count,
            armed,
            cue_name: "Démesure",
            macro_count: 3,
        }
    }

    #[test]
    fn chaque_commande_a_un_raccourci_affiche() {
        for command in ALL {
            assert!(
                !command.shortcut_text().is_empty(),
                "{command:?} sans raccourci affiché"
            );
            assert!(!command.label().is_empty());
        }
    }

    #[test]
    fn aucune_touche_propre_a_la_vue_ne_recoupe_une_autre() {
        let mut seen: Vec<(Key, bool, bool)> = Vec::new();
        for command in ALL {
            let (key, shift, ctrl) = match command.shortcut() {
                Shortcut::Press { key, shift, ctrl, .. } => (key, shift, ctrl),
                Shortcut::Hold { key, .. } => (key, false, false),
                Shortcut::Bridge(_) => continue,
            };
            assert!(!seen.contains(&(key, shift, ctrl)), "{command:?} : touche déjà prise");
            seen.push((key, shift, ctrl));
        }
    }

    #[test]
    fn les_touches_de_la_vue_ne_recoupent_pas_le_clavier_par_defaut() {
        // Lettres du clavier par défaut (D N G T A E X B O) : la vue ne les relit pas.
        let taken = [Key::D, Key::N, Key::G, Key::T, Key::A, Key::E, Key::X, Key::B, Key::O];
        for command in ALL {
            if let Shortcut::Press { key, .. } | Shortcut::Hold { key, .. } = command.shortcut() {
                assert!(!taken.contains(&key), "{command:?} recoupe le clavier par défaut");
            }
        }
    }

    #[test]
    fn les_actions_sont_celles_de_la_table_de_sillage() {
        let c = ctx(0, 3, None);
        assert_eq!(LiveCommand::Go.action(&c), Some(Action::Go));
        assert_eq!(
            LiveCommand::Hold.action(&c),
            Some(Action::Hold(Switch::Toggle)),
            "bascule résolue par cf-show : jamais de désynchronisation"
        );
        assert_eq!(LiveCommand::Blackout.action(&c), Some(Action::Blackout(Switch::Toggle)));
        assert_eq!(
            LiveCommand::Title.action(&c),
            Some(Action::Title(TitleId::new("Démesure")))
        );
        assert_eq!(LiveCommand::SwitchMode.action(&c), None);
    }

    #[test]
    fn armer_le_suivant_tourne_sans_viser_le_courant() {
        assert_eq!(next_slot(&ctx(0, 3, None)), Some(1));
        assert_eq!(next_slot(&ctx(0, 3, Some(1))), Some(2));
        assert_eq!(next_slot(&ctx(0, 3, Some(2))), Some(1), "saute le créneau courant");
        assert_eq!(next_slot(&ctx(1, 3, None)), Some(2));
        assert_eq!(next_slot(&ctx(0, 1, None)), None);
        assert_eq!(
            LiveCommand::ArmNext.action(&ctx(0, 3, None)),
            Some(Action::ArmDj(DjIndex(1)))
        );
    }

    #[test]
    fn une_macro_absente_du_look_ne_fait_rien() {
        let c = ctx(0, 2, None);
        assert!(LiveCommand::MacroUp(2).action(&c).is_some());
        assert_eq!(LiveCommand::MacroUp(3).action(&c), None);
        assert_eq!(LiveCommand::MacroDown(7).action(&c), None);
    }

    /// Des images egui successives avec ces événements clavier ; `check` lit l'`InputState` de la dernière. egui
    /// décide lui-même de ce qui est une répétition : un appui sur une touche déjà enfoncée.
    fn with_keys(frames: Vec<Vec<Event>>, check: impl FnOnce(&egui::InputState)) {
        let ctx = egui::Context::default();
        let last = frames.len().saturating_sub(1);
        let mut check = Some(check);
        for (n, events) in frames.into_iter().enumerate() {
            let raw = egui::RawInput {
                events,
                ..egui::RawInput::default()
            };
            let output = ctx.run_ui(raw, |ui| {
                if n == last
                    && let Some(check) = check.take()
                {
                    ui.ctx().input(check);
                }
            });
            output.drop_without_applying_deltas();
        }
    }

    fn key_event(key: Key, pressed: bool, repeat: bool, modifiers: egui::Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat,
            modifiers,
        }
    }

    #[test]
    fn la_vue_lit_ses_touches_et_ignore_la_repetition_des_boutons() {
        let none = egui::Modifiers::NONE;
        with_keys(vec![vec![key_event(Key::H, true, false, none)]], |input| {
            let mut out = Vec::new();
            poll(input, &mut out);
            assert_eq!(out, [LiveCommand::Hold]);
        });
        // La répétition d'une touche de bouton ne rebascule pas HOLD.
        with_keys(
            vec![
                vec![key_event(Key::H, true, false, none)],
                vec![key_event(Key::H, true, true, none)],
            ],
            |input| {
                let mut out = Vec::new();
                poll(input, &mut out);
                assert!(out.is_empty());
            },
        );
        // La répétition d'un potard continue de le balayer.
        with_keys(
            vec![
                vec![key_event(Key::PageUp, true, false, none)],
                vec![key_event(Key::PageUp, true, true, none)],
            ],
            |input| {
                let mut out = Vec::new();
                poll(input, &mut out);
                assert_eq!(out, [LiveCommand::MasterUp]);
            },
        );
    }

    #[test]
    fn maj_descend_un_potard_et_ctrl_m_demande_le_changement_de_mode() {
        let shift = egui::Modifiers::SHIFT;
        with_keys(vec![vec![key_event(Key::F6, true, false, shift)]], |input| {
            let mut out = Vec::new();
            poll(input, &mut out);
            assert_eq!(out, [LiveCommand::MacroDown(1)]);
        });
        let ctrl = egui::Modifiers::CTRL;
        with_keys(vec![vec![key_event(Key::M, true, false, ctrl)]], |input| {
            let mut out = Vec::new();
            poll(input, &mut out);
            assert_eq!(out, [LiveCommand::SwitchMode]);
        });
        // M seul, ou avec Alt, ne change rien.
        with_keys(
            vec![vec![
                key_event(Key::M, true, false, egui::Modifiers::NONE),
                key_event(Key::H, true, false, egui::Modifiers::ALT),
            ]],
            |input| {
                let mut out = Vec::new();
                poll(input, &mut out);
                assert!(out.is_empty());
            },
        );
    }
}
