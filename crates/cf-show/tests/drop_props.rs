//! Propriétés de J2-04, quelles que soient la musique, les portes et les réglages de l'opérateur :
//!
//! - `drop_now` est vrai exactement sur les images où un drop (réel ou forcé) est signalé, et le plan est lisible alors ;
//! - le paquet d'effets ne porte aucun flash quand les flashs sont coupés, et reste dans ses plages ;
//! - plans et cascades choisis respectent toujours les portes ; un titre n'apparaît que sur un drop, à partir du
//!   niveau 2 ;
//! - même graine et mêmes entrées : mêmes décisions.

#![allow(
    clippy::unwrap_used,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::path::PathBuf;

use cf_show::drop::{DropConductor, DropInput, DropPlan, DropTitles, TitleLanguage};
use cf_show::freshness::Freshness;
use cf_show::gates::open_gates;
use cf_show::model::{ShotId, ShotTier, ShowGates};
use proptest::prelude::*;
use rand_core::SeedableRng;
use rand_pcg::Pcg32;

const SHOTS: [(&str, &str); 7] = [
    ("chase", "common"),
    ("side-track", "common"),
    ("bumper", "common"),
    ("orbit", "common"),
    ("wheel", "uncommon"),
    ("flyby", "uncommon"),
    ("strobe", "uncommon"),
];

/// Un tronçon de musique : intensité qui glisse de `from` à `to`, réglages constants, drop signalé à la dernière image.
#[derive(Debug, Clone)]
struct Segment {
    from: f32,
    to: f32,
    frames: u32,
    real_drop: bool,
    forced: bool,
    flashes: bool,
    heat: f32,
    level: u8,
    language: TitleLanguage,
    dt: f32,
}

fn segment() -> impl Strategy<Value = Segment> {
    (
        // Les plages f32 démarrent à 0 puis se décalent (proptest 1.12 panique par intermittence sur certaines plages
        // non nulles au départ).
        (0.0f32..=0.85, 0.0f32..=0.85, 1u32..400),
        (prop::bool::weighted(0.4), prop::bool::weighted(0.15), any::<bool>()),
        (0.0f32..=1.0, 0u8..=5, any::<bool>(), 0.0f32..0.05),
    )
        .prop_map(
            |((from, to, frames), (real_drop, forced, flashes), (heat, level, english, dt))| Segment {
                from,
                to,
                frames,
                real_drop,
                forced,
                flashes,
                heat,
                level,
                language: if english { TitleLanguage::En } else { TitleLanguage::Fr },
                dt: dt + 0.002,
            },
        )
}

/// Portes au hasard : rangs de plans, plans interdits, cascades, Ascension, dosage du flash.
fn gates() -> impl Strategy<Value = ShowGates> {
    (any::<[bool; 2]>(), 0u8..128, 0usize..4, any::<bool>(), 0.0f32..=1.0).prop_map(
        |(tiers, blocked, stunts, ascension, flash)| {
            let mut g = open_gates();
            g.shot_tiers = ["common", "uncommon"]
                .iter()
                .zip(tiers)
                .filter(|(_, on)| *on)
                .map(|(t, _)| ShotTier::new(*t).unwrap())
                .collect();
            g.shot_block = SHOTS
                .iter()
                .enumerate()
                .filter(|(i, _)| blocked & (1 << i) != 0)
                .map(|(_, (id, _))| ShotId::new(*id).unwrap())
                .chain((blocked % 5 == 0).then(|| ShotId::new("stunt-cam").unwrap()))
                .collect();
            g.stunt_kinds.truncate(stunts);
            g.ascension = ascension;
            g.fx.flash = flash;
            g
        },
    )
}

fn titles() -> DropTitles {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/titles/drop-titles.ron");
    DropTitles::load(&path).unwrap()
}

/// Ce qui est observable à la fin d'une image, avec ce qui l'a produit.
#[derive(Debug, Clone, PartialEq)]
struct Observed {
    drop_now: bool,
    plan: Option<DropPlan>,
    signal: bool,
    flashes: bool,
    level: u8,
}

fn run(seed: u64, gates: &ShowGates, segments: &[Segment]) -> Vec<Observed> {
    let mut conductor = DropConductor::new(Pcg32::seed_from_u64(seed), titles());
    let mut freshness = Freshness::default();
    conductor.register_known(&mut freshness);
    let mut now = 0.0f64;
    let mut out = Vec::new();
    for seg in segments {
        for i in 0..seg.frames {
            let last = i + 1 == seg.frames;
            let (real_drop, forced) = (last && seg.real_drop, last && seg.forced);
            now += f64::from(seg.dt);
            let input = DropInput {
                dt: seg.dt,
                now,
                intensity: seg.from + (seg.to - seg.from) * i as f32 / seg.frames as f32,
                real_drop,
                forced,
                gates,
                heat: seg.heat,
                level: seg.level,
                flashes: seg.flashes,
                language: seg.language,
            };
            conductor.update(&mut freshness, &input);
            out.push(Observed {
                drop_now: conductor.drop_now(),
                plan: conductor.plan().cloned(),
                signal: real_drop || forced,
                flashes: seg.flashes,
                level: seg.level,
            });
        }
    }
    out
}

fn check_plan(plan: &DropPlan, gates: &ShowGates, seen: &Observed) -> Result<(), TestCaseError> {
    let fx = plan.fx;
    prop_assert!((0.0..=1.0).contains(&fx.flash), "flash {}", fx.flash);
    prop_assert!((0.0..=1.0).contains(&fx.rgb_split) && (0.0..=1.0).contains(&fx.city_pulse));
    prop_assert!((1..=3).contains(&fx.shockwaves), "ondes de choc {}", fx.shockwaves);
    prop_assert!((20..=60).contains(&fx.burst), "gerbes {}", fx.burst);
    if !seen.flashes {
        prop_assert!(
            fx.flash.abs() < f32::EPSILON,
            "flash {} alors que les flashs sont coupés",
            fx.flash
        );
    }
    if let Some(stunt) = &plan.stunt {
        let listed = gates.stunt_kinds.iter().any(|k| k == stunt);
        prop_assert!(
            listed || (stunt.as_str() == "ascension" && gates.ascension),
            "cascade {stunt} hors portes"
        );
        prop_assert!(!gates.stunt_kinds.is_empty());
    }
    if let Some(shot) = &plan.shot {
        let tier_ok = |tier: &str| gates.shot_tiers.iter().any(|t| t.as_str() == tier);
        let blocked = gates.shot_block.iter().any(|b| b == shot);
        prop_assert!(!blocked, "plan {shot} interdit");
        if shot.as_str() == "stunt-cam" {
            prop_assert!(plan.stunt.is_some() && tier_ok("uncommon"));
        } else {
            let tier = SHOTS.iter().find(|(id, _)| *id == shot.as_str()).map(|(_, tier)| *tier);
            prop_assert!(tier.is_some_and(tier_ok), "plan {shot} hors rangs permis");
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn prop_drop_now_follows_the_signal_and_every_plan_is_safe_and_allowed(
        seed in any::<u64>(),
        gates in gates(),
        segments in prop::collection::vec(segment(), 1..8),
    ) {
        for seen in run(seed, &gates, &segments) {
            prop_assert_eq!(seen.drop_now, seen.signal);
            if seen.drop_now {
                prop_assert!(seen.plan.is_some(), "pas de plan lisible sur l'image du drop");
            }
            if let Some(plan) = &seen.plan {
                check_plan(plan, &gates, &seen)?;
                if let Some(title) = &plan.title {
                    prop_assert!(seen.drop_now, "titre hors d'un drop");
                    prop_assert!(seen.level >= 2, "titre au niveau {}", seen.level);
                    prop_assert_eq!(&**title, title.to_uppercase());
                }
            }
        }
    }

    #[test]
    fn prop_same_seed_and_inputs_give_the_same_decisions(
        seed in any::<u64>(),
        gates in gates(),
        segments in prop::collection::vec(segment(), 1..5),
    ) {
        prop_assert_eq!(run(seed, &gates, &segments), run(seed, &gates, &segments));
    }
}
