//! Profils RON : aller-retour identique, conflits signalés au chargement, profil livré conforme au code.

use cf_control::{
    ActionTemplate, Binding, ControlProfile, CueIndex, DjIndex, Error, InputSource, Key, KeyCode, Layer, MacroIndex,
    Mode, ProfileIssue, RelativeEncoding, ShiftRule, TitleId, TriggerId,
};

/// Un profil qui passe par toutes les formes de source, d'action et de mode.
fn rich_profile() -> ControlProfile {
    let b = |source, action, mode| Binding { source, action, mode };
    let note = |note| InputSource::MidiNote {
        port: Some("DDJ-FLX4".into()),
        channel: 0,
        note,
    };
    let cc = |cc, encoding| InputSource::MidiCc {
        port: None,
        channel: 6,
        cc,
        encoding,
    };
    ControlProfile {
        name: "Contrôleur d'essai « é »".into(),
        bindings: vec![
            b(note(36), ActionTemplate::Go, Mode::Momentary),
            b(note(37), ActionTemplate::Back, Mode::Momentary),
            b(note(38), ActionTemplate::GotoCue(CueIndex(12)), Mode::Momentary),
            b(note(39), ActionTemplate::Hold, Mode::Toggle),
            b(note(40), ActionTemplate::ArmDj(DjIndex(2)), Mode::Momentary),
            b(note(41), ActionTemplate::Title(TitleId::new("merci")), Mode::Momentary),
            b(
                note(42),
                ActionTemplate::Trigger(TriggerId::new("storm")),
                Mode::Momentary,
            ),
            b(note(43), ActionTemplate::Blackout, Mode::Momentary),
            b(note(44), ActionTemplate::SafeLook, Mode::Momentary),
            b(note(45), ActionTemplate::TapTempo, Mode::Momentary),
            b(note(46), ActionTemplate::Resync, Mode::Momentary),
            b(
                cc(1, RelativeEncoding::BinaryOffset),
                ActionTemplate::Macro(MacroIndex(0)),
                Mode::Absolute,
            ),
            b(
                cc(2, RelativeEncoding::TwosComplement),
                ActionTemplate::Macro(MacroIndex(7)),
                Mode::Relative,
            ),
            b(
                cc(3, RelativeEncoding::SignMagnitude),
                ActionTemplate::NudgePhase { beats: -0.125 },
                Mode::Relative,
            ),
            b(
                cc(4, RelativeEncoding::BinaryOffset),
                ActionTemplate::Master,
                Mode::Absolute,
            ),
            b(
                cc(5, RelativeEncoding::BinaryOffset),
                ActionTemplate::LayerFade(Layer::Foreground),
                Mode::Absolute,
            ),
            b(
                cc(6, RelativeEncoding::BinaryOffset),
                ActionTemplate::FlashKill,
                Mode::Absolute,
            ),
            b(
                InputSource::Osc {
                    address: "/cf/go".into(),
                },
                ActionTemplate::Go,
                Mode::Momentary,
            ),
            b(
                InputSource::Key {
                    key: Key::Code(KeyCode::F5),
                    shift: ShiftRule::Released,
                    ctrl: true,
                    alt: true,
                    meta: true,
                },
                ActionTemplate::Breathing,
                Mode::Toggle,
            ),
            b(
                InputSource::key(Key::Char('é')),
                ActionTemplate::NudgePhase { beats: 0.25 },
                Mode::Momentary,
            ),
        ],
    }
}

#[test]
fn profiles_survive_a_ron_round_trip() {
    for profile in [ControlProfile::sillage_keyboard(), rich_profile()] {
        let text = profile.to_ron().expect("sérialisation");
        let back = ControlProfile::from_ron(&text).expect("relecture");
        assert_eq!(back, profile, "{text}");
        // Et le texte réécrit est le même : rien ne dérive d'une sauvegarde à l'autre.
        assert_eq!(back.to_ron().expect("sérialisation"), text);
    }
}

#[test]
fn shipped_keyboard_profile_matches_the_code() {
    let text = include_str!("../../../assets/controllers/keyboard-sillage.ron");
    let shipped = ControlProfile::from_ron(text).expect("le profil livré est valide");
    assert_eq!(shipped, ControlProfile::sillage_keyboard());
}

#[test]
fn hand_written_ron_is_terse() {
    let text = r#"
        ControlProfile(
            name: "X1",
            bindings: [
                Binding(source: MidiNote(channel: 0, note: 36), action: Go, mode: Momentary),
                Binding(source: Key(key: Char('g'), shift: Held), action: PrevDj, mode: Momentary),
                Binding(source: MidiCc(port: Some("X1 mk3"), channel: 0, cc: 21), action: Macro(3), mode: Absolute),
            ],
        )
    "#;
    let profile = ControlProfile::from_ron(text).expect("profil valide");
    assert_eq!(profile.bindings.len(), 3);
    assert_eq!(profile.bindings[2].action, ActionTemplate::Macro(MacroIndex(3)));
}

#[test]
fn a_binding_conflict_is_reported_on_load() {
    let text = r#"
        ControlProfile(
            name: "doublon",
            bindings: [
                Binding(source: MidiNote(port: Some("X1"), channel: 0, note: 36), action: Go, mode: Momentary),
                Binding(source: Key(key: Char('d')), action: ForceDrop, mode: Momentary),
                Binding(source: MidiNote(channel: 0, note: 36), action: Blackout, mode: Momentary),
                Binding(source: Key(key: Char('D'), shift: Held), action: Go, mode: Momentary),
            ],
        )
    "#;
    let Err(Error::InvalidProfile { profile, issues }) = ControlProfile::from_ron(text) else {
        unreachable!("le profil aurait dû être refusé");
    };
    assert_eq!(profile, "doublon");
    assert!(
        matches!(
            issues[0],
            ProfileIssue::Conflict {
                first: 0,
                second: 2,
                ..
            }
        ),
        "{issues:?}"
    );
    assert!(
        matches!(
            issues[1],
            ProfileIssue::Conflict {
                first: 1,
                second: 3,
                ..
            }
        ),
        "{issues:?}"
    );
    assert_eq!(issues.len(), 2);
    // Le message d'erreur nomme les lignes (à partir de 1) et la source.
    let message = Error::InvalidProfile { profile, issues }.to_string();
    assert!(message.contains("lignes 1 et 3"), "{message}");
    assert!(message.contains("note 36 canal 1"), "{message}");
}

#[test]
fn unreadable_ron_is_a_parse_error() {
    assert!(matches!(
        ControlProfile::from_ron("ControlProfile(name: 3)"),
        Err(Error::Parse(_))
    ));
    assert!(matches!(
        ControlProfile::from_ron(
            r#"(name: "x", bindings: [(source: Key(key: Char('a')), action: Fly, mode: Momentary)])"#
        ),
        Err(Error::Parse(_))
    ));
}
