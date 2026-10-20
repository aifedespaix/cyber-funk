//! Des octets MIDI aux actions : analyseur, mapper, CC 14 bits, Program Change, noms de ports souples, MIDI learn.
//! Tout passe octet par octet, sans matériel.

use cf_control::{
    Action, ActionTemplate, Binding, ControlProfile, Dispatch, Error, InputSource, Layer, Learner, Level, MacroIndex,
    Mapper, MidiEvent, MidiItem, MidiParser, Mode, PortEvent, PortId, ProfileIssue, RawInput, RelativeEncoding, Result,
    Switch, dispatch,
};

const PORT: PortId = PortId(4);
const ALSA_NAME: &str = "DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0";

/// Ce que fait le callback : octets → messages → événements horodatés.
fn events(bytes: &[u8]) -> Vec<MidiEvent> {
    let mut parser = MidiParser::new();
    let mut out = Vec::new();
    parser.parse(bytes, |message| {
        out.push(MidiEvent {
            time_s: 3.0,
            port: PORT,
            message,
        });
    });
    out
}

fn raws(bytes: &[u8]) -> Vec<RawInput> {
    events(bytes).iter().filter_map(MidiEvent::to_raw).collect()
}

fn mapper_with(bindings: Vec<Binding>) -> Result<Mapper> {
    let profile = ControlProfile {
        name: "test".into(),
        bindings,
    };
    let (mut mapper, _) = Mapper::from_profiles(&[&profile])?;
    mapper.set_port_name(PORT, Some(ALSA_NAME));
    Ok(mapper)
}

fn run(mapper: &mut Mapper, bytes: &[u8]) -> Vec<Action> {
    raws(bytes)
        .iter()
        .filter_map(|r| mapper.map(r))
        .map(|t| t.action)
        .collect()
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

fn master_level(action: &Action) -> f32 {
    match action {
        Action::Master(Level::Set(v)) => *v,
        other => unreachable!("pas un Master absolu : {other:?}"),
    }
}

fn note_binding(port: Option<&str>, note: u8, action: ActionTemplate, mode: Mode) -> Binding {
    Binding {
        source: InputSource::MidiNote {
            port: port.map(str::to_owned),
            channel: 0,
            note,
        },
        action,
        mode,
    }
}

#[test]
fn pad_press_and_both_kinds_of_release_drive_a_held_switch() -> Result<()> {
    let mut m = mapper_with(vec![note_binding(None, 36, ActionTemplate::Blackout, Mode::Momentary)])?;
    // Appui, Note Off, appui, Note On vélocité 0.
    let out = run(&mut m, &[0x90, 36, 100, 0x80, 36, 0, 0x90, 36, 90, 0x90, 36, 0]);
    assert_eq!(
        out,
        [
            Action::Blackout(Switch::On),
            Action::Blackout(Switch::Off),
            Action::Blackout(Switch::On),
            Action::Blackout(Switch::Off)
        ]
    );
    Ok(())
}

#[test]
fn running_status_pad_hits_reach_the_mapper() -> Result<()> {
    let mut m = mapper_with(vec![
        note_binding(None, 36, ActionTemplate::Go, Mode::Momentary),
        note_binding(None, 37, ActionTemplate::Back, Mode::Momentary),
    ])?;
    // Un seul statut pour quatre messages : 36 appui, 36 relâché (vélocité 0), 37 appui, 37 relâché.
    assert_eq!(
        run(&mut m, &[0x90, 36, 100, 36, 0, 37, 100, 37, 0]),
        [Action::Go, Action::Back]
    );
    Ok(())
}

#[test]
fn clock_bytes_between_cc_bytes_do_not_disturb_a_knob() -> Result<()> {
    let mut m = mapper_with(vec![Binding {
        source: InputSource::MidiCc {
            port: None,
            channel: 0,
            cc: 7,
            encoding: RelativeEncoding::default(),
        },
        action: ActionTemplate::Master,
        mode: Mode::Absolute,
    }])?;
    let out = run(&mut m, &[0xB0, 7, 0xF8, 127]);
    assert_eq!(out, [Action::Master(Level::Set(1.0))]);
    Ok(())
}

fn wide_master() -> Binding {
    Binding {
        source: InputSource::MidiCc14 {
            port: None,
            channel: 0,
            cc: 1,
        },
        action: ActionTemplate::Master,
        mode: Mode::Absolute,
    }
}

#[test]
fn fourteen_bit_cc_combines_both_halves() -> Result<()> {
    let mut m = mapper_with(vec![wide_master()])?;
    // MSB 64 (LSB inconnu = 0) : 8192/16383 ; puis LSB 127 : 8319/16383.
    let out = run(&mut m, &[0xB0, 1, 64, 0xB0, 33, 127]);
    assert_eq!(out.len(), 2);
    assert!(near(master_level(&out[0]), 8192.0 / 16383.0));
    assert!(near(master_level(&out[1]), 8319.0 / 16383.0));
    // Plein : MSB 127 puis LSB 127 = 1.0 exactement.
    let out = run(&mut m, &[0xB0, 1, 127, 33, 127]);
    assert!(near(master_level(out.last().unwrap()), 1.0));
    // Une valeur inchangée n'émet rien.
    assert_eq!(run(&mut m, &[0xB0, 33, 127]).len(), 0);
    Ok(())
}

#[test]
fn fourteen_bit_cc_accepts_lsb_first_and_ignores_other_controllers() -> Result<()> {
    let mut m = mapper_with(vec![wide_master()])?;
    let out = run(&mut m, &[0xB0, 33, 5, 1, 2]);
    assert_eq!(out.len(), 2);
    assert!(near(master_level(&out[1]), f32::from((2_u16 << 7) | 5) / 16383.0));
    // CC 2 et CC 34 ne sont pas la paire du CC 1.
    assert_eq!(run(&mut m, &[0xB0, 2, 99, 34, 99]).len(), 0);
    Ok(())
}

#[test]
fn program_change_triggers_impulses_and_toggles() -> Result<()> {
    let program = |n, action, mode| Binding {
        source: InputSource::MidiProgram {
            port: None,
            channel: 2,
            program: n,
        },
        action,
        mode,
    };
    let mut m = mapper_with(vec![
        program(0, ActionTemplate::NextDj, Mode::Momentary),
        program(1, ActionTemplate::Hold, Mode::Toggle),
    ])?;
    // Canal 3 (index 2) ; le troisième message est en statut courant ; le dernier est sur un autre canal.
    let out = run(&mut m, &[0xC2, 0, 1, 0, 0xC3, 0]);
    assert_eq!(out, [Action::NextDj, Action::Hold(Switch::Toggle), Action::NextDj]);
    Ok(())
}

#[test]
fn a_named_binding_follows_the_controller_across_host_renamings() -> Result<()> {
    // Nom court écrit à la main ; le port s'appelle autrement selon l'hôte.
    let mut m = mapper_with(vec![note_binding(
        Some("DDJ-FLX4"),
        36,
        ActionTemplate::Go,
        Mode::Momentary,
    )])?;
    assert_eq!(run(&mut m, &[0x90, 36, 100]), [Action::Go]);
    m.set_port_name(PORT, Some("DDJ-FLX4:DDJ-FLX4 MIDI 1 32:0"));
    assert_eq!(run(&mut m, &[0x90, 36, 100]), [Action::Go]);
    m.set_port_name(PORT, Some("4- DDJ-FLX4"));
    assert_eq!(run(&mut m, &[0x90, 36, 100]), [Action::Go]);
    // Un autre contrôleur sur ce port : rien.
    m.set_port_name(PORT, Some("MPK mini 3:MPK mini 3 MIDI 1 20:0"));
    assert_eq!(run(&mut m, &[0x90, 36, 100]).len(), 0);
    // Port inconnu : un binding nommé n'est jamais déclenché au hasard.
    m.set_port_name(PORT, None);
    assert_eq!(run(&mut m, &[0x90, 36, 100]).len(), 0);
    Ok(())
}

#[test]
fn wide_cc_conflicts_with_the_seven_bit_cc_on_its_lsb_number() {
    let profile = ControlProfile {
        name: "conflit".into(),
        bindings: vec![
            wide_master(),
            Binding::new(
                InputSource::MidiCc {
                    port: None,
                    channel: 0,
                    cc: 33,
                    encoding: RelativeEncoding::default(),
                },
                ActionTemplate::Macro(MacroIndex(0)),
            ),
        ],
    };
    assert!(matches!(
        profile.issues().as_slice(),
        [ProfileIssue::Conflict {
            first: 0,
            second: 1,
            ..
        }]
    ));
}

#[test]
fn malformed_wide_and_program_bindings_are_refused() {
    let wide = |cc, mode| Binding {
        source: InputSource::MidiCc14 {
            port: None,
            channel: 0,
            cc,
        },
        action: ActionTemplate::Master,
        mode,
    };
    let program = |n, action, mode| Binding {
        source: InputSource::MidiProgram {
            port: None,
            channel: 0,
            program: n,
        },
        action,
        mode,
    };
    let profile = ControlProfile {
        name: "mal formé".into(),
        bindings: vec![
            wide(32, Mode::Absolute),
            wide(5, Mode::Relative),
            program(1, ActionTemplate::Blackout, Mode::Momentary),
            program(2, ActionTemplate::Master, Mode::Absolute),
            program(3, ActionTemplate::Go, Mode::Momentary),
        ],
    };
    let issues = profile.issues();
    assert_eq!(
        issues,
        [
            ProfileIssue::WideCc { binding: 0 },
            ProfileIssue::WideCc { binding: 1 },
            ProfileIssue::ProgramHasNoRelease {
                binding: 2,
                mode: Mode::Momentary
            },
            ProfileIssue::ProgramHasNoRelease {
                binding: 3,
                mode: Mode::Absolute
            },
        ]
    );
    assert!(matches!(profile.validate(), Err(Error::InvalidProfile { .. })));
}

// --- MIDI learn, de bout en bout par `dispatch` ---

fn feed(bytes: &[u8], mapper: &mut Mapper, learner: &mut Learner, first_connect: Option<&str>) -> Vec<Dispatch> {
    let mut out = Vec::new();
    if let Some(name) = first_connect {
        let connected = MidiItem::Port(PortEvent::Connected {
            port: PORT,
            name: name.into(),
        });
        out.push(dispatch(connected, mapper, learner));
    }
    for event in events(bytes) {
        out.push(dispatch(MidiItem::Message(event), mapper, learner));
    }
    out
}

#[test]
fn learning_a_pad_names_the_port_without_host_numbers_and_triggers_nothing() -> Result<()> {
    let mut profile = ControlProfile::new("appris");
    let (mut mapper, _) = Mapper::from_profiles(&[&profile])?;
    let mut learner = Learner::new();
    learner.start_learn(ActionTemplate::ForceDrop);

    let out = feed(
        &[0x99, 38, 120, 0x99, 38, 0],
        &mut mapper,
        &mut learner,
        Some(ALSA_NAME),
    );
    let learned = out
        .iter()
        .find_map(|d| match d {
            Dispatch::Learned(b) => Some(b.clone()),
            _ => None,
        })
        .expect("un binding appris");
    assert_eq!(
        learned.source,
        InputSource::MidiNote {
            port: Some("DDJ-FLX4:DDJ-FLX4 MIDI 1".into()),
            channel: 9,
            note: 38
        }
    );
    // Aucune action pendant l'écoute ; le relâchement qui suit n'en produit pas non plus.
    assert!(!out.iter().any(|d| matches!(d, Dispatch::Action(_))));
    assert!(!learner.is_learning());

    // Le binding rangé dans le profil et rechargé fonctionne… même après un rebranchement sous d'autres numéros.
    assert_eq!(profile.set_binding(learned).len(), 0);
    mapper.load(&[&profile])?;
    mapper.set_port_name(PORT, Some("DDJ-FLX4:DDJ-FLX4 MIDI 1 32:0"));
    let out = feed(&[0x99, 38, 100], &mut mapper, &mut learner, None);
    assert!(matches!(&out[..], [Dispatch::Action(t)] if t.action == Action::ForceDrop));
    Ok(())
}

#[test]
fn learning_a_fourteen_bit_fader_and_replacing_an_existing_binding() -> Result<()> {
    let mut profile = ControlProfile::new("appris");
    profile.bindings.push(Binding {
        source: InputSource::MidiCc {
            port: None,
            channel: 0,
            cc: 33,
            encoding: RelativeEncoding::default(),
        },
        action: ActionTemplate::Macro(MacroIndex(1)),
        mode: Mode::Absolute,
    });
    let (mut mapper, _) = Mapper::from_profiles(&[&profile])?;
    let mut learner = Learner::new();
    learner.start_learn(ActionTemplate::LayerFade(Layer::Overlay));
    // MSB puis LSB du même fader.
    let out = feed(&[0xB0, 1, 64, 0xB0, 33, 5], &mut mapper, &mut learner, Some(ALSA_NAME));
    let learned = out
        .into_iter()
        .find_map(|d| match d {
            Dispatch::Learned(b) => Some(b),
            _ => None,
        })
        .expect("un binding appris");
    assert!(matches!(learned.source, InputSource::MidiCc14 { cc: 1, .. }));
    // Le CC 33 était déjà pris par une macro 7 bits : le profil le remplace et le dit.
    let replaced = profile.set_binding(learned);
    assert_eq!(replaced.len(), 1);
    assert_eq!(replaced[0].action, ActionTemplate::Macro(MacroIndex(1)));
    assert!(profile.validate().is_ok());
    Ok(())
}

#[test]
fn transport_messages_are_passed_through_not_swallowed() -> Result<()> {
    let (mut mapper, _) = Mapper::from_profiles(&[])?;
    let mut learner = Learner::new();
    learner.start_learn(ActionTemplate::Go);
    let out = feed(&[0xF8, 0xFA], &mut mapper, &mut learner, None);
    assert!(out.iter().all(|d| matches!(d, Dispatch::Other(_))), "{out:?}");
    assert_eq!(out.len(), 2);
    assert!(learner.is_learning(), "l'horloge ne conclut pas l'apprentissage");
    Ok(())
}
