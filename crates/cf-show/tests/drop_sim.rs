//! Critères de J2-04 : simulation sans rendu d'une montée puis d'un drop synthétiques (pas de 1/60 s), drop forcé sans
//! montée, oubli après `PLAN_TTL`, flashs coupés, déterminisme par la graine, titres du fichier RON.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::path::PathBuf;

use cf_show::drop::{DropConductor, DropInput, DropPlan, DropTitles, MAX_LEVEL, PLAN_TTL, TitleLanguage};
use cf_show::freshness::Freshness;
use cf_show::gates::open_gates;
use cf_show::model::{ShotId, ShotTier, ShowGates, StuntKind};
use rand_core::SeedableRng;
use rand_pcg::Pcg32;

const DT: f32 = 1.0 / 60.0;

fn titles() -> DropTitles {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/titles/drop-titles.ron");
    DropTitles::load(&path).unwrap_or_else(|e| panic!("{e}"))
}

/// Le conductor, sa fraîcheur et l'horloge du show.
struct Rig {
    conductor: DropConductor<Pcg32>,
    freshness: Freshness,
    gates: ShowGates,
    now: f64,
    heat: f32,
    level: u8,
    flashes: bool,
    language: TitleLanguage,
}

/// Ce que le conductor publie à la fin d'une image.
#[derive(Debug, Clone, PartialEq)]
struct Frame {
    drop_now: bool,
    plan: Option<DropPlan>,
}

impl Rig {
    fn new(seed: u64) -> Self {
        let conductor = DropConductor::new(Pcg32::seed_from_u64(seed), titles());
        let mut freshness = Freshness::default();
        conductor.register_known(&mut freshness);
        Self {
            conductor,
            freshness,
            gates: open_gates(),
            now: 0.0,
            heat: 0.5,
            level: 3,
            flashes: true,
            language: TitleLanguage::Fr,
        }
    }

    fn step(&mut self, intensity: f32, real_drop: bool, forced: bool) -> Frame {
        self.now += f64::from(DT);
        let input = DropInput {
            dt: DT,
            now: self.now,
            intensity,
            real_drop,
            forced,
            gates: &self.gates,
            heat: self.heat,
            level: self.level,
            flashes: self.flashes,
            language: self.language,
        };
        self.conductor.update(&mut self.freshness, &input);
        Frame {
            drop_now: self.conductor.drop_now(),
            plan: self.conductor.plan().cloned(),
        }
    }

    fn hold(&mut self, seconds: f32, intensity: f32) -> Vec<Frame> {
        (0..(seconds / DT) as u32)
            .map(|_| self.step(intensity, false, false))
            .collect()
    }

    /// Montée linéaire de `from` à `to` sur `seconds`.
    fn ramp(&mut self, seconds: f32, from: f32, to: f32) -> Vec<Frame> {
        let n = (seconds / DT) as u32;
        (0..n)
            .map(|i| self.step(from + (to - from) * i as f32 / n as f32, false, false))
            .collect()
    }
}

#[test]
fn plan_exists_during_the_build_and_drop_now_is_a_single_frame() {
    let mut rig = Rig::new(42);
    let calm = rig.hold(30.0, 0.3);
    assert!(
        calm.iter().all(|f| f.plan.is_none() && !f.drop_now),
        "rien à préparer en régime calme"
    );

    let build = rig.ramp(12.0, 0.3, 0.85);
    let first = build
        .iter()
        .position(|f| f.plan.is_some())
        .expect("un plan est préparé pendant la montée");
    assert!(
        build[first..].iter().all(|f| f.plan.is_some() && !f.drop_now),
        "le plan reste lisible jusqu'au drop"
    );
    assert!(build[first].plan.as_ref().is_some_and(|p| !p.forced));
    let prepared = build.last().unwrap().plan.clone().unwrap();

    // Le drop réel arrive : une seule image avec `drop_now`, le plan est celui qui était préparé.
    let strike = rig.step(0.9, true, false);
    assert!(strike.drop_now);
    let hit = strike.plan.expect("le plan est lisible sur l'image du drop");
    assert_eq!((&hit.shot, &hit.stunt), (&prepared.shot, &prepared.stunt));
    assert_eq!(hit.fx.shockwaves, prepared.fx.shockwaves);
    assert!(!hit.forced);

    let after = rig.hold(40.0, 0.9);
    assert!(
        after.iter().all(|f| !f.drop_now && f.plan.is_none()),
        "ni drop ni plan après la frappe"
    );
}

#[test]
fn drop_now_is_true_on_exactly_one_frame_per_drop() {
    let mut rig = Rig::new(1);
    rig.hold(30.0, 0.3);
    rig.ramp(12.0, 0.3, 0.85);
    let mut frames = vec![rig.step(0.9, true, false)];
    frames.extend(rig.hold(5.0, 0.9));
    assert_eq!(frames.iter().filter(|f| f.drop_now).count(), 1);
    assert!(frames[0].drop_now);
}

#[test]
fn forced_drop_without_a_build_prepares_and_strikes_at_once() {
    let mut rig = Rig::new(3);
    rig.hold(10.0, 0.4);
    let strike = rig.step(0.4, false, true);
    assert!(strike.drop_now);
    assert!(strike.plan.as_ref().is_some_and(|p| p.forced));
    let after = rig.hold(2.0, 0.4);
    assert!(after.iter().all(|f| !f.drop_now && f.plan.is_none()));
}

#[test]
fn forced_drop_during_a_build_keeps_the_prepared_plan_and_marks_it_forced() {
    let mut rig = Rig::new(4);
    rig.hold(30.0, 0.3);
    let build = rig.ramp(12.0, 0.3, 0.85);
    let prepared = build.last().unwrap().plan.clone().unwrap();
    assert!(!prepared.forced);
    let strike = rig.step(0.85, false, true).plan.unwrap();
    assert!(strike.forced);
    assert_eq!((strike.shot, strike.stunt), (prepared.shot, prepared.stunt));
}

#[test]
fn a_prepared_drop_that_never_arrives_is_forgotten_after_plan_ttl() {
    let mut rig = Rig::new(5);
    rig.hold(30.0, 0.3);
    let mut frames = rig.ramp(12.0, 0.3, 0.85);
    // Le plan est prêt ; la musique reste en haut sans jamais drop.
    frames.extend(rig.hold(PLAN_TTL + 10.0, 0.85));
    let prepared_at = frames
        .iter()
        .position(|f| f.plan.is_some())
        .expect("un plan est préparé");
    let gone = prepared_at
        + frames[prepared_at..]
            .iter()
            .position(|f| f.plan.is_none())
            .expect("plan oublié");
    let seconds = (gone - prepared_at) as f32 * DT;
    assert!(
        (PLAN_TTL..PLAN_TTL + 0.1).contains(&seconds),
        "oublié {seconds} s après sa préparation"
    );
    assert!(
        frames[gone..].iter().all(|f| f.plan.is_none() && !f.drop_now),
        "ni drop ni nouveau plan ensuite"
    );
}

#[test]
fn flashes_off_means_no_flash_in_any_plan() {
    let mut rig = Rig::new(6);
    rig.flashes = false;
    rig.heat = 1.0;
    rig.gates.fx.flash = 1.0;
    rig.hold(30.0, 0.3);
    let mut frames = rig.ramp(12.0, 0.3, 0.85);
    frames.push(rig.step(0.9, true, false));
    frames.push(rig.step(0.9, false, true)); // aussi sur un drop forcé
    let plans: Vec<_> = frames.iter().filter_map(|f| f.plan.as_ref()).collect();
    assert!(plans.len() > 10);
    assert!(
        plans.iter().all(|p| p.fx.flash.abs() < f32::EPSILON),
        "flash non nul alors que les flashs sont coupés"
    );
}

#[test]
fn flashes_cut_between_preparation_and_strike_leave_no_flash() {
    let mut rig = Rig::new(7);
    rig.heat = 1.0;
    rig.gates.fx.flash = 1.0;
    rig.hold(30.0, 0.3);
    let build = rig.ramp(12.0, 0.3, 0.85);
    let prepared = build.last().unwrap().plan.as_ref().unwrap();
    assert!(
        prepared.fx.flash > 0.5,
        "précondition : la montée prépare bien un flash ({})",
        prepared.fx.flash
    );

    rig.flashes = false; // l'opérateur coupe les flashs avant la frappe
    let strike = rig.step(0.9, true, false).plan.unwrap();
    assert!(strike.fx.flash.abs() < f32::EPSILON);
    // Les autres effets ne sont pas touchés.
    assert!((strike.fx.rgb_split - prepared.fx.rgb_split).abs() < f32::EPSILON);
}

#[test]
fn replan_on_the_strike_frame_redoes_the_plan_without_title() {
    let mut rig = Rig::new(8);
    rig.level = 2;
    rig.hold(30.0, 0.3);
    rig.ramp(12.0, 0.3, 0.85);
    assert!(rig.step(0.9, true, false).drop_now);

    // Le niveau change sur ce drop : portes fermées, flashs coupés, plan refait.
    rig.level = 5;
    rig.flashes = false;
    let mut closed = open_gates();
    closed.stunt_kinds.clear();
    closed.ascension = false;
    closed.shot_tiers = vec![ShotTier::new("common").unwrap()];
    let input = DropInput {
        dt: DT,
        now: rig.now,
        intensity: 0.9,
        real_drop: false,
        forced: false,
        gates: &closed,
        heat: 0.0,
        level: 5,
        flashes: false,
        language: TitleLanguage::Fr,
    };
    rig.conductor.replan(&rig.freshness, &input);
    let plan = rig.conductor.plan().unwrap();
    assert!(rig.conductor.drop_now());
    assert!(plan.stunt.is_none() && plan.title.is_none());
    assert!(plan.fx.flash.abs() < f32::EPSILON);
    assert_eq!(plan.fx.shockwaves, 3);
    let common = ["chase", "side-track", "bumper", "orbit"];
    assert!(plan.shot.as_ref().is_some_and(|s| common.contains(&s.as_str())));
}

#[test]
fn replan_without_a_plan_does_nothing() {
    let mut rig = Rig::new(9);
    rig.hold(1.0, 0.3);
    let gates = open_gates();
    let input = DropInput {
        dt: DT,
        now: rig.now,
        intensity: 0.3,
        real_drop: false,
        forced: false,
        gates: &gates,
        heat: 0.5,
        level: 3,
        flashes: true,
        language: TitleLanguage::Fr,
    };
    rig.conductor.replan(&rig.freshness, &input);
    assert!(rig.conductor.plan().is_none());
}

#[test]
fn choices_respect_the_gates() {
    let mut rig = Rig::new(10);
    rig.gates.shot_tiers = vec![ShotTier::new("common").unwrap()];
    rig.gates.shot_block = vec![ShotId::new("chase").unwrap(), ShotId::new("orbit").unwrap()];
    rig.gates.stunt_kinds = vec![StuntKind::new("slalom").unwrap()];
    rig.gates.ascension = false;
    rig.heat = 1.0;
    let mut stunts = 0;
    for _ in 0..200 {
        rig.hold(1.0, 0.4);
        let plan = rig.step(0.4, false, true).plan.unwrap();
        assert!(
            matches!(plan.shot.as_ref().map(ShotId::as_str), Some("side-track" | "bumper")),
            "{:?}",
            plan.shot
        );
        if let Some(stunt) = plan.stunt {
            assert_eq!(stunt.as_str(), "slalom");
            stunts += 1;
        }
    }
    assert!(
        stunts > 100,
        "chaleur au maximum : cascade la plupart du temps ({stunts})"
    );
}

#[test]
fn a_stunt_takes_the_stunt_camera_when_uncommon_shots_are_allowed() {
    let mut rig = Rig::new(11);
    rig.heat = 1.0;
    rig.gates.ascension = false;
    let mut seen = 0;
    for _ in 0..100 {
        rig.hold(1.0, 0.4);
        let plan = rig.step(0.4, false, true).plan.unwrap();
        if plan.stunt.is_some() {
            assert_eq!(plan.shot.as_ref().map(ShotId::as_str), Some("stunt-cam"));
            seen += 1;
        }
    }
    assert!(seen > 20);

    // Plan de cascade interdit : on retombe sur le tirage ordinaire.
    rig.gates.shot_block = vec![ShotId::new("stunt-cam").unwrap()];
    for _ in 0..50 {
        rig.hold(1.0, 0.4);
        let plan = rig.step(0.4, false, true).plan.unwrap();
        assert_ne!(plan.shot.as_ref().map(ShotId::as_str), Some("stunt-cam"));
    }
}

#[test]
fn closed_gates_prepare_nothing_but_effects() {
    let mut rig = Rig::new(12);
    rig.gates.shot_tiers.clear();
    rig.gates.stunt_kinds.clear();
    let plan = rig.step(0.4, false, true).plan.unwrap();
    assert!(plan.shot.is_none() && plan.stunt.is_none());
}

#[test]
fn ascension_is_possible_only_when_its_gate_is_open() {
    let mut rig = Rig::new(13);
    let mut with_gate = 0;
    for _ in 0..300 {
        rig.heat = 0.0;
        let plan = rig.step(0.4, false, true).plan.unwrap();
        rig.now += 1000.0; // la fraîcheur se repose entre deux drops
        if plan.stunt.as_ref().is_some_and(|s| s.as_str() == "ascension") {
            with_gate += 1;
        }
    }
    // Rare par construction : 20 % de chance, et la fraîcheur repousse une cascade qui sort trop souvent.
    assert!(with_gate >= 1, "l'Ascension n'est jamais sortie sur 300 drops");

    rig.gates.ascension = false;
    for _ in 0..300 {
        let plan = rig.step(0.4, false, true).plan.unwrap();
        rig.now += 1000.0;
        assert!(plan.stunt.as_ref().is_none_or(|s| s.as_str() != "ascension"));
    }
}

#[test]
fn strikes_count_as_played_in_the_freshness() {
    let mut rig = Rig::new(14);
    rig.gates.ascension = false;
    let before = rig.freshness.weight("shot:chase", rig.now);
    let plan = rig.step(0.4, false, true).plan.unwrap();
    let shot = format!("shot:{}", plan.shot.unwrap());
    assert!(
        rig.freshness.weight(&shot, rig.now) < before,
        "le plan joué est repoussé"
    );
}

/// Rejoue `n` drops forcés d'un même set et renvoie ce qui a été choisi.
fn replay(seed: u64, n: usize) -> Vec<(Option<ShotId>, Option<StuntKind>, Option<String>)> {
    let mut rig = Rig::new(seed);
    (0..n)
        .map(|_| {
            rig.hold(2.0, 0.4);
            let plan = rig.step(0.4, false, true).plan.unwrap();
            (plan.shot, plan.stunt, plan.title.map(|t| t.to_string()))
        })
        .collect()
}

#[test]
fn same_seed_same_drops_other_seed_other_drops() {
    assert_eq!(replay(99, 60), replay(99, 60));
    assert_ne!(replay(99, 60), replay(100, 60));
}

#[test]
fn titles_follow_the_gap_the_level_the_language_and_the_file() {
    // Niveau 1 : jamais de carton.
    let mut rig = Rig::new(20);
    rig.level = 1;
    assert!((0..50).all(|_| rig.step(0.4, false, true).plan.unwrap().title.is_none()));

    // Niveau 3 : un titre tous les 4 à 6 drops, en capitales, dans la langue demandée.
    let mut rig = Rig::new(21);
    rig.level = 3;
    let mut drops_with_title = Vec::new();
    for n in 1..=60 {
        if let Some(title) = rig.step(0.4, false, true).plan.unwrap().title {
            assert_eq!(*title, title.to_uppercase());
            drops_with_title.push(n);
        }
    }
    assert!(
        (4..=6).contains(&drops_with_title[0]),
        "premier titre au drop n°{}",
        drops_with_title[0]
    );
    for pair in drops_with_title.windows(2) {
        assert!((4..=6).contains(&(pair[1] - pair[0])), "{drops_with_title:?}");
    }

    // Au niveau 2, seuls les titres de niveau minimal 2 sortent ; en anglais, ce sont les textes anglais.
    let mut rig = Rig::new(22);
    rig.level = 2;
    rig.language = TitleLanguage::En;
    let level_two_en = [
        "DROP",
        "HERE WE GO",
        "NOW",
        "NO BRAKES",
        "LET GO",
        "FULL SEND",
        "AGAIN",
        "LOUDER",
    ];
    let mut seen = 0;
    for _ in 0..200 {
        if let Some(title) = rig.step(0.4, false, true).plan.unwrap().title {
            assert!(level_two_en.contains(&&*title), "{title}");
            seen += 1;
        }
    }
    assert!(seen >= 200 / 6 - 1, "{seen}");
}

#[test]
fn the_top_titles_are_reachable_at_the_top_level() {
    let mut rig = Rig::new(23);
    rig.level = MAX_LEVEL;
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..600 {
        if let Some(title) = rig.step(0.4, false, true).plan.unwrap().title {
            seen.insert(title.to_string());
        }
        rig.now += 500.0;
    }
    assert!(
        seen.contains("AU-DELÀ") && seen.contains("JUSQU'AUX ÉTOILES") && seen.contains("TOUT CASSER"),
        "{seen:?}"
    );
}

#[test]
fn reset_forgets_the_plan_and_the_build() {
    let mut rig = Rig::new(30);
    rig.hold(30.0, 0.3);
    rig.ramp(12.0, 0.3, 0.85);
    assert!(rig.conductor.plan().is_some());
    rig.conductor.reset();
    assert!(rig.conductor.plan().is_none() && !rig.conductor.drop_now());
}

#[test]
fn the_shipped_titles_file_is_complete() {
    let titles = titles();
    assert_eq!(titles.len(), 22);
    assert!(titles.keys().any(|k| k == "title:stars"));
}
