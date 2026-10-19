//! Le profil clavier par défaut : les raccourcis du mode cinéma de Sillage (`engine/show/operator.ts`,
//! `OPERATOR_BINDINGS`), désormais reconfigurables par un profil placé au-dessus dans la pile.

use crate::action::CueIndex;
use crate::keyboard::{Key, KeyCode, ShiftRule};
use crate::profile::{Binding, ControlProfile};
use crate::source::InputSource;
use crate::template::{ActionTemplate, Mode};

/// Nom du profil clavier par défaut.
pub const SILLAGE_KEYBOARD_NAME: &str = "Clavier (Sillage)";

/// Chiffres liés à `GotoCue`. Sillage s'arrêtait à 5 (six niveaux) ; une liste de cues peut être plus longue, les
/// rangées 6 à 9 sont donc liées aussi (un cue absent est ignoré par `cf-show`).
const LAST_DIGIT: u8 = 9;

impl ControlProfile {
    /// Les raccourcis de Sillage, dans l'ordre de sa table :
    ///
    /// | Touche | Action | Sillage |
    /// | --- | --- | --- |
    /// | D | `ForceDrop` | `forceDrop` |
    /// | N | `SkipShot` | `skipShot` |
    /// | → / ← | `Go` / `Back` | `nextLevel` / `prevLevel` |
    /// | 0 à 9 (rangée du haut et pavé) | `GotoCue(n)` | `gotoLevel:n` (0 à 5) |
    /// | G / Maj+G | `NextDj` / `PrevDj` | `nextDj` / `prevDj` |
    /// | T | `ShowDj` | `showDj` |
    /// | A | `Ascension` | `ascension` |
    /// | E | `RandomEvent` | `randomEvent` |
    /// | X | `FlashKill` (bascule) | `toggleFlashes` |
    /// | B | `Breathing` (bascule) | `toggleBreathing` |
    /// | O | `OperatorOverlay` (bascule) | `toggleOperatorHud` |
    /// | Maj+Espace | `Playback` (bascule) | `togglePlay` |
    ///
    /// Les lettres sont lues par caractère (AZERTY compris), le reste par code physique, comme dans Sillage.
    /// Échap (`exit` dans Sillage) n'est pas lié : dans cyber-funk, Échap ne quitte jamais seul (J0-02).
    #[must_use]
    pub fn sillage_keyboard() -> Self {
        let letter = |c: char, action: ActionTemplate| Binding::new(InputSource::key(Key::Char(c)), action);
        let code = |k: KeyCode, action: ActionTemplate| Binding::new(InputSource::key(Key::Code(k)), action);
        let toggle = |source: InputSource, action: ActionTemplate| Binding {
            source,
            action,
            mode: Mode::Toggle,
        };

        let mut bindings = vec![
            letter('d', ActionTemplate::ForceDrop),
            letter('n', ActionTemplate::SkipShot),
            code(KeyCode::ArrowRight, ActionTemplate::Go),
            code(KeyCode::ArrowLeft, ActionTemplate::Back),
        ];
        for n in 0..=LAST_DIGIT {
            let cue = ActionTemplate::GotoCue(CueIndex(u16::from(n)));
            bindings.extend(KeyCode::digit(n).map(|k| code(k, cue.clone())));
            bindings.extend(KeyCode::numpad(n).map(|k| code(k, cue.clone())));
        }
        bindings.extend([
            Binding::new(
                InputSource::key_with_shift(Key::Char('g'), ShiftRule::Released),
                ActionTemplate::NextDj,
            ),
            Binding::new(
                InputSource::key_with_shift(Key::Char('g'), ShiftRule::Held),
                ActionTemplate::PrevDj,
            ),
            letter('t', ActionTemplate::ShowDj),
            letter('a', ActionTemplate::Ascension),
            letter('e', ActionTemplate::RandomEvent),
            toggle(InputSource::key(Key::Char('x')), ActionTemplate::FlashKill),
            toggle(InputSource::key(Key::Char('b')), ActionTemplate::Breathing),
            toggle(InputSource::key(Key::Char('o')), ActionTemplate::OperatorOverlay),
            toggle(
                InputSource::key_with_shift(Key::Code(KeyCode::Space), ShiftRule::Held),
                ActionTemplate::Playback,
            ),
        ]);
        Self {
            name: SILLAGE_KEYBOARD_NAME.to_owned(),
            bindings,
        }
    }
}
