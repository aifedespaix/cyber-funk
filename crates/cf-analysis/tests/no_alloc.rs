//! Aucune allocation par pas (règle 4 de CLAUDE.md, contrat de [`Analyzer::step`]) : un allocateur compteur, propre au
//! thread du test, compte ce que `BeatAnalyzer` demande au tas pendant des milliers de pas, kicks compris.
//!
//! Contrairement à `assert_no_alloc` (inerte en release), ce compteur fonctionne dans tous les profils ; un contrôle
//! positif prouve qu'il voit bien les allocations.

// Le seul `unsafe` du fichier : `GlobalAlloc` est un trait `unsafe` ; l'implémentation délègue telle quelle à `System`.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::AtomicU64;

use cf_analysis::{Analyzer, AudioFeatures, BeatAnalyzer, EventSink, StepContext};
use cf_audio::feed::{HOP, HOP_SECONDS};

thread_local! {
    // `const` : pas d'initialisation paresseuse, donc aucune allocation depuis l'allocateur lui-même.
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

struct CountingAllocator;

fn record() {
    if COUNTING.with(Cell::get) {
        ALLOCATIONS.with(|n| n.set(n.get() + 1));
    }
}

// SAFETY: chaque méthode délègue à `System` avec les mêmes arguments, donc hérite de ses garanties ; le compteur ne
// touche qu'à des `Cell` locales au thread, sans allocation ni accès au bloc.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: `layout` vient de l'appelant, qui respecte le contrat de `GlobalAlloc::alloc`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: idem `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        // SAFETY: `ptr` et `layout` viennent de l'appelant, qui respecte le contrat de `GlobalAlloc::realloc`.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` a été alloué par `System` avec ce `layout` (toutes nos allocations en viennent).
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Nombre d'allocations (et réallocations) faites par ce thread pendant `f`.
fn allocations_during(f: impl FnOnce()) -> u64 {
    ALLOCATIONS.with(|n| n.set(0));
    COUNTING.with(|c| c.set(true));
    f();
    COUNTING.with(|c| c.set(false));
    ALLOCATIONS.with(Cell::get)
}

/// Un pas de musique synthétique : un coup de basse à chaque temps (128 BPM) sur un peu de bruit.
fn fill_hop(hop: &mut [f32; HOP], step: u64) {
    let beat_samples = (60.0 / 128.0 * 48_000.0) as u64;
    for (i, slot) in hop.iter_mut().enumerate() {
        let n = step * HOP as u64 + i as u64;
        let since_kick = (n % beat_samples) as f64 / 48_000.0;
        let kick = 0.8 * (-since_kick / 0.05).exp() * (2.0 * std::f64::consts::PI * 55.0 * since_kick).sin();
        let noise = (((n * 2_654_435_761) >> 8) % 2001) as f64 / 1000.0 - 1.0;
        *slot = (kick + 0.02 * noise) as f32;
    }
}

#[test]
fn counter_sees_allocations() {
    assert!(allocations_during(|| drop(std::hint::black_box(vec![0_u8; 64]))) > 0);
    assert_eq!(allocations_during(|| {}), 0);
}

#[test]
fn beat_analyzer_steps_without_allocating() {
    let (tx, rx) = crossbeam_channel::bounded(16);
    let dropped = AtomicU64::new(0);
    let mut analyzer = BeatAnalyzer::new();
    let mut features = AudioFeatures::NEUTRAL;
    let mut hop = [0.0_f32; HOP];
    let mut kicks = 0_u32;

    let allocations = allocations_during(|| {
        // 3 000 pas : plus de 30 s de musique, des dizaines de kicks, un canal d'événements qui se remplit (les
        // événements en trop sont jetés sans allouer), un passage au repos au milieu.
        for step in 1..=3_000_u64 {
            fill_hop(&mut hop, step);
            let time_s = step as f64 * HOP_SECONDS;
            let ctx = StepContext {
                step,
                time_s,
                dt_s: HOP_SECONDS as f32,
                sample_rate: 48_000,
            };
            let mut sink = EventSink::new(&tx, &dropped, step, time_s);
            analyzer.step(&hop, &ctx, &mut features, &mut sink);
            kicks += u32::from(features.kick);
            if step == 1_500 {
                analyzer.idle(&mut features);
            }
        }
    });
    assert!(
        kicks > 30,
        "le signal doit produire des kicks pour exercer le chemin complet ({kicks})"
    );
    assert!(features.bpm.is_some());
    assert_eq!(allocations, 0, "{allocations} allocation(s) pendant les pas d'analyse");
    drop(rx);
}
