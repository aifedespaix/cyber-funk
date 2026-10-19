//! Chaque raccourci du mode cinéma de Sillage (`engine/show/operator.ts`, `OPERATOR_BINDINGS` et `matchCinemaKey`)
//! produit l'action attendue avec le profil clavier par défaut, y compris sur un clavier AZERTY.

use cf_control::{
    Action, ControlProfile, CueIndex, KeyCode, KeyEvent, Mapper, Modifiers, RawInput, Result, Switch, TimedAction,
};

const NONE: Modifiers = Modifiers {
    shift: false,
    ctrl: false,
    alt: false,
    meta: false,
};
const SHIFT: Modifiers = Modifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
};

/// Le mapper chargé du seul profil par défaut (aucun remplacement possible avec un seul profil).
fn keyboard() -> Result<Mapper> {
    let profile = ControlProfile::sillage_keyboard();
    Mapper::from_profiles(&[&profile]).map(|(mapper, _)| mapper)
}

/// Une touche telle que le pont Bevy la transmettrait : caractère selon la disposition, et code physique.
fn press(ch: Option<char>, code: Option<KeyCode>, mods: Modifiers) -> KeyEvent {
    KeyEvent {
        ch,
        code,
        mods,
        pressed: true,
        repeat: false,
    }
}

fn action(mapper: &mut Mapper, event: KeyEvent) -> Option<Action> {
    mapper.map(&RawInput::key(1.0, event)).map(|a: TimedAction| a.action)
}

#[test]
fn every_sillage_shortcut_maps_to_its_action() -> Result<()> {
    let mut m = keyboard()?;
    // (touche telle que reçue sur un QWERTY, action attendue, commande Sillage correspondante)
    let cases: Vec<(KeyEvent, Action, &str)> = vec![
        (
            press(Some('d'), Some(KeyCode::KeyD), NONE),
            Action::ForceDrop,
            "forceDrop",
        ),
        (
            press(Some('n'), Some(KeyCode::KeyN), NONE),
            Action::SkipShot,
            "skipShot",
        ),
        (press(None, Some(KeyCode::ArrowRight), NONE), Action::Go, "nextLevel"),
        (press(None, Some(KeyCode::ArrowLeft), NONE), Action::Back, "prevLevel"),
        (press(Some('g'), Some(KeyCode::KeyG), NONE), Action::NextDj, "nextDj"),
        (press(Some('G'), Some(KeyCode::KeyG), SHIFT), Action::PrevDj, "prevDj"),
        (press(Some('t'), Some(KeyCode::KeyT), NONE), Action::ShowDj, "showDj"),
        (
            press(Some('a'), Some(KeyCode::KeyA), NONE),
            Action::Ascension,
            "ascension",
        ),
        (
            press(Some('e'), Some(KeyCode::KeyE), NONE),
            Action::RandomEvent,
            "randomEvent",
        ),
        (
            press(Some('x'), Some(KeyCode::KeyX), NONE),
            Action::FlashKill(Switch::Toggle),
            "toggleFlashes",
        ),
        (
            press(Some('b'), Some(KeyCode::KeyB), NONE),
            Action::Breathing(Switch::Toggle),
            "toggleBreathing",
        ),
        (
            press(Some('o'), Some(KeyCode::KeyO), NONE),
            Action::OperatorOverlay(Switch::Toggle),
            "toggleOperatorHud",
        ),
        (
            press(None, Some(KeyCode::Space), SHIFT),
            Action::Playback(Switch::Toggle),
            "togglePlay",
        ),
    ];
    for (event, expected, sillage) in cases {
        assert_eq!(action(&mut m, event), Some(expected), "{sillage}");
    }
    Ok(())
}

#[test]
fn digits_go_to_cues_from_both_rows() -> Result<()> {
    let mut m = keyboard()?;
    for n in 0..=9u8 {
        let expected = Some(Action::GotoCue(CueIndex(u16::from(n))));
        let digit = KeyCode::digit(n).expect("chiffre");
        let numpad = KeyCode::numpad(n).expect("chiffre");
        assert_eq!(action(&mut m, press(None, Some(digit), NONE)), expected, "Digit{n}");
        assert_eq!(action(&mut m, press(None, Some(numpad), NONE)), expected, "Numpad{n}");
    }
    Ok(())
}

#[test]
fn azerty_letters_follow_the_layout_and_digits_follow_the_position() -> Result<()> {
    let mut m = keyboard()?;
    // AZERTY : la touche physique Q (code KeyQ) produit « a » → Ascension ; la touche physique A produit « q ».
    assert_eq!(
        action(&mut m, press(Some('a'), Some(KeyCode::KeyQ), NONE)),
        Some(Action::Ascension)
    );
    assert_eq!(action(&mut m, press(Some('q'), Some(KeyCode::KeyA), NONE)), None);
    // AZERTY : la rangée des chiffres produit « & é " ' ( » sans Maj ; seul l'emplacement compte.
    assert_eq!(
        action(&mut m, press(Some('&'), Some(KeyCode::Digit1), NONE)),
        Some(Action::GotoCue(CueIndex(1)))
    );
    assert_eq!(
        action(&mut m, press(Some('('), Some(KeyCode::Digit5), NONE)),
        Some(Action::GotoCue(CueIndex(5)))
    );
    Ok(())
}

#[test]
fn shift_is_indifferent_unless_the_binding_says_otherwise() -> Result<()> {
    let mut m = keyboard()?;
    // Sillage : D et Maj+D déclenchent tous deux le drop.
    assert_eq!(
        action(&mut m, press(Some('D'), Some(KeyCode::KeyD), SHIFT)),
        Some(Action::ForceDrop)
    );
    // Espace sans Maj ne fait rien (couper la musique par erreur gâcherait le set).
    assert_eq!(action(&mut m, press(None, Some(KeyCode::Space), NONE)), None);
    Ok(())
}

#[test]
fn system_shortcuts_repeats_releases_and_escape_do_nothing() -> Result<()> {
    let mut m = keyboard()?;
    let ctrl = Modifiers { ctrl: true, ..NONE };
    let alt = Modifiers { alt: true, ..NONE };
    let meta = Modifiers { meta: true, ..NONE };
    for mods in [ctrl, alt, meta] {
        assert_eq!(
            action(&mut m, press(Some('d'), Some(KeyCode::KeyD), mods)),
            None,
            "{mods:?}+D"
        );
    }
    let repeat = KeyEvent {
        repeat: true,
        ..press(Some('d'), Some(KeyCode::KeyD), NONE)
    };
    assert_eq!(action(&mut m, repeat), None);
    assert_eq!(
        action(&mut m, press(Some('d'), Some(KeyCode::KeyD), NONE).released()),
        None
    );
    assert_eq!(
        action(&mut m, press(Some('x'), Some(KeyCode::KeyX), NONE).released()),
        None
    );
    // Échap ne quitte jamais seul (J0-02) : il n'est pas lié.
    assert_eq!(action(&mut m, press(None, Some(KeyCode::Escape), NONE)), None);
    Ok(())
}

#[test]
fn the_action_keeps_the_input_timestamp() -> Result<()> {
    let mut m = keyboard()?;
    let out = m.map(&RawInput::key(42.125, KeyEvent::char_press('n', NONE)));
    assert_eq!(
        out,
        Some(TimedAction {
            time_s: 42.125,
            action: Action::SkipShot
        })
    );
    Ok(())
}
