//! J2-08 : trois scénarios de soirée rejoués sans rendu (instantanés `insta`), et le budget de temps : trois heures de
//! set simulées en moins de dix secondes.

#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "fichier de test : les aides hors #[test] peuvent échouer bruyamment"
)]

mod director_rig;

use std::time::Instant;

use cf_control::{Action, DjIndex, Switch};
use cf_show::load::LoadedShow;
use cf_show::sim::{Command, Script, SimReport, SyntheticMusic, simulate};
use director_rig::{assets, cue, event, library, slot, titles};
use rand_pcg::Pcg32;

const SEED: u64 = 2026;
const THREE_HOURS_S: f64 = 3.0 * 3600.0;

fn run(show: &LoadedShow, music: SyntheticMusic, script: Script, duration_s: f64) -> SimReport {
    simulate::<Pcg32>(show, titles(), SEED, music, script, duration_s).unwrap()
}

fn montee_auto() -> LoadedShow {
    LoadedShow::load(&assets().join("shows/montee-auto.ron"), &library()).unwrap()
}

/// La montée automatique de Sillage : la musique n'arrive qu'à 20 s (pré-intro), puis les niveaux montent au temps
/// (`After`) et entrent au drop. Trois heures : la fin du set passe en variations.
fn montee_auto_run() -> SimReport {
    run(
        &montee_auto(),
        SyntheticMusic::club(125.0, false),
        Script::new().at(20.0, Command::Live(true)),
        THREE_HOURS_S,
    )
}

#[test]
fn scenario_auto_ramp_three_hours() {
    let report = montee_auto_run();
    assert_eq!(
        (report.final_slot, report.final_cue),
        (0, 5),
        "le set finit au niveau 5"
    );
    insta::assert_snapshot!(report.trace.render());
}

/// Un créneau Semi : le DJ appuie sur GO avant une échéance, fige l'avancée, force un drop, passe au noir puis au
/// repli, et quitte le repli par GO.
fn semi_run() -> SimReport {
    let cues = [
        cue("c0", "niveau-1-ville", "Cut", "After(seconds: 300.0)"),
        cue("c1", "niveau-2-rues", "OnDrop", "After(seconds: 600.0)"),
        cue("c2", "niveau-3-envol", "OnDrop", "After(seconds: 600.0)"),
        cue("c3", "niveau-4-meteores", "OnPhrase", "Manual"),
    ]
    .concat();
    let show = event(&slot("semi", "Semi", "", &cues));
    let script = Script::new()
        .act(100.0, Action::Go) // avant l'échéance de c0 : c1 est appelé, il entre au prochain drop
        .act(500.0, Action::Hold(Switch::On)) // l'échéance de c1 (900 s) passe sans effet
        .act(1000.0, Action::Hold(Switch::Off)) // l'avancée due part (au drop)
        .act(1500.0, Action::ForceDrop)
        .act(1800.0, Action::Blackout(Switch::On))
        .act(1805.0, Action::Blackout(Switch::Off))
        .act(2000.0, Action::SafeLook)
        .act(2100.0, Action::Go); // quitte le repli sans avancer
    run(&show, SyntheticMusic::club(128.0, true), script, 3600.0)
}

#[test]
fn scenario_semi_with_go() {
    insta::assert_snapshot!(semi_run().trace.render());
}

/// Trois DJs : le premier en Auto, le deuxième en Semi, le troisième en Manuel ; passation vers un DJ armé hors
/// ordre, changement de tempo, perte de la source en plein set.
fn three_djs_run() -> SimReport {
    let a = [
        cue("a0", "niveau-1-ville", "Cut", "After(seconds: 400.0)"),
        cue("a1", "niveau-2-rues", "OnDrop", "After(seconds: 400.0)"),
        cue("a2", "niveau-3-envol", "OnDrop", "Manual"),
    ]
    .concat();
    let b = [
        cue("b0", "niveau-2-rues", "OnPhrase", "After(seconds: 300.0)"),
        cue("b1", "niveau-4-meteores", "OnDrop", "Manual"),
    ]
    .concat();
    let c = [
        cue("c0", "niveau-3-envol", "Cut", "Manual"),
        cue("c1", "niveau-5-cosmos", "OnDrop", "Manual"),
    ]
    .concat();
    let show = event(&format!(
        "{}{}{}",
        slot("aya", "Auto", "", &a),
        slot("brume", "Semi", ", theme: Preset(Synthwave)", &b),
        slot("coda", "Manual", "", &c)
    ));
    let script = Script::new()
        .act(600.0, Action::ArmDj(DjIndex(2))) // le troisième DJ passe avant le deuxième
        .act(900.0, Action::NextDj)
        .at(900.0, Command::Tempo(134.0))
        .at(900.0, Command::NewTrack)
        .act(1200.0, Action::Go)
        .act(1500.0, Action::PrevDj) // de Coda, le précédent dans l'ordre est Brume
        .at(1500.0, Command::Tempo(124.0))
        .at(1800.0, Command::Live(false)) // le micro lâche pendant 40 s
        .at(1840.0, Command::Live(true))
        .act(2400.0, Action::NextDj) // pas d'armé : le suivant dans l'ordre
        .act(3000.0, Action::Go)
        .act(3300.0, Action::NextDj); // Coda est le dernier créneau : l'ordre est sans effet
    run(&show, SyntheticMusic::club(124.0, true), script, 5400.0)
}

#[test]
fn scenario_three_djs_evening() {
    insta::assert_snapshot!(three_djs_run().trace.render());
}

#[test]
fn simulation_is_deterministic() {
    assert_eq!(semi_run(), semi_run());
    assert_eq!(three_djs_run(), three_djs_run());
}

#[test]
fn event_queue_never_overflows_in_any_scenario() {
    for report in [semi_run(), three_djs_run()] {
        assert!(report.trace.stats.max_events_per_frame < 64, "{:?}", report.trace.stats);
    }
}

#[test]
fn three_simulated_hours_run_in_under_ten_seconds() {
    let start = Instant::now();
    let report = montee_auto_run();
    let took = start.elapsed();
    assert_eq!(report.trace.stats.frames, 648_001, "648 000 images plus l'ouverture");
    assert!(took.as_secs_f64() < 10.0, "3 h simulées en {took:?}");
}
