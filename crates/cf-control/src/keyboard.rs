//! Clavier abstrait, sans winit ni Bevy.
//!
//! Comme dans Sillage (`engine/show/operator.ts`), une touche se reconnaît de deux façons :
//! - par **caractère** ([`Key::Char`]) pour les lettres : on suit la disposition du clavier, donc le D d'un clavier
//!   AZERTY déclenche bien « drop » (c'est `event.key` côté web, la touche logique côté Bevy) ;
//! - par **code physique** ([`Key::Code`]) pour les touches sans lettre (chiffres, flèches, Espace) : sur un AZERTY,
//!   la rangée des chiffres produit `&é"'(` sans Maj, seul l'emplacement est fiable (c'est `event.code`, le
//!   `KeyCode` de Bevy).
//!
//! Le pont Bevy (J3-06) remplit un [`KeyEvent`] avec les deux informations ; le binding choisit laquelle il regarde.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Emplacement physique d'une touche, nommé comme dans la spécification W3C « UI Events `KeyboardEvent.code` »
/// (mêmes noms que `KeyCode` de Bevy/winit, la conversion du pont est donc directe). Seules les touches utiles à un
/// pupitre figurent ici ; une touche absente arrive avec `code: None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum KeyCode {
    // Rangée des chiffres.
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,
    // Pavé numérique.
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
    // Lettres, par emplacement QWERTY (indépendant de la disposition, rarement voulu : préférer `Key::Char`).
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
    KeyG,
    KeyH,
    KeyI,
    KeyJ,
    KeyK,
    KeyL,
    KeyM,
    KeyN,
    KeyO,
    KeyP,
    KeyQ,
    KeyR,
    KeyS,
    KeyT,
    KeyU,
    KeyV,
    KeyW,
    KeyX,
    KeyY,
    KeyZ,
    // Navigation et édition.
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    Space,
    Enter,
    Tab,
    Backspace,
    Escape,
    // Ponctuation (emplacements QWERTY).
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Backquote,
    Comma,
    Period,
    Slash,
    // Fonctions.
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

impl KeyCode {
    /// La touche de la rangée des chiffres pour `n` (0 à 9), ou `None` au-delà.
    #[must_use]
    pub fn digit(n: u8) -> Option<Self> {
        const DIGITS: [KeyCode; 10] = [
            KeyCode::Digit0,
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ];
        DIGITS.get(usize::from(n)).copied()
    }

    /// La touche du pavé numérique pour `n` (0 à 9), ou `None` au-delà.
    #[must_use]
    pub fn numpad(n: u8) -> Option<Self> {
        const NUMPAD: [KeyCode; 10] = [
            KeyCode::Numpad0,
            KeyCode::Numpad1,
            KeyCode::Numpad2,
            KeyCode::Numpad3,
            KeyCode::Numpad4,
            KeyCode::Numpad5,
            KeyCode::Numpad6,
            KeyCode::Numpad7,
            KeyCode::Numpad8,
            KeyCode::Numpad9,
        ];
        NUMPAD.get(usize::from(n)).copied()
    }
}

/// Ce qu'un binding clavier regarde : le caractère produit (lettres) ou l'emplacement physique (le reste).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    /// Caractère produit par la touche, comparé sans tenir compte de la casse (Maj+G donne « G », qui reste un g).
    Char(char),
    /// Emplacement physique.
    Code(KeyCode),
}

impl Key {
    /// Les deux touches désignent-elles la même chose ? Un caractère et un code ne se comparent pas (il faudrait
    /// connaître la disposition) : `false`.
    pub(crate) fn same_as(self, other: Self) -> bool {
        match (self, other) {
            (Self::Char(a), Self::Char(b)) => fold_case(a) == fold_case(b),
            (Self::Code(a), Self::Code(b)) => a == b,
            _ => false,
        }
    }

    /// La touche correspond-elle à cet appui ?
    pub(crate) fn matches(self, event: &KeyEvent) -> bool {
        match self {
            Self::Char(c) => event.ch.is_some_and(|e| fold_case(e) == fold_case(c)),
            Self::Code(code) => event.code == Some(code),
        }
    }
}

/// Minuscule d'un caractère (`'G'` → `'g'`) ; un caractère dont la minuscule tient en plusieurs caractères reste tel
/// quel, ce qui suffit à comparer deux touches.
fn fold_case(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

/// Exigence d'un binding sur la touche Maj. Sillage : « Maj exigée (true) ou interdite (false) ; absent :
/// indifférent ». Ctrl, Alt et Méta, eux, doivent correspondre exactement (voir [`Modifiers`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum ShiftRule {
    /// Maj indifférente (D et Maj+D déclenchent tous deux le drop, comme dans Sillage).
    #[default]
    Any,
    /// Maj exigée (Maj+G : DJ précédent).
    Held,
    /// Maj interdite (G seul : DJ suivant).
    Released,
}

impl ShiftRule {
    pub(crate) fn accepts(self, shift: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Held => shift,
            Self::Released => !shift,
        }
    }

    /// Existe-t-il un état de Maj accepté par les deux règles ?
    pub(crate) fn overlaps(self, other: Self) -> bool {
        self.accepts(true) && other.accepts(true) || self.accepts(false) && other.accepts(false)
    }

    #[allow(clippy::trivially_copy_pass_by_ref)] // signature imposée par `skip_serializing_if`
    pub(crate) fn is_any(&self) -> bool {
        *self == Self::Any
    }
}

/// Touches de modification enfoncées au moment de l'appui.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // quatre touches indépendantes : un bool chacune est le modèle exact
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Touche Windows / Commande.
    pub meta: bool,
}

/// Un appui ou un relâchement de touche, tel que le pont Bevy le transmet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// Caractère produit selon la disposition (`event.key` d'une seule lettre), `None` pour une touche nommée
    /// (flèches, Espace, Échap…).
    pub ch: Option<char>,
    /// Emplacement physique, `None` si la touche n'a pas d'équivalent dans [`KeyCode`].
    pub code: Option<KeyCode>,
    pub mods: Modifiers,
    /// `true` à l'appui, `false` au relâchement.
    pub pressed: bool,
    /// Répétition automatique d'une touche tenue : ignorée par le mapper, comme dans Sillage (`!event.repeat`).
    pub repeat: bool,
}

impl KeyEvent {
    /// Appui d'une lettre (sans répétition), pratique pour les tests et le pont.
    #[must_use]
    pub fn char_press(ch: char, mods: Modifiers) -> Self {
        Self {
            ch: Some(ch),
            code: None,
            mods,
            pressed: true,
            repeat: false,
        }
    }

    /// Appui d'une touche repérée par son code (sans caractère, sans répétition).
    #[must_use]
    pub fn code_press(code: KeyCode, mods: Modifiers) -> Self {
        Self {
            ch: None,
            code: Some(code),
            mods,
            pressed: true,
            repeat: false,
        }
    }

    /// Le même événement, en relâchement.
    #[must_use]
    pub fn released(self) -> Self {
        Self {
            pressed: false,
            repeat: false,
            ..self
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Char(c) => write!(f, "{}", c.to_uppercase()),
            Self::Code(code) => write!(f, "{code:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chars_compare_without_case() {
        assert!(Key::Char('g').same_as(Key::Char('G')));
        assert!(Key::Char('d').matches(&KeyEvent::char_press('D', Modifiers::default())));
        assert!(!Key::Char('d').same_as(Key::Code(KeyCode::KeyD)));
    }

    #[test]
    fn shift_rules_overlap() {
        assert!(ShiftRule::Any.overlaps(ShiftRule::Held));
        assert!(ShiftRule::Any.overlaps(ShiftRule::Released));
        assert!(!ShiftRule::Held.overlaps(ShiftRule::Released));
        assert!(ShiftRule::Held.overlaps(ShiftRule::Held));
    }

    #[test]
    fn digit_helpers_stop_at_nine() {
        assert_eq!(KeyCode::digit(3), Some(KeyCode::Digit3));
        assert_eq!(KeyCode::numpad(9), Some(KeyCode::Numpad9));
        assert_eq!(KeyCode::digit(10), None);
    }
}
