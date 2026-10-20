//! Rebranchement à chaud, sélection des ports et ordre du flux, sur un hôte MIDI factice (aucun matériel).
//!
//! Le scénario de référence de la fiche J3-02 est `unplugged_controller_is_picked_up_again_by_name`.

use std::time::{Duration, Instant};

use cf_control::{
    Action, ActionTemplate, Binding, ControlProfile, Dispatch, FakeMidi, InputSource, Learner, Mapper, MidiConfig,
    MidiInput, MidiItem, MidiMessage, PortEvent, PortSelection, dispatch,
};

const FLX4_BEFORE: &str = "DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0";
/// Même contrôleur, rebranché : ALSA lui a donné d'autres numéros.
const FLX4_AFTER: &str = "DDJ-FLX4:DDJ-FLX4 MIDI 1 32:0";
const NOTE_ON_36: [u8; 3] = [0x90, 36, 100];
const NOTE_OFF_36: [u8; 3] = [0x80, 36, 0];

fn fast_config() -> MidiConfig {
    MidiConfig {
        scan_period: Duration::from_millis(10),
        ..MidiConfig::default()
    }
}

/// Lit le flux jusqu'à ce que `done` soit vrai de ce qu'on a lu (au plus 5 s) ; rend tout ce qui est sorti.
fn read_until(input: &MidiInput, done: impl Fn(&[MidiItem]) -> bool) -> Vec<MidiItem> {
    let mut items = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done(&items) {
        assert!(Instant::now() < deadline, "délai dépassé, lu : {items:?}");
        match input.try_recv() {
            Some(item) => items.push(item),
            None => std::thread::sleep(Duration::from_millis(2)),
        }
    }
    items
}

fn connected_names(items: &[MidiItem]) -> Vec<&str> {
    items
        .iter()
        .filter_map(|i| match i {
            MidiItem::Port(PortEvent::Connected { name, .. }) => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

fn disconnected_count(items: &[MidiItem]) -> usize {
    items
        .iter()
        .filter(|i| matches!(i, MidiItem::Port(PortEvent::Disconnected { .. })))
        .count()
}

/// Un profil qui désigne le contrôleur par son nom court, comme le ferait un profil écrit à la main.
fn go_on_flx4() -> ControlProfile {
    let mut profile = ControlProfile::new("FLX4");
    profile.bindings.push(Binding::new(
        InputSource::MidiNote {
            port: Some("DDJ-FLX4".into()),
            channel: 0,
            note: 36,
        },
        ActionTemplate::Go,
    ));
    profile
}

/// Envoie les octets et rend les actions produites une fois le flux relu et passé au mapper.
fn send_and_map(host: &FakeMidi, input: &MidiInput, mapper: &mut Mapper, port: &str, bytes: &[u8]) -> Vec<Action> {
    let before = input.stats().delivered();
    assert!(host.send(port, bytes), "aucune connexion ouverte sur {port}");
    let wanted = before + 1;
    let mut actions = Vec::new();
    let mut learner = Learner::new();
    let mut seen = before;
    let deadline = Instant::now() + Duration::from_secs(5);
    while seen < wanted {
        assert!(Instant::now() < deadline, "message non reçu");
        match input.try_recv() {
            Some(item) => {
                if matches!(item, MidiItem::Message(_)) {
                    seen += 1;
                }
                if let Dispatch::Action(timed) = dispatch(item, mapper, &mut learner) {
                    actions.push(timed.action);
                }
            }
            None => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    actions
}

#[test]
fn unplugged_controller_is_picked_up_again_by_name() {
    let host = FakeMidi::new();
    host.plug(FLX4_BEFORE);
    let input = MidiInput::start(host.backend(), fast_config()).unwrap();
    let profile = go_on_flx4();
    let (mut mapper, _) = Mapper::from_profiles(&[&profile]).unwrap();

    // 1. Le contrôleur est là au démarrage : annoncé, puis ses messages passent.
    let first = read_until(&input, |items| !connected_names(items).is_empty());
    assert_eq!(connected_names(&first), [FLX4_BEFORE]);
    let mut learner = Learner::new();
    for item in first {
        dispatch(item, &mut mapper, &mut learner);
    }
    assert_eq!(
        send_and_map(&host, &input, &mut mapper, FLX4_BEFORE, &NOTE_ON_36),
        [Action::Go]
    );
    assert_eq!(input.ports().connected.len(), 1);

    // 2. Débranché : le port disparaît de la liste, le superviseur ferme la connexion et l'annonce.
    host.unplug(FLX4_BEFORE);
    let gone = read_until(&input, |items| disconnected_count(items) == 1);
    for item in gone {
        dispatch(item, &mut mapper, &mut learner);
    }
    assert!(host.open_ports().is_empty(), "connexion fermée");
    assert!(!host.send(FLX4_BEFORE, &NOTE_ON_36), "plus personne n'écoute");
    assert_eq!(input.ports().connected.len(), 0);

    // 3. Rebranché sous d'autres numéros : rouvert sans redémarrer, avec un nouveau numéro de port, et le profil écrit
    //    avec le nom court reconnaît encore le contrôleur.
    host.plug(FLX4_AFTER);
    let back = read_until(&input, |items| !connected_names(items).is_empty());
    assert_eq!(connected_names(&back), [FLX4_AFTER]);
    for item in back {
        dispatch(item, &mut mapper, &mut learner);
    }
    assert_eq!(host.open_ports(), [FLX4_AFTER]);
    assert_eq!(
        send_and_map(&host, &input, &mut mapper, FLX4_AFTER, &NOTE_ON_36),
        [Action::Go]
    );
}

#[test]
fn port_is_announced_before_its_first_message_and_closed_after_its_last() {
    let host = FakeMidi::new();
    host.plug("Pad A");
    let input = MidiInput::start(host.backend(), fast_config()).unwrap();
    let items = read_until(&input, |items| !connected_names(items).is_empty());
    assert!(matches!(items[0], MidiItem::Port(PortEvent::Connected { .. })));

    assert!(host.send("Pad A", &NOTE_ON_36));
    assert!(host.send("Pad A", &NOTE_OFF_36));
    host.unplug("Pad A");
    let items = read_until(&input, |items| disconnected_count(items) == 1);
    // Les deux messages précèdent l'annonce de fermeture : aucun relâchement n'est perdu par l'ordre du flux.
    assert!(matches!(items[0], MidiItem::Message(_)));
    assert!(matches!(items[1], MidiItem::Message(_)));
    assert!(matches!(items[2], MidiItem::Port(PortEvent::Disconnected { .. })));
    let MidiItem::Message(first) = &items[0] else {
        unreachable!()
    };
    assert_eq!(
        first.message,
        MidiMessage::NoteOn {
            channel: 0,
            note: 36,
            velocity: 100
        }
    );
}

#[test]
fn a_refused_open_is_retried_on_the_next_scan() {
    let host = FakeMidi::new();
    host.refuse_next_connects(3);
    host.plug("Busy Pad");
    let input = MidiInput::start(host.backend(), fast_config()).unwrap();
    let items = read_until(&input, |items| !connected_names(items).is_empty());
    assert_eq!(connected_names(&items), ["Busy Pad"]);
    assert!(host.connect_attempts() >= 4, "{} essais", host.connect_attempts());
}

#[test]
fn a_failing_host_keeps_working_connections_and_recovers() {
    let host = FakeMidi::new();
    host.plug("Pad A");
    let input = MidiInput::start(host.backend(), fast_config()).unwrap();
    read_until(&input, |items| !connected_names(items).is_empty());

    host.set_host_down(true);
    input.rescan();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(host.open_ports(), ["Pad A"], "une énumération en échec ne coupe rien");

    host.set_host_down(false);
    host.plug("Pad B");
    let items = read_until(&input, |items| connected_names(items).contains(&"Pad B"));
    assert_eq!(connected_names(&items), ["Pad B"]);
}

#[test]
fn all_ports_skips_the_alsa_loopback_and_opens_the_others() {
    let host = FakeMidi::new();
    host.plug("Midi Through:Midi Through Port-0 14:0");
    host.plug("MPK mini 3:MPK mini 3 MIDI 1 20:0");
    host.plug("DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0");
    let input = MidiInput::start(host.backend(), fast_config()).unwrap();
    let items = read_until(&input, |items| connected_names(items).len() == 2);
    assert_eq!(
        connected_names(&items),
        ["MPK mini 3:MPK mini 3 MIDI 1 20:0", "DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0"]
    );
    assert_eq!(input.ports().available.len(), 3);
}

#[test]
fn named_selection_opens_only_the_wanted_ports_and_prefers_exact_names() {
    let host = FakeMidi::new();
    host.plug("Traktor Kontrol X1 MK2");
    host.plug("X1");
    host.plug("DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0");
    let config = MidiConfig {
        selection: PortSelection::Named(vec!["x1".into()]),
        ..fast_config()
    };
    let input = MidiInput::start(host.backend(), config).unwrap();
    read_until(&input, |items| !connected_names(items).is_empty());
    assert_eq!(
        host.open_ports(),
        ["X1"],
        "le nom exact l'emporte sur le nom qui le contient"
    );

    // Changer de sélection ferme ce qui n'est plus voulu et ouvre le reste, sans redémarrer.
    input.select(PortSelection::Named(vec!["DDJ-FLX4".into()]));
    let items = read_until(&input, |items| {
        disconnected_count(items) == 1 && !connected_names(items).is_empty()
    });
    assert_eq!(connected_names(&items), ["DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0"]);
    assert_eq!(host.open_ports(), ["DDJ-FLX4:DDJ-FLX4 MIDI 1 28:0"]);
}

#[test]
fn dropping_the_input_closes_every_port() {
    let host = FakeMidi::new();
    host.plug("Pad A");
    host.plug("Pad B");
    let input = MidiInput::start(host.backend(), fast_config()).unwrap();
    read_until(&input, |items| connected_names(items).len() == 2);
    drop(input);
    assert_eq!(host.open_ports().len(), 0);
}
