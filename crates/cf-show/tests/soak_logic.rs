//! J9-01 : simulation logique accélérée de 6 h (sans rendu) de la démo, avec des actions aléatoires réalistes
//! (GO, retours, passations, drops forcés, pauses) et des changements de musique.
//!
//! Vérifie que le chef du spectacle reste cohérent sur la durée : index toujours valides, temps fini et monotone,
//! file d'événements vidée à chaque image (donc bornée), aucun blocage.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use cf_control::{Action, Switch};
use cf_show::load::LoadedShow;
use director_rig::{DT, Music, Rig, assets, library};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

const HOURS: f64 = 6.0;

fn demo() -> LoadedShow {
    LoadedShow::load(&assets().join("shows/demo.ron"), &library()).unwrap_or_else(|e| panic!("{e}"))
}

/// Une action « d'opérateur » tirée au sort, pondérée comme un vrai set (surtout des GO, rarement une passation).
fn random_action(rng: &mut Pcg32) -> Option<Action> {
    match rng.next_u32() % 100 {
        0..=9 => Some(Action::Go),
        10..=12 => Some(Action::Back),
        13..=15 => Some(Action::ForceDrop),
        16 => Some(Action::NextDj),
        17 => Some(Action::PrevDj),
        18 => Some(Action::SkipShot),
        19 => Some(Action::Hold(Switch::Toggle)),
        _ => None,
    }
}

#[test]
fn six_hours_of_random_operation_stay_coherent() {
    let show = demo();
    let slot_cues: Vec<usize> = show.event().slots.iter().map(|s| s.cues.len()).collect();
    let mut rig = Rig::with_seed(&show, Music::Kicks(124.0), 42);
    let mut rng = Pcg32::seed_from_u64(0x00C0_FFEE);

    let frames = (HOURS * 3600.0 / f64::from(DT)) as u64;
    let mut last_elapsed = 0.0_f64;
    let mut max_events_per_frame = 0_usize;
    let mut handovers = 0_u32;
    let mut last_slot_seq = rig.state().slot_seq;

    for frame in 0..frames {
        // Toutes les ~2 s, une chance d'action ; toutes les 10 min, la musique change (kicks, break, silence).
        if frame % 120 == 0
            && let Some(action) = random_action(&mut rng)
        {
            rig.act(action);
        }
        if frame % 36_000 == 0 {
            rig.music = match rng.next_u32() % 4 {
                0 => Music::Silent,
                1 => Music::Breakdown,
                2 => Music::Kicks(100.0 + (rng.next_u32() % 50) as f32),
                _ => Music::Kicks(124.0),
            };
        }

        rig.frame();

        max_events_per_frame = max_events_per_frame.max(rig.events.len());
        rig.events.clear();

        let state = rig.state();
        assert!(
            state.slot < slot_cues.len(),
            "image {frame} : créneau {} hors limites",
            state.slot
        );
        assert!(
            state.cue < slot_cues[state.slot],
            "image {frame} : cue {} hors limites ({} cues)",
            state.cue,
            slot_cues[state.slot]
        );
        assert!(
            state.elapsed.is_finite() && state.cue_elapsed.is_finite(),
            "image {frame} : temps non fini"
        );
        // `elapsed` compte depuis le chargement du créneau : il repart à zéro à chaque passation, jamais ailleurs.
        if state.slot_seq == last_slot_seq {
            assert!(
                state.elapsed >= last_elapsed,
                "image {frame} : le temps du créneau recule"
            );
        } else {
            handovers += 1;
            last_slot_seq = state.slot_seq;
        }
        last_elapsed = state.elapsed;
        assert!(
            state.elapsed <= rig.now + 1.0,
            "image {frame} : le créneau est plus vieux que le show"
        );
    }

    assert!(
        rig.now > HOURS * 3600.0 * 0.99,
        "l'horloge du show a avancé de {:.0} s",
        rig.now
    );
    assert!(handovers > 0, "au moins une passation sur 6 h");
    assert!(
        max_events_per_frame < 64,
        "{max_events_per_frame} événements en une image : la file gonfle"
    );
}
