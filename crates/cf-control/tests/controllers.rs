//! Les profils de contrôleurs livrés dans `assets/controllers/` : lisibles, valides, sans conflit, et qui font ce que
//! leurs commentaires disent.

use std::path::PathBuf;

use cf_control::{
    Action, ActionTemplate, ControlProfile, InputSource, KeyEvent, Layer, Level, MacroIndex, Mapper, Modifiers, PortId,
    RawEvent, RawInput, Result, Switch,
};

fn controllers_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/controllers")
}

type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

fn generic() -> Result<ControlProfile> {
    ControlProfile::from_ron(include_str!("../../../assets/controllers/generic-8knobs-8pads.ron"))
}

#[test]
fn every_shipped_profile_loads_validates_and_round_trips() -> TestResult {
    let mut seen = 0;
    for entry in std::fs::read_dir(controllers_dir())? {
        let path = entry?.path();
        if path.extension().is_none_or(|e| e != "ron") {
            continue;
        }
        let profile = ControlProfile::from_ron(&std::fs::read_to_string(&path)?)
            .map_err(|e| format!("{} : {e}", path.display()))?;
        let rewritten = profile.to_ron()?;
        assert_eq!(ControlProfile::from_ron(&rewritten)?, profile, "{}", path.display());
        seen += 1;
    }
    assert!(seen >= 2, "clavier + générique au moins, {seen} trouvé(s)");
    Ok(())
}

#[test]
fn generic_profile_has_eight_knobs_and_eight_pads() -> Result<()> {
    let profile = generic()?;
    let knobs = profile
        .bindings
        .iter()
        .filter(|b| matches!(b.source, InputSource::MidiCc { .. }))
        .count();
    let pads = profile
        .bindings
        .iter()
        .filter(|b| matches!(b.source, InputSource::MidiNote { .. }))
        .count();
    assert_eq!((knobs, pads), (8, 8));
    Ok(())
}

fn midi(event: RawEvent) -> RawInput {
    RawInput { time_s: 0.0, event }
}

#[test]
fn generic_profile_drives_the_show_next_to_the_keyboard() -> Result<()> {
    let keyboard = ControlProfile::sillage_keyboard();
    let generic = generic()?;
    // Clavier en bas de la pile, contrôleur au-dessus : aucun geste partagé, donc aucun remplacement.
    let (mut mapper, overrides) = Mapper::from_profiles(&[&keyboard, &generic])?;
    assert!(overrides.is_empty(), "{overrides:?}");
    let port = PortId(0);
    mapper.set_port_name(port, Some("MPK mini 3:MPK mini 3 MIDI 1 20:0"));

    let pad = |note, velocity| {
        midi(RawEvent::MidiNote {
            port,
            channel: 0,
            note,
            velocity,
        })
    };
    let knob = |cc, value| {
        midi(RawEvent::MidiCc {
            port,
            channel: 0,
            cc,
            value,
        })
    };
    let act = |mapper: &mut Mapper, input| mapper.map(&input).map(|t| t.action);

    // GO / BACK : la démo de fin de J3.
    assert_eq!(act(&mut mapper, pad(36, 100)), Some(Action::Go));
    assert_eq!(
        act(&mut mapper, pad(36, 0)),
        None,
        "le relâchement d'un GO ne fait rien"
    );
    assert_eq!(act(&mut mapper, pad(37, 100)), Some(Action::Back));
    // Le noir se tient : appui = allumé, relâchement = éteint.
    assert_eq!(act(&mut mapper, pad(40, 100)), Some(Action::Blackout(Switch::On)));
    assert_eq!(act(&mut mapper, pad(40, 0)), Some(Action::Blackout(Switch::Off)));
    // Hold est une bascule.
    assert_eq!(act(&mut mapper, pad(42, 100)), Some(Action::Hold(Switch::Toggle)));
    // Les potards tournent les macros et l'intensité.
    assert_eq!(
        act(&mut mapper, knob(72, 127)),
        Some(Action::Macro {
            index: MacroIndex(2),
            value: Level::Set(1.0)
        })
    );
    assert_eq!(act(&mut mapper, knob(77, 0)), Some(Action::Master(Level::Set(0.0))));
    assert_eq!(
        act(&mut mapper, knob(76, 127)),
        Some(Action::LayerFade {
            layer: Layer::Overlay,
            value: Level::Set(1.0)
        })
    );
    // Le clavier marche toujours à côté.
    let drop = mapper.map(&RawInput::key(0.0, KeyEvent::char_press('d', Modifiers::default())));
    assert_eq!(drop.map(|t| t.action), Some(Action::ForceDrop));
    Ok(())
}

#[test]
fn generic_profile_uses_every_macro_slot_it_declares_exactly_once() -> Result<()> {
    let mut macros: Vec<u8> = generic()?
        .bindings
        .iter()
        .filter_map(|b| match b.action {
            ActionTemplate::Macro(MacroIndex(i)) => Some(i),
            _ => None,
        })
        .collect();
    macros.sort_unstable();
    assert_eq!(macros, [0, 1, 2, 3, 4, 5]);
    Ok(())
}
