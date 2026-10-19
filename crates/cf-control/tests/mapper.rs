//! Le mapper : modes de lecture, ports MIDI par nom, fusion de profils.

use cf_control::{
    Action, ActionTemplate, Binding, ControlProfile, InputSource, Key, KeyEvent, Level, MacroIndex, Mapper, Mode,
    Modifiers, PortId, RELATIVE_LEVEL_STEP, RawEvent, RawInput, RelativeEncoding, Result, Switch,
};

const X1: PortId = PortId(0);
const FLX4: PortId = PortId(1);

fn profile(name: &str, bindings: Vec<Binding>) -> ControlProfile {
    ControlProfile {
        name: name.into(),
        bindings,
    }
}

fn note_src(port: Option<&str>, note: u8) -> InputSource {
    InputSource::MidiNote {
        port: port.map(str::to_owned),
        channel: 0,
        note,
    }
}

fn cc_src(cc: u8, encoding: RelativeEncoding) -> InputSource {
    InputSource::MidiCc {
        port: None,
        channel: 0,
        cc,
        encoding,
    }
}

fn note(port: PortId, note: u8, velocity: u8) -> RawInput {
    RawInput {
        time_s: 0.0,
        event: RawEvent::MidiNote {
            port,
            channel: 0,
            note,
            velocity,
        },
    }
}

fn cc(cc: u8, value: u8) -> RawInput {
    RawInput {
        time_s: 0.0,
        event: RawEvent::MidiCc {
            port: X1,
            channel: 0,
            cc,
            value,
        },
    }
}

fn osc(address: &str, value: Option<f32>) -> RawInput {
    RawInput {
        time_s: 0.0,
        event: RawEvent::Osc {
            address: address.into(),
            value,
        },
    }
}

/// Toutes les actions produites par la suite d'entrées.
fn run(mapper: &mut Mapper, inputs: &[RawInput]) -> Vec<Action> {
    inputs.iter().filter_map(|i| mapper.map(i)).map(|t| t.action).collect()
}

fn mapper_with(bindings: Vec<Binding>) -> Result<Mapper> {
    let p = profile("test", bindings);
    let (mut mapper, _) = Mapper::from_profiles(&[&p])?;
    mapper.set_port_name(X1, Some("X1"));
    mapper.set_port_name(FLX4, Some("DDJ-FLX4"));
    Ok(mapper)
}

#[test]
fn momentary_and_toggle_switches() -> Result<()> {
    let b = |n, mode| Binding {
        source: note_src(None, n),
        action: ActionTemplate::Blackout,
        mode,
    };
    let mut m = mapper_with(vec![b(1, Mode::Momentary), b(2, Mode::Toggle)])?;
    let out = run(
        &mut m,
        &[note(X1, 1, 100), note(X1, 1, 0), note(X1, 2, 100), note(X1, 2, 0)],
    );
    assert_eq!(
        out,
        [
            Action::Blackout(Switch::On),
            Action::Blackout(Switch::Off),
            Action::Blackout(Switch::Toggle)
        ]
    );
    Ok(())
}

#[test]
fn absolute_and_relative_levels() -> Result<()> {
    let mut m = mapper_with(vec![
        Binding {
            source: cc_src(1, RelativeEncoding::default()),
            action: ActionTemplate::Master,
            mode: Mode::Absolute,
        },
        Binding {
            source: cc_src(2, RelativeEncoding::BinaryOffset),
            action: ActionTemplate::Macro(MacroIndex(3)),
            mode: Mode::Relative,
        },
    ])?;
    let out = run(&mut m, &[cc(1, 0), cc(1, 127), cc(2, 66), cc(2, 64), cc(2, 63)]);
    let macro3 = |value| Action::Macro {
        index: MacroIndex(3),
        value,
    };
    assert_eq!(
        out,
        [
            Action::Master(Level::Set(0.0)),
            Action::Master(Level::Set(1.0)),
            macro3(Level::Shift(2.0 * RELATIVE_LEVEL_STEP)),
            // 64 : encodeur immobile, rien.
            macro3(Level::Shift(-RELATIVE_LEVEL_STEP)),
        ]
    );
    Ok(())
}

#[test]
fn cc_buttons_react_to_edges_only() -> Result<()> {
    let mut m = mapper_with(vec![Binding::new(
        cc_src(7, RelativeEncoding::default()),
        ActionTemplate::Go,
    )])?;
    // Repos (0) au départ : pas un relâchement. 127 répété : un seul GO.
    let out = run(&mut m, &[cc(7, 0), cc(7, 127), cc(7, 127), cc(7, 0), cc(7, 127)]);
    assert_eq!(out, [Action::Go, Action::Go]);
    Ok(())
}

#[test]
fn a_fader_drives_a_switch_on_threshold_crossings() -> Result<()> {
    let mut m = mapper_with(vec![Binding {
        source: cc_src(8, RelativeEncoding::default()),
        action: ActionTemplate::FlashKill,
        mode: Mode::Absolute,
    }])?;
    let out = run(&mut m, &[cc(8, 10), cc(8, 70), cc(8, 90), cc(8, 30), cc(8, 20)]);
    assert_eq!(out, [Action::FlashKill(Switch::On), Action::FlashKill(Switch::Off)]);
    Ok(())
}

#[test]
fn nudge_steps_scale_with_encoder_ticks() -> Result<()> {
    let mut m = mapper_with(vec![
        Binding {
            source: cc_src(9, RelativeEncoding::TwosComplement),
            action: ActionTemplate::NudgePhase { beats: 0.5 },
            mode: Mode::Relative,
        },
        Binding::new(
            InputSource::key(Key::Char(',')),
            ActionTemplate::NudgePhase { beats: -0.25 },
        ),
    ])?;
    let out = run(
        &mut m,
        &[
            cc(9, 127),
            cc(9, 3),
            RawInput::key(0.0, KeyEvent::char_press(',', Modifiers::default())),
        ],
    );
    assert_eq!(
        out,
        [
            Action::NudgePhase { beats: -0.5 },
            Action::NudgePhase { beats: 1.5 },
            Action::NudgePhase { beats: -0.25 }
        ]
    );
    Ok(())
}

#[test]
fn osc_impulses_buttons_and_faders() -> Result<()> {
    let mut m = mapper_with(vec![
        Binding::new(
            InputSource::Osc {
                address: "/cf/go".into(),
            },
            ActionTemplate::Go,
        ),
        Binding::new(
            InputSource::Osc {
                address: "/cf/master".into(),
            },
            ActionTemplate::Master,
        ),
        Binding {
            source: InputSource::Osc {
                address: "/cf/hold".into(),
            },
            action: ActionTemplate::Hold,
            mode: Mode::Momentary,
        },
    ])?;
    let out = run(
        &mut m,
        &[
            osc("/cf/go", None),
            osc("/cf/master", Some(0.25)),
            osc("/cf/master", Some(3.0)),
            osc("/cf/master", Some(f32::NAN)),
            osc("/cf/hold", Some(1.0)),
            osc("/cf/hold", Some(0.0)),
            osc("/cf/inconnue", None),
        ],
    );
    assert_eq!(
        out,
        [
            Action::Go,
            Action::Master(Level::Set(0.25)),
            Action::Master(Level::Set(1.0)),
            Action::Hold(Switch::On),
            Action::Hold(Switch::Off),
        ]
    );
    Ok(())
}

#[test]
fn midi_ports_are_matched_by_name() -> Result<()> {
    let mut m = mapper_with(vec![
        Binding::new(note_src(Some("X1"), 36), ActionTemplate::Go),
        Binding::new(note_src(Some("DDJ-FLX4"), 36), ActionTemplate::Back),
        Binding::new(note_src(None, 40), ActionTemplate::SafeLook),
    ])?;
    assert_eq!(
        run(&mut m, &[note(X1, 36, 127), note(FLX4, 36, 127)]),
        [Action::Go, Action::Back]
    );
    // Port inconnu : seuls les bindings sans port réagissent.
    assert_eq!(
        run(&mut m, &[note(PortId(7), 36, 127), note(PortId(7), 40, 127)]),
        [Action::SafeLook]
    );
    // Rebranchement : le X1 revient sous un autre numéro, ses bindings le suivent.
    m.set_port_name(X1, None);
    m.set_port_name(PortId(5), Some("X1"));
    assert_eq!(
        run(&mut m, &[note(X1, 36, 127), note(PortId(5), 36, 127)]),
        [Action::Go]
    );
    Ok(())
}

#[test]
fn higher_profiles_override_shared_gestures_and_report_it() -> Result<()> {
    let keyboard = ControlProfile::sillage_keyboard();
    let event = profile(
        "Événement",
        vec![
            Binding::new(note_src(None, 36), ActionTemplate::Go),
            Binding::new(InputSource::key(Key::Char('d')), ActionTemplate::Blackout),
        ],
    );
    let slot = profile(
        "Créneau KRMN",
        vec![Binding::new(note_src(Some("DDJ-FLX4"), 36), ActionTemplate::ForceDrop)],
    );
    let (mut m, overrides) = Mapper::from_profiles(&[&keyboard, &event, &slot])?;
    m.set_port_name(FLX4, Some("DDJ-FLX4"));

    assert_eq!(overrides.len(), 2, "{overrides:?}");
    assert_eq!(overrides[0].replaced, ActionTemplate::ForceDrop);
    assert_eq!(overrides[0].replaced_profile, keyboard.name);
    assert_eq!(overrides[0].by, ActionTemplate::Blackout);
    assert_eq!(overrides[1].replaced, ActionTemplate::Go);
    assert_eq!(overrides[1].by_profile, "Créneau KRMN");

    let d = RawInput::key(0.0, KeyEvent::char_press('d', Modifiers::default()));
    assert_eq!(
        run(&mut m, &[d, note(FLX4, 36, 127)]),
        [Action::Blackout(Switch::Toggle), Action::ForceDrop]
    );
    // Le reste du clavier par défaut est toujours là.
    let n = RawInput::key(0.0, KeyEvent::char_press('n', Modifiers::default()));
    assert_eq!(run(&mut m, &[n]), [Action::SkipShot]);
    Ok(())
}

#[test]
fn a_rejected_load_keeps_the_current_table() -> Result<()> {
    let mut m = mapper_with(vec![Binding::new(note_src(None, 1), ActionTemplate::Go)])?;
    let broken = profile(
        "cassé",
        vec![
            Binding::new(note_src(None, 2), ActionTemplate::Go),
            Binding::new(note_src(None, 2), ActionTemplate::Back),
        ],
    );
    assert!(m.load(&[&broken]).is_err());
    assert_eq!(run(&mut m, &[note(X1, 1, 127)]), [Action::Go]);
    Ok(())
}
