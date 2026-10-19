//! Débranchement et rebranchement d'une interface pendant l'exécution, simulés par le backend synthétique : la
//! perte est vue tout de suite (`live = false`), l'analyse ne s'arrête pas, la capture rouvre le périphérique par son
//! nom dès qu'il revient, et les features repartent sur la même horloge.

mod common;

use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use cf_analysis::AudioEventKind;
use cf_audio::capture::{CaptureConfig, CaptureState, SampleType, SyntheticDevice};
use common::{format, rig, sine, wait_for};

// `AllocDisabler` n'existe qu'en debug (en release, `assert_no_alloc` est compilé en appel direct).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[test]
fn unplug_then_replug_recovers_without_restarting_analysis() {
    let device = SyntheticDevice::new("Interface USB", format(48_000, 2, SampleType::F32), sine(440.0, 0.5));
    let mut r = rig(device, CaptureConfig::default()).unwrap();
    wait_for("live", Duration::from_secs(3), || r.outputs.features.latest().live);
    thread::sleep(Duration::from_millis(300));

    // Débranchement.
    assert!(r.bus.unplug("Interface USB"));
    let lost_after = wait_for("live = false", Duration::from_secs(1), || {
        !r.outputs.features.latest().live
    });
    let frozen = r.features();
    assert!(
        r.next_event(AudioEventKind::LiveChanged { live: false }, Duration::from_secs(1))
            .is_some()
    );
    wait_for("capture en attente", Duration::from_secs(1), || {
        matches!(r.capture.state(), CaptureState::Waiting { .. })
    });
    // Pendant l'absence : l'analyse tourne toujours, mais l'horloge audio ne progresse pas et les énergies sont au
    // repos ; la capture réessaie sans planter.
    thread::sleep(Duration::from_millis(400));
    let waiting = r.features();
    assert_eq!(waiting.step, frozen.step);
    assert!(!waiting.live);
    assert!(waiting.loud.abs() < f32::EPSILON);
    let CaptureState::Waiting { attempts, .. } = r.capture.state() else {
        unreachable!("la capture doit attendre le périphérique");
    };
    assert!(attempts >= 2, "{attempts} tentatives seulement");

    // Rebranchement.
    assert!(r.bus.replug("Interface USB"));
    let back_after = wait_for("live de nouveau", Duration::from_secs(3), || {
        r.outputs.features.latest().live
    });
    assert!(
        r.next_event(AudioEventKind::LiveChanged { live: true }, Duration::from_secs(1))
            .is_some()
    );
    wait_for("capture connectée", Duration::from_secs(1), || {
        matches!(r.capture.state(), CaptureState::Connected { .. })
    });
    thread::sleep(Duration::from_millis(300));
    let resumed = r.features();
    eprintln!(
        "débranché : live = false après {lost_after:?} ({} tentatives pendant l'absence) ; rebranché : live après \
         {back_after:?} ; pas {} → {}",
        attempts, frozen.step, resumed.step
    );
    assert!(lost_after < Duration::from_millis(100), "perte vue en {lost_after:?}");
    // Même thread, même horloge : les pas reprennent après le dernier pas d'avant la coupure.
    assert!(resumed.step > frozen.step + 20);
    assert!(resumed.live);
    assert!((resumed.loud - 0.5 * std::f32::consts::FRAC_1_SQRT_2).abs() < 0.01);
    // Une reconnexion n'est pas un changement de source : l'analyseur garde sa mémoire (pas de `reset`).
    assert_eq!(r.probe.resets.load(Ordering::Relaxed), 0);
    assert_eq!(r.probe.idles.load(Ordering::Relaxed), 1);
}

#[test]
fn several_unplug_cycles_in_a_row() {
    let device = SyntheticDevice::new("Instable", format(44_100, 2, SampleType::I16), sine(300.0, 0.5));
    let mut r = rig(device, CaptureConfig::default()).unwrap();
    for cycle in 0..3 {
        wait_for("live", Duration::from_secs(3), || r.outputs.features.latest().live);
        let before = r.features().step;
        assert!(r.bus.unplug("Instable"));
        wait_for("live = false", Duration::from_secs(1), || {
            !r.outputs.features.latest().live
        });
        // Rebranché au bout de durées variables, y compris presque aussitôt.
        thread::sleep(Duration::from_millis(20 + 150 * cycle));
        assert!(r.bus.replug("Instable"));
        wait_for("reprise", Duration::from_secs(3), || {
            r.outputs.features.latest().step > before + 5
        });
    }
    assert_eq!(r.analysis.stats().feed_errors, 0);
}
