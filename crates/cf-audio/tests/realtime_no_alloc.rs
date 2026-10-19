//! Preuve d'absence d'allocation sur les chemins temps réel de `cf-audio`.
//!
//! Ce binaire de test installe `assert_no_alloc::AllocDisabler` comme allocateur global : toute allocation ou
//! libération dans une section `assert_no_alloc` fait avorter le processus (en debug). Le callback de capture
//! (`FeedWriter::push_interleaved`) s'enveloppe lui-même dans une telle section ; la lecture côté analyse
//! (`FeedReader::next_hop`, `drain`, `reset`) est enveloppée ici. Le test `trap_is_armed` vérifie, dans un processus
//! enfant, que le piège se déclenche vraiment : sans lui, un test vert ne prouverait rien.

use std::time::Instant;

use assert_no_alloc::assert_no_alloc;
use cf_audio::feed::{FeedReader, HOP, HopStatus, feed};
use cf_audio::source::SourceKind;

// `AllocDisabler` n'existe qu'en debug (en release, `assert_no_alloc` est compilé en appel direct).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

/// Variable d'environnement qui transforme `trap_is_armed` en processus enfant fautif.
const CHILD_ENV: &str = "CF_AUDIO_ALLOC_TRAP_CHILD";

#[test]
fn capture_callback_never_allocates_for_any_format() {
    // Toutes les allocations (rings, tampons de test) ont lieu ici, avant les appels temps réel.
    let (mut writer, input) = feed(SourceKind::Capture, 48_000, 4 * HOP).unwrap();
    let i16_block = vec![1_000_i16; 480 * 2];
    let u16_block = vec![40_000_u16; 480 * 2];
    let i32_block = vec![1 << 20; 480 * 6];
    let f32_block = vec![0.25_f32; 441];
    for _ in 0..200 {
        // Le ring déborde très vite (aucun lecteur) : le chemin « plein, on jette et on compte » est couvert aussi.
        writer.push_interleaved(&i16_block, 2);
        writer.push_interleaved(&u16_block, 2);
        writer.push_interleaved(&i32_block, 6);
        writer.push_interleaved(&f32_block, 1);
        writer.push_interleaved::<f32>(&[], 2);
    }
    assert_eq!(input.stats().written(), (4 * HOP) as u64);
    assert!(input.stats().dropped() > 0);
    assert_eq!(input.stats().callbacks(), 1000);
}

fn steady_state_reader(sample_rate: u32) -> Result<(), Box<dyn std::error::Error>> {
    let (mut writer, input) = feed(SourceKind::Capture, sample_rate, 1 << 15)?;
    let mut reader = FeedReader::new(input)?;
    let block = vec![0.1_f32; sample_rate as usize / 100];
    let mut hop = [0.0_f32; HOP];
    let mut hops = 0_u32;
    for _ in 0..300 {
        writer.push_interleaved(&block, 1);
        let now = Instant::now();
        hops += assert_no_alloc(|| {
            let mut ready = 0;
            while let Ok(HopStatus::Ready { .. }) = reader.next_hop(&mut hop, now) {
                ready += 1;
            }
            ready
        });
    }
    // Trois secondes d'entrée ≈ 281 pas à 48 kHz (moins le délai du rééchantillonneur).
    assert!(hops > 270, "{hops} pas seulement");
    writer.push_interleaved(&block, 1);
    let now = Instant::now();
    assert_no_alloc(|| {
        reader.drain(now);
        reader.reset();
    });
    Ok(())
}

#[test]
fn analysis_side_reader_never_allocates_at_48k() {
    steady_state_reader(48_000).unwrap();
}

#[test]
fn analysis_side_reader_never_allocates_while_resampling() {
    steady_state_reader(44_100).unwrap();
    steady_state_reader(96_000).unwrap();
}

/// Le piège doit être armé : un processus enfant qui alloue dans une section interdite doit avorter.
#[cfg(debug_assertions)]
#[test]
fn trap_is_armed() {
    if std::env::var_os(CHILD_ENV).is_some() {
        assert_no_alloc(|| std::hint::black_box(Vec::<u8>::with_capacity(64)).capacity());
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
    assert!(String::from_utf8_lossy(&output.stderr).contains("memory allocation of 64 bytes failed"));
}
