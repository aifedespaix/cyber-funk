//! Preuve d'absence d'allocation dans le callback MIDI.
//!
//! Ce binaire de test installe `assert_no_alloc::AllocDisabler` comme allocateur global : toute allocation ou
//! libération dans une section `assert_no_alloc` fait avorter le processus (en debug). `InputCallback::handle`
//! s'enveloppe lui-même dans une telle section ; ici on le fait travailler par l'hôte factice, sur toutes les formes de
//! messages, y compris canal plein. Le test `trap_is_armed` vérifie, dans un processus enfant, que le piège se
//! déclenche vraiment : sans lui, un test vert ne prouverait rien.

use std::time::{Duration, Instant};

use cf_control::{FakeMidi, MidiConfig, MidiInput, MidiItem, PortEvent};

// `AllocDisabler` n'existe qu'en debug (en release, `assert_no_alloc` est compilé en appel direct).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Variable d'environnement qui transforme `trap_is_armed` en processus enfant fautif.
const CHILD_ENV: &str = "CF_CONTROL_ALLOC_TRAP_CHILD";

/// Une collection de paquets : un par type de message, plus running status, `SysEx` long, horloge, octets parasites.
fn packets() -> Vec<Vec<u8>> {
    let mut sysex = vec![0xF0, 0x7E, 0x7F, 0x06, 0x01];
    sysex.extend((0..300).map(|i| (i % 128) as u8));
    sysex.push(0xF7);
    vec![
        vec![0x90, 36, 100],
        vec![0x80, 36, 0],
        vec![0x90, 36, 0],
        vec![0xB0, 1, 64],
        vec![0xB0, 33, 12, 34, 13, 35, 14],
        vec![0xC0, 5],
        vec![0xA0, 60, 10],
        vec![0xD0, 10],
        vec![0xE0, 0, 64],
        vec![0xF2, 0x10, 0x01],
        vec![0xF3, 2],
        vec![0xF8],
        vec![0xFA, 0xFB, 0xFC, 0xFF, 0xFE],
        vec![0x90, 0xF8, 40, 0xF8, 90],
        sysex,
        vec![60, 61, 62],
        vec![],
    ]
}

fn wait_for_port(input: &MidiInput) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match input.try_recv() {
            Some(MidiItem::Port(PortEvent::Connected { .. })) => return,
            Some(_) => {}
            None => {
                assert!(Instant::now() < deadline, "port jamais ouvert");
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

#[test]
fn midi_callback_never_allocates() {
    // Toutes les allocations (paquets, canal, thread) ont lieu ici, avant les appels temps réel.
    let host = FakeMidi::new();
    host.plug("Pad A");
    let input = MidiInput::start(
        host.backend(),
        MidiConfig {
            queue_capacity: 32,
            ..MidiConfig::default()
        },
    )
    .unwrap();
    wait_for_port(&input);
    let packets = packets();

    // Chemin nominal : le canal est vidé entre les tours.
    for _ in 0..50 {
        for packet in &packets {
            assert!(host.send("Pad A", packet));
        }
        while input.try_recv().is_some() {}
    }
    let delivered = input.stats().delivered();
    assert!(delivered >= 50 * 20, "{delivered} messages livrés");

    // Chemin « canal plein » : personne ne lit, on jette et on compte, sans allouer.
    for _ in 0..50 {
        for packet in &packets {
            assert!(host.send("Pad A", packet));
        }
    }
    assert!(
        input.stats().dropped() > 0,
        "le canal de 32 éléments aurait dû déborder"
    );
}

/// Le piège doit être armé : un processus enfant qui alloue dans une section interdite doit avorter.
#[cfg(debug_assertions)]
#[test]
fn trap_is_armed() {
    if std::env::var_os(CHILD_ENV).is_some() {
        assert_no_alloc::assert_no_alloc(|| std::hint::black_box(Vec::<u8>::with_capacity(64)).capacity());
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "trap_is_armed", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "l'enfant aurait dû avorter : {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
