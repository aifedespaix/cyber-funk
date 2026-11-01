//! Tests du gouverneur sur des séquences d'instantanés écrites à la main.

use super::{
    AFTER_DROP_FALLBACK_S, Decision, DistrictId, DistrictKind, Governor, GovernorSnapshot, MAX_DWELL_S, MIN_DWELL_S,
    StayReason, TRANSITION_MAX_S, TransitionKind,
};
use cf_analysis::Section;

const VARIANTS: [u8; 3] = [6, 7, 8];

fn snap(now_s: f64) -> GovernorSnapshot {
    GovernorSnapshot {
        now_s,
        section: Section::Groove,
        section_changed: false,
        intensity: 0.5,
        bass: 0.5,
        high: 0.5,
        bpm: None,
        drop_imminent: false,
        drop_now: false,
        event_ended: false,
        show_present: true,
    }
}

fn calm(now_s: f64) -> GovernorSnapshot {
    GovernorSnapshot {
        section: Section::Calm,
        ..snap(now_s)
    }
}

fn is_change(d: Decision) -> bool {
    matches!(d, Decision::Change { .. })
}

/// Fait tourner le gouverneur une image par seconde et renvoie les (temps, décision) de changement.
fn run(gov: &mut Governor, secs: u32, make: impl Fn(f64) -> GovernorSnapshot) -> Vec<(f64, DistrictId)> {
    let mut out = Vec::new();
    for s in 0..secs {
        let t = f64::from(s);
        if let Decision::Change { to, .. } = gov.tick(&make(t)) {
            out.push((t, to));
        }
    }
    out
}

#[test]
fn jamais_en_peak() {
    let mut gov = Governor::new(1, VARIANTS);
    let changes = run(&mut gov, 20 * 60, |t| GovernorSnapshot {
        section: Section::Peak,
        ..snap(t)
    });
    assert_eq!(changes.len(), 0);
}

#[test]
fn jamais_avec_drop_imminent() {
    let mut gov = Governor::new(1, VARIANTS);
    let changes = run(&mut gov, 20 * 60, |t| GovernorSnapshot {
        drop_imminent: true,
        ..calm(t)
    });
    assert_eq!(changes.len(), 0);
}

#[test]
fn jamais_apres_un_drop() {
    // À 128 BPM, 8 mesures de 4 temps = 32 temps = 15 s (le brief écrit 30 s par erreur de calcul).
    let mut gov = Governor::new(1, VARIANTS);
    let drop_t = 200.0;
    for s in 0..200 {
        let _ = gov.tick(&GovernorSnapshot {
            bpm: Some(128.0),
            ..snap(f64::from(s))
        });
    }
    let first = (0..200u32)
        .map(|k| {
            let t = drop_t + f64::from(k) * 0.5;
            gov.tick(&GovernorSnapshot {
                drop_now: k == 0,
                bpm: Some(128.0),
                ..calm(t)
            })
        })
        .position(is_change)
        .map(|k| drop_t + k as f64 * 0.5);
    let first = first.expect("un changement finit par arriver");
    assert!(first - drop_t >= 15.0, "changement trop tôt : {first}");
    assert!(first - drop_t < 16.0, "changement trop tard : {first}");

    // Sans BPM : 16 s.
    let mut gov = Governor::new(2, VARIANTS);
    for s in 0..200 {
        let _ = gov.tick(&snap(f64::from(s)));
    }
    let first = (0..100u32)
        .map(|k| {
            gov.tick(&GovernorSnapshot {
                drop_now: k == 0,
                ..calm(drop_t + f64::from(k))
            })
        })
        .position(is_change)
        .map(|k| k as f64)
        .expect("un changement finit par arriver");
    assert!(first >= AFTER_DROP_FALLBACK_S);
    assert!(first < AFTER_DROP_FALLBACK_S + 1.0);
}

#[test]
fn temps_minimum_respecte() {
    let mut gov = Governor::new(3, VARIANTS);
    let changes = run(&mut gov, 200, calm);
    let first = changes.first().expect("un changement").0;
    assert!(first >= MIN_DWELL_S);
    assert!(first < MIN_DWELL_S + 1.0);
    let mut gov = Governor::new(3, VARIANTS);
    assert_eq!(
        gov.tick(&calm(0.0)),
        Decision::Stay {
            reason: StayReason::TooSoon
        }
    );
}

#[test]
fn temps_maximum() {
    let mut gov = Governor::new(4, VARIANTS);
    let changes = run(&mut gov, 600, snap);
    let first = changes.first().expect("un changement").0;
    assert!((MAX_DWELL_S..MAX_DWELL_S + 1.0).contains(&first), "{first}");
    // Avant, la fenêtre reste fermée.
    let mut gov = Governor::new(4, VARIANTS);
    assert_eq!(run(&mut gov, 470, snap).len(), 0, "pas de fenêtre avant le maximum");
}

#[test]
fn jamais_deux_fois_le_meme() {
    let mut gov = Governor::new(5, VARIANTS);
    let mut count = 0;
    let mut t = 0.0;
    let mut last_variant = [Some(0u8), None, None];
    while count < 200 {
        let before = gov.current();
        if let Decision::Change { to, .. } = gov.tick(&calm(t)) {
            assert_ne!(to.kind, before.kind);
            assert_ne!(to, before);
            assert_eq!(gov.current(), to);
            assert_ne!(last_variant[to.kind.index()], Some(to.variant));
            last_variant[to.kind.index()] = Some(to.variant);
            count += 1;
        }
        t += 1.0;
    }
}

#[test]
fn reproductible() {
    let seq = |seed| {
        let mut gov = Governor::new(seed, VARIANTS);
        run(&mut gov, 4 * 3600, calm)
    };
    assert_eq!(seq(42), seq(42));
    assert_ne!(seq(42), seq(43));
}

#[test]
fn sans_show_ne_propose_rien() {
    let mut gov = Governor::new(7, VARIANTS);
    let d = gov.tick(&GovernorSnapshot {
        show_present: false,
        ..calm(1000.0)
    });
    assert_eq!(
        d,
        Decision::Stay {
            reason: StayReason::NoShow
        }
    );
}

#[test]
fn force_remplace_la_transition() {
    let mut gov = Governor::new(8, VARIANTS);
    let a = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 2,
    };
    let b = DistrictId {
        kind: DistrictKind::Residential,
        variant: 1,
    };
    gov.force(a, TransitionKind::Fog, 10.0);
    assert_eq!(gov.current(), a);
    assert_eq!(
        gov.tick(&calm(11.0)),
        Decision::Stay {
            reason: StayReason::Transitioning
        }
    );
    gov.force(b, TransitionKind::Gradient, 12.0);
    assert_eq!(gov.current(), b);
    assert_eq!(gov.history().last(), Some(&b));
    // La nouvelle transition repart de 12 s : encore en cours à 21 s, finie à 22 s.
    assert_eq!(
        gov.tick(&calm(21.0)),
        Decision::Stay {
            reason: StayReason::Transitioning
        }
    );
    assert!(!matches!(
        gov.tick(&calm(22.0)),
        Decision::Stay {
            reason: StayReason::Transitioning
        }
    ));
}

#[test]
fn gel_bloque_l_automatique_pas_le_force() {
    let mut gov = Governor::new(9, VARIANTS);
    gov.set_frozen(true);
    assert_eq!(run(&mut gov, 1000, calm).len(), 0);
    assert_eq!(
        gov.tick(&calm(2000.0)),
        Decision::Stay {
            reason: StayReason::Frozen
        }
    );
    let to = DistrictId {
        kind: DistrictKind::Residential,
        variant: 3,
    };
    gov.force(to, TransitionKind::Gradient, 2001.0);
    assert_eq!(gov.current(), to);
    gov.set_frozen(false);
    assert!(is_change(gov.tick(&calm(3000.0))));
}

#[test]
fn penchant_sonore() {
    let share = |s: GovernorSnapshot| {
        let mut gov = Governor::new(11, VARIANTS);
        let mut counts = [0u32; 3];
        for _ in 0..2000 {
            let kind = gov.pick_kind(&s, &[]).expect("types disponibles");
            counts[kind.index()] += 1;
        }
        counts.map(|c| f64::from(c) / 2000.0)
    };
    let high = share(GovernorSnapshot {
        intensity: 0.9,
        bass: 0.4,
        high: 0.6,
        ..snap(0.0)
    });
    let bass = share(GovernorSnapshot {
        intensity: 0.5,
        bass: 1.0,
        high: 0.0,
        ..snap(0.0)
    });
    let quiet = share(GovernorSnapshot {
        intensity: 0.1,
        bass: 0.3,
        high: 0.5,
        ..snap(0.0)
    });
    for (probs, favored) in [(high, 0), (bass, 1), (quiet, 2)] {
        assert!(probs[favored] > 1.0 / 3.0, "{probs:?}");
        for (i, p) in probs.iter().enumerate() {
            if i != favored {
                assert!(*p >= 0.10, "{probs:?}");
            }
        }
    }
}

#[test]
fn transition_dure_au_plus_dix_secondes() {
    let mut gov = Governor::new(12, VARIANTS);
    let mut t = 0.0;
    let start = loop {
        if is_change(gov.tick(&calm(t))) {
            break t;
        }
        t += 1.0;
    };
    assert_eq!(
        gov.tick(&calm(start + TRANSITION_MAX_S - 1.0)),
        Decision::Stay {
            reason: StayReason::Transitioning
        }
    );
    // Sans `transition_done`, l'état stable revient à `TRANSITION_MAX_S` (ici : trop tôt pour rechanger).
    assert_eq!(
        gov.tick(&calm(start + TRANSITION_MAX_S)),
        Decision::Stay {
            reason: StayReason::TooSoon
        }
    );
}

#[test]
fn transition_done_libere_avant_le_delai() {
    let mut gov = Governor::new(13, VARIANTS);
    gov.force(
        DistrictId {
            kind: DistrictKind::Suburb,
            variant: 0,
        },
        TransitionKind::Gradient,
        0.0,
    );
    gov.transition_done(3.0);
    assert_eq!(
        gov.tick(&calm(4.0)),
        Decision::Stay {
            reason: StayReason::TooSoon
        }
    );
}
