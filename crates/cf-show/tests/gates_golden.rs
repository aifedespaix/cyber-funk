//! Les six niveaux de Sillage (`assets/looks/niveau-*.ron`), le fondu des portes et les variations rejoués contre les
//! vrais `acts.ts` et `gates.ts` (`tools/sillage-golden/gates.ts` → `tests/golden/gates.json`), et la montée
//! automatique (`assets/shows/montee-auto.ron`) contre `LEVEL_STARTS`.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

use std::path::PathBuf;

use cf_show::gates::{Blend, GateBlender, Variation, open_gates};
use cf_show::load::{LoadedShow, LookLibrary};
use cf_show::model::{CueAdvance, CueEntry, LookId, ShowGates, SlotMode};
use serde::Deserialize;

/// Écart toléré sur un nombre de porte (Sillage calcule en f64, ici en f32) : le glissement est contractant, l'erreur
/// ne s'accumule pas.
const TOLERANCE: f32 = 2e-4;
/// Durée par défaut d'un set de Sillage (`durationMinutes` de `defaults.ts`), en secondes.
const SET_SECONDS: f64 = 180.0 * 60.0;

const LEVEL_FILES: [&str; 6] = [
    "niveau-0-pre-intro",
    "niveau-1-ville",
    "niveau-2-rues",
    "niveau-3-envol",
    "niveau-4-meteores",
    "niveau-5-cosmos",
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_excessive_bools, reason = "miroir de `ShowGates` de Sillage")]
struct GoldenGates {
    max_floor: u8,
    flight_rate: f32,
    flight_on_drop_only: bool,
    tricks: bool,
    stunt_tiers: Vec<String>,
    stunt_kinds: Vec<String>,
    stunt_gap_scale: f32,
    shot_tiers: Vec<String>,
    shot_block: Vec<String>,
    cut_grid: GoldenCutGrid,
    salvo: bool,
    events: Vec<String>,
    event_rate: f32,
    meteor_rate: f32,
    meteor_shower: bool,
    cruise: f32,
    ground_traffic: f32,
    fx: GoldenFx,
    ascension: bool,
}

#[derive(Deserialize)]
struct GoldenCutGrid {
    calm: u8,
    groove: u8,
    peak: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoldenFx {
    rgb_split: f32,
    flash: f32,
    city_pulse: f32,
}

/// Les neuf nombres qui glissent.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoldenNumbers {
    flight_rate: f32,
    stunt_gap_scale: f32,
    event_rate: f32,
    meteor_rate: f32,
    cruise: f32,
    ground_traffic: f32,
    rgb_split: f32,
    flash: f32,
    city_pulse: f32,
}

#[derive(Deserialize)]
struct Snapshot {
    mix: f32,
    gates: GoldenGates,
}

#[derive(Deserialize)]
struct Level {
    level: usize,
    snapshots: Vec<Snapshot>,
}

#[derive(Deserialize)]
struct GlideOp {
    source: usize,
    mix: f32,
    dt: f32,
    snap: bool,
}

#[derive(Deserialize)]
struct Glide {
    ops: Vec<GlideOp>,
    outs: Vec<GoldenNumbers>,
}

#[derive(Deserialize)]
struct VariationOp {
    dt: f64,
    on: bool,
}

#[derive(Deserialize)]
struct VariationOut {
    active: bool,
    source: u8,
    mix: f32,
}

#[derive(Deserialize)]
struct VariationGolden {
    seed: u32,
    ops: Vec<VariationOp>,
    outs: Vec<VariationOut>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    level_starts: Vec<f64>,
    open_gates: GoldenGates,
    levels: Vec<Level>,
    glide: Glide,
    variation: VariationGolden,
}

fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn golden() -> Golden {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden/gates.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn library() -> LookLibrary {
    LookLibrary::load_dir(&assets().join("looks")).unwrap_or_else(|e| panic!("{e}"))
}

/// Les portes des six niveaux, lues dans les fichiers de la bibliothèque.
fn level_gates(library: &LookLibrary) -> Vec<ShowGates> {
    LEVEL_FILES
        .iter()
        .map(|id| library.get(&LookId::new(*id).unwrap()).unwrap().look.gates.clone())
        .collect()
}

fn close(actual: f32, expected: f32, what: &str, ctx: &str) {
    assert!(
        (actual - expected).abs() <= TOLERANCE,
        "{ctx} : {what} vaut {actual}, Sillage {expected}"
    );
}

fn strings<T: ToString>(list: &[T]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

fn assert_gates(actual: &ShowGates, expected: &GoldenGates, ctx: &str) {
    assert_eq!(actual.max_floor, expected.max_floor, "{ctx} : max_floor");
    assert_eq!(
        actual.flight_on_drop_only, expected.flight_on_drop_only,
        "{ctx} : flight_on_drop_only"
    );
    assert_eq!(actual.tricks, expected.tricks, "{ctx} : tricks");
    assert_eq!(
        strings(&actual.stunt_tiers),
        expected.stunt_tiers,
        "{ctx} : stunt_tiers"
    );
    assert_eq!(
        strings(&actual.stunt_kinds),
        expected.stunt_kinds,
        "{ctx} : stunt_kinds"
    );
    assert_eq!(strings(&actual.shot_tiers), expected.shot_tiers, "{ctx} : shot_tiers");
    assert_eq!(strings(&actual.shot_block), expected.shot_block, "{ctx} : shot_block");
    assert_eq!(actual.cut_grid.calm, expected.cut_grid.calm, "{ctx} : cut_grid.calm");
    assert_eq!(
        actual.cut_grid.groove, expected.cut_grid.groove,
        "{ctx} : cut_grid.groove"
    );
    assert_eq!(actual.cut_grid.peak, expected.cut_grid.peak, "{ctx} : cut_grid.peak");
    assert_eq!(actual.salvo, expected.salvo, "{ctx} : salvo");
    assert_eq!(strings(&actual.events), expected.events, "{ctx} : events");
    assert_eq!(actual.meteor_shower, expected.meteor_shower, "{ctx} : meteor_shower");
    assert_eq!(actual.ascension, expected.ascension, "{ctx} : ascension");
    close(actual.flight_rate, expected.flight_rate, "flight_rate", ctx);
    close(actual.stunt_gap_scale, expected.stunt_gap_scale, "stunt_gap_scale", ctx);
    close(actual.event_rate, expected.event_rate, "event_rate", ctx);
    close(actual.meteor_rate, expected.meteor_rate, "meteor_rate", ctx);
    close(actual.cruise, expected.cruise, "cruise", ctx);
    close(actual.ground_traffic, expected.ground_traffic, "ground_traffic", ctx);
    close(actual.fx.rgb_split, expected.fx.rgb_split, "fx.rgb_split", ctx);
    close(actual.fx.flash, expected.fx.flash, "fx.flash", ctx);
    close(actual.fx.city_pulse, expected.fx.city_pulse, "fx.city_pulse", ctx);
}

fn assert_numbers(actual: &ShowGates, expected: &GoldenNumbers, ctx: &str) {
    close(actual.flight_rate, expected.flight_rate, "flight_rate", ctx);
    close(actual.stunt_gap_scale, expected.stunt_gap_scale, "stunt_gap_scale", ctx);
    close(actual.event_rate, expected.event_rate, "event_rate", ctx);
    close(actual.meteor_rate, expected.meteor_rate, "meteor_rate", ctx);
    close(actual.cruise, expected.cruise, "cruise", ctx);
    close(actual.ground_traffic, expected.ground_traffic, "ground_traffic", ctx);
    close(actual.fx.rgb_split, expected.rgb_split, "fx.rgb_split", ctx);
    close(actual.fx.flash, expected.flash, "fx.flash", ctx);
    close(actual.fx.city_pulse, expected.city_pulse, "fx.city_pulse", ctx);
}

/// Look du niveau suivant, ou le même au niveau 5 (`ACTS[min(5, source + 1)]`).
fn next(gates: &[ShowGates], level: usize) -> &ShowGates {
    &gates[(level + 1).min(5)]
}

#[test]
fn level_files_equal_sillage_acts() {
    let golden = golden();
    let gates = level_gates(&library());
    for level in &golden.levels {
        let ctx = format!("niveau {}", level.level);
        // Les fichiers RON : les nombres de l'acte, atteints au début (mix 0).
        assert_gates(&gates[level.level], &level.snapshots[0].gates, &ctx);
    }
}

#[test]
fn blender_reproduces_sillage_gates_for_every_level_and_mix() {
    let golden = golden();
    let gates = level_gates(&library());
    for level in &golden.levels {
        for snapshot in &level.snapshots {
            let ctx = format!("niveau {} à mix {}", level.level, snapshot.mix);
            let mut blender = GateBlender::new();
            blender.update(
                &gates[level.level],
                next(&gates, level.level),
                snapshot.mix,
                Blend::ACT_PROGRESS,
                0.0,
                true,
            );
            assert_gates(blender.gates(), &snapshot.gates, &ctx);
        }
    }
}

#[test]
fn blender_glide_follows_sillage_frame_by_frame() {
    let golden = golden();
    let gates = level_gates(&library());
    let mut blender = GateBlender::new();
    blender.update(&gates[0], &gates[0], 0.0, Blend::ACT_PROGRESS, 0.0, true);
    for (i, (op, out)) in golden.glide.ops.iter().zip(&golden.glide.outs).enumerate() {
        blender.update(
            &gates[op.source],
            next(&gates, op.source),
            op.mix,
            Blend::ACT_PROGRESS,
            op.dt,
            op.snap,
        );
        assert_numbers(blender.gates(), out, &format!("image {i}"));
    }
}

#[test]
fn open_gates_equal_sillage_open_gates() {
    assert_gates(&open_gates(), &golden().open_gates, "OPEN_GATES");
}

/// LCG 32 bits (Numerical Recipes) du script de référence : même suite de tirages.
fn lcg(seed: u32) -> impl FnMut() -> f64 {
    let mut state = seed;
    move || {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        f64::from(state) / 4_294_967_296.0
    }
}

#[test]
fn variation_matches_sillage() {
    let golden = golden();
    let mut variation = Variation::new(lcg(golden.variation.seed));
    let mut changes = 0;
    let mut last = (false, 0.0);
    for (i, (op, out)) in golden.variation.ops.iter().zip(&golden.variation.outs).enumerate() {
        let active = variation.update(op.dt, op.on);
        assert_eq!(active, out.active, "image {i} : active");
        if active {
            // Sillage : source 4 = le niveau du bas, 5 = celui du haut.
            assert_eq!(variation.uses_lower(), out.source == 4, "image {i} : source");
            close(variation.mix(), out.mix, "mix", &format!("image {i}"));
            if (variation.uses_lower(), variation.mix()) != last {
                changes += 1;
                last = (variation.uses_lower(), variation.mix());
            }
        }
    }
    assert!(
        changes >= 6,
        "le scénario doit exercer plusieurs tirages, il n'en fait que {changes}"
    );
}

#[test]
fn auto_climb_follows_sillage_level_starts() {
    let library = library();
    let show = LoadedShow::load(&assets().join("shows/montee-auto.ron"), &library).unwrap_or_else(|e| panic!("{e}"));
    let slots = &show.event().slots;
    assert_eq!(slots.len(), 1);
    let slot = &slots[0];
    assert_eq!(slot.mode, SlotMode::Auto);
    let ids: Vec<&str> = slot.cues.iter().map(|cue| cue.look.as_str()).collect();
    assert_eq!(ids, LEVEL_FILES);

    // La pré-intro se quitte quand la musique est là (`wait.ts`) ; chaque niveau 1 à 4 avance après la part de set
    // qui le sépare du suivant.
    let starts = golden().level_starts;
    assert!(matches!(slot.cues[0].advance, CueAdvance::OnMusic));
    assert!(matches!(slot.cues[5].advance, CueAdvance::Manual));
    for level in 1..=4 {
        let expected = (starts[level + 1] - starts[level]) * SET_SECONDS;
        match slot.cues[level].advance {
            CueAdvance::After { seconds } => {
                assert!(
                    (seconds - expected).abs() < 1e-6,
                    "niveau {level} : {seconds} s, Sillage {expected} s"
                );
            }
            other => panic!("niveau {level} : avancée {other:?}, attendu After"),
        }
    }
    // Entrées : le niveau 1 coupe, les suivants attendent un drop (`levelIsDue` de Sillage).
    assert_eq!(slot.cues[1].entry, CueEntry::Cut);
    assert!(slot.cues[2..].iter().all(|cue| cue.entry == CueEntry::OnDrop));
    // Le repli de sécurité ne flashe pas et ne vole pas.
    let safe = show.look(&show.event().settings.safe_look).unwrap();
    assert!(safe.look.gates.fx.flash.abs() < f32::EPSILON && safe.look.gates.max_floor == 0);
}
