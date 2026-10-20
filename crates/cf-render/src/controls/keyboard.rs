//! Le clavier de Bevy vers `RawInput` : les deux fenêtres (projecteur et opérateur), sauf quand egui écrit du texte.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyCode as BevyKey, KeyboardInput};
use bevy::prelude::*;
use bevy::window::WindowFocused;
use cf_control::{KeyCode as CfKey, KeyEvent, Modifiers, RawInput};

use super::bridge::{ControlBridge, Emitter};
use super::status::{ControlsStatus, LastInput};
use super::{ControlAction, TextInputFocus};

/// Associe chaque touche de Bevy à son homonyme de `cf-control` (mêmes noms, ceux de `KeyboardEvent.code`).
macro_rules! key_map {
    ($input:expr; $($name:ident),+ $(,)?) => {
        match $input {
            $(BevyKey::$name => Some(CfKey::$name),)+
            _ => None,
        }
    };
}

/// L'emplacement physique d'une touche, `None` si `cf-control` ne la connaît pas.
#[must_use]
pub fn key_code(code: BevyKey) -> Option<CfKey> {
    key_map!(code;
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
        NumpadAdd, NumpadSubtract, NumpadMultiply, NumpadDivide, NumpadDecimal, NumpadEnter,
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        ArrowLeft, ArrowRight, ArrowUp, ArrowDown, Home, End, PageUp, PageDown, Insert, Delete,
        Space, Enter, Tab, Backspace, Escape,
        Minus, Equal, BracketLeft, BracketRight, Backslash, Semicolon, Quote, Backquote, Comma, Period, Slash,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    )
}

/// Le caractère produit, selon la disposition du clavier : seulement pour une touche qui donne une seule lettre
/// (Maj+G donne « G », une flèche ou Espace ne donnent rien).
#[must_use]
pub fn logical_char(key: &Key) -> Option<char> {
    let Key::Character(text) = key else {
        return None;
    };
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Some(c),
        _ => None,
    }
}

/// Maj, Ctrl, Alt et Méta tels que `ButtonInput` les connaît à cette image.
#[must_use]
pub fn modifiers(keys: &ButtonInput<BevyKey>) -> Modifiers {
    Modifiers {
        shift: keys.any_pressed([BevyKey::ShiftLeft, BevyKey::ShiftRight]),
        ctrl: keys.any_pressed([BevyKey::ControlLeft, BevyKey::ControlRight]),
        alt: keys.any_pressed([BevyKey::AltLeft, BevyKey::AltRight]),
        meta: keys.any_pressed([BevyKey::SuperLeft, BevyKey::SuperRight]),
    }
}

/// L'événement `cf-control` d'une touche de Bevy.
#[must_use]
pub fn key_event(input: &KeyboardInput, mods: Modifiers) -> KeyEvent {
    KeyEvent {
        ch: logical_char(&input.logical_key),
        code: key_code(input.key_code),
        mods,
        pressed: input.state == ButtonState::Pressed,
        repeat: input.repeat,
    }
}

/// Lit le clavier des deux fenêtres.
///
/// - Quand un champ texte d'egui a le focus (`TextInputFocus`), les **appuis** sont avalés : taper « d » dans un champ
///   ne déclenche pas le drop. Les relâchements, eux, passent toujours : le mapper ignore un relâchement dont il n'a
///   pas vu l'appui, et une touche enfoncée avant que le champ prenne le focus ne reste pas tenue.
/// - Une fenêtre qui perd le focus relâche tout ce qui est tenu (le relâchement se produirait dans l'autre fenêtre).
pub(super) fn read_keyboard(
    mut bridge: ResMut<ControlBridge>,
    mut status: ResMut<ControlsStatus>,
    focus: Res<TextInputFocus>,
    keys: Res<ButtonInput<BevyKey>>,
    mut inputs: MessageReader<KeyboardInput>,
    mut focused: MessageReader<WindowFocused>,
    mut out: MessageWriter<ControlAction>,
) {
    let now = bridge.now_s();
    let mut emitter = Emitter {
        bridge: &mut bridge,
        status: &mut status,
        out: &mut out,
    };
    if focused.read().any(|f| !f.focused) {
        emitter.release_held(now);
    }
    let mods = modifiers(&keys);
    for input in inputs.read() {
        let event = key_event(input, mods);
        if focus.0 && event.pressed {
            continue;
        }
        emitter.status.last_input = Some(LastInput::Key(event));
        emitter.map(&RawInput::key(now, event));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_touches_du_pupitre_ont_leur_equivalent() {
        assert_eq!(key_code(BevyKey::ArrowRight), Some(CfKey::ArrowRight));
        assert_eq!(key_code(BevyKey::Digit3), Some(CfKey::Digit3));
        assert_eq!(key_code(BevyKey::KeyG), Some(CfKey::KeyG));
        assert_eq!(key_code(BevyKey::F12), Some(CfKey::F12));
        assert_eq!(key_code(BevyKey::ShiftLeft), None, "un modificateur n'est pas une commande");
    }

    #[test]
    fn seule_une_lettre_unique_donne_un_caractere() {
        assert_eq!(logical_char(&Key::Character("G".into())), Some('G'));
        assert_eq!(logical_char(&Key::Character("ab".into())), None);
        assert_eq!(logical_char(&Key::ArrowRight), None);
        assert_eq!(logical_char(&Key::Space), None);
    }
}
