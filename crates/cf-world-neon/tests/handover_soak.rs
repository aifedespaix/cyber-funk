//! Endurance de la séquence : 50 cycles Démarrer → trois passations → Arrêter doivent revenir exactement à l'état
//! d'attente, sans dérive ni phase bloquée (règle de fiabilité du direct).

#![allow(clippy::expect_used, clippy::panic)]

use cf_render::{Phase, RunOrder};
use cf_world_neon::run::{RunOutput, RunSequencer};

const DT: f32 = 1.0 / 30.0;
/// Garde-fou : aucune phase ne dure plus de ce nombre d'images (3 minutes).
const MAX_FRAMES: u32 = 30 * 180;

fn step(seq: &mut RunSequencer, order: Option<RunOrder>) -> RunOutput {
    let out = seq.update(order, DT, false);
    for v in [
        out.stages.road,
        out.stages.build,
        out.stages.gantry,
        out.double.presence,
        out.progress,
    ] {
        assert!(
            v.is_finite() && (-0.001..=1.001).contains(&v),
            "étape hors bornes : {v}"
        );
    }
    out
}

fn until(seq: &mut RunSequencer, phase: Phase) {
    for _ in 0..MAX_FRAMES {
        if step(seq, None).phase == phase {
            return;
        }
    }
    panic!("phase {phase:?} jamais atteinte");
}

#[test]
fn fifty_cycles_return_to_the_exact_waiting_state() {
    let mut seq = RunSequencer::new();
    seq.set_slot_info(0, 4);
    let reference = step(&mut seq, None);
    assert_eq!(reference.phase, Phase::Waiting);
    for cycle in 0..50 {
        step(&mut seq, Some(RunOrder::Start));
        until(&mut seq, Phase::Running);
        for slot in 1..=3 {
            seq.set_slot_info(slot - 1, 4);
            step(&mut seq, Some(RunOrder::Handover { slot: None }));
            assert_eq!(seq.phase(), Phase::Handover, "cycle {cycle}, DJ {slot}");
            until(&mut seq, Phase::Running);
        }
        step(&mut seq, Some(RunOrder::Stop));
        until(&mut seq, Phase::Waiting);
        seq.set_slot_info(0, 4);
        let after = step(&mut seq, None);
        assert_eq!(after, reference, "dérive au cycle {cycle}");
    }
}

#[test]
fn a_handover_order_is_refused_outside_the_running_phase() {
    let mut seq = RunSequencer::new();
    seq.set_slot_info(0, 4);
    step(&mut seq, Some(RunOrder::Handover { slot: Some(2) }));
    assert_eq!(seq.phase(), Phase::Waiting);
    step(&mut seq, Some(RunOrder::Start));
    step(&mut seq, Some(RunOrder::Handover { slot: Some(2) }));
    assert_eq!(seq.phase(), Phase::Starting);
}
