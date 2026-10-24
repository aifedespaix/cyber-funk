//! Le réalisateur sur 10 minutes simulées **avec le vrai chef du spectacle** (J8-07) : le `ShowDirector` du show de
//! démonstration fournit les portes du look, le drop préparé et les flashs permis, comme dans le binaire.
//!
//! Les tests sans spectacle (`tests.rs`) passent par `NoShow` : ils ne peuvent pas voir un `strobe` choisi alors que les
//! flashs sont coupés, ni une coupe hors de la grille d'un vrai look.

use std::path::Path;

use bevy::math::DVec3;
use cf_analysis::AudioFeatures;
use cf_control::{Action, Switch};
use cf_render::CameraPose;
use cf_show::director::{ShowDirector, ShowInput};
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use rand_pcg::Pcg32;

use super::*;
use crate::choreo::MAX_CUTS_PER_SECOND;
use crate::editor::is_strobing;
use crate::shots::catalog;
use crate::testkit::{BoxCity, DT, Sim, TEN_MINUTES, check_pose};
use crate::types::ShowView;
use crate::world::CinemaFrame;

/// Ce que la soirée simulée a produit.
#[derive(Default)]
struct Report {
    /// Instants (horloge du monde) de chaque coupe franche admise.
    cuts_at: Vec<f64>,
    /// Coupes de routine (`PickMode::Normal`) et celles qui sont tombées sur la grille du look.
    routine: u32,
    on_grid: u32,
    /// Coupes de routine hors temps musical, hors réaction à un événement.
    off_beat: u32,
    /// Plans `strobe` lancés alors que les flashs étaient coupés.
    strobe_without_flashes: u32,
    /// Images jouées avec les flashs coupés (garde-fou : le test de `strobe` ne doit pas être vide).
    frames_without_flashes: u32,
    /// Plans lancés en tout.
    shots: u32,
    /// Un drop préparé a été annoncé au réalisateur (`drop_planned`) au moins une fois.
    drop_planned_seen: bool,
}

fn demo_director() -> ShowDirector<Pcg32> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets");
    let looks = LookLibrary::load_dir(&assets.join("looks")).expect("looks du dépôt");
    let show = LoadedShow::load(&assets.join("shows/demo.ron"), &looks).expect("show de démonstration");
    ShowDirector::<Pcg32>::new(&show, DropTitles::empty(), 7).expect("chef du spectacle")
}

/// Joue 10 minutes. Si `kill_flashes`, `FlashKill(On)` (coupe les flashs) est envoyé à la première image.
fn play(kill_flashes: bool) -> Report {
    let mut sim = Sim::new(true);
    let mut show = demo_director();
    let mut director = CinemaDirector::new(catalog(), false, 42);
    let city = BoxCity;
    let mut report = Report::default();
    let start = CameraPose {
        position: sim.world.car + DVec3::new(0.0, 3.0, 8.0),
        ..CameraPose::default()
    };
    director.start_with(&start, &CinemaFrame::new(&sim.world, &city, &[]), &mut show);
    let mut shots = director.stats().shots;
    let mut cuts = director.stats().cuts;
    let mut first = true;
    let kill = [Action::FlashKill(Switch::On)];
    while sim.world.time < TEN_MINUTES {
        sim.step();
        let w = &sim.world;
        let features = AudioFeatures {
            live: true,
            kick: w.beat.pulse > 0.99,
            pulse: w.beat.pulse,
            count: w.beat.count,
            bpm: w.beat.bpm,
            section: w.section,
            intensity: w.intensity,
            ..AudioFeatures::NEUTRAL
        };
        let actions: &[Action] = if kill_flashes && first { &kill } else { &[] };
        first = false;
        let state = show
            .update(&ShowInput {
                dt: DT,
                now: w.time,
                local_time_s: 0.0,
                audio: &features,
                real_drop: false,
                actions,
            })
            .clone();
        // Ce que fait `NeonCinemaPlugin` : la vue du spectacle de l'image, plus le drop préparé.
        let mut view = ShowView::of(&state);
        view.drop_planned = show.drop_plan().is_some();
        report.drop_planned_seen |= view.drop_planned;
        report.frames_without_flashes += u32::from(!state.flashes);
        sim.world.show = view;
        sim.world.drop = state.drop_now;

        let frame = CinemaFrame::new(&sim.world, &city, &[]);
        let pose = *director.update_with(&frame, &mut show);
        check_pose(&pose, &frame);
        let stats = director.stats();
        if stats.cuts != cuts {
            cuts = stats.cuts;
            report.cuts_at.push(frame.time);
        }
        if stats.shots == shots {
            continue;
        }
        shots = stats.shots;
        report.shots += 1;
        if !state.flashes && director.shot_id().is_some_and(is_strobing) {
            report.strobe_without_flashes += 1;
        }
        if stats.last_mode == Some(PickMode::Normal) {
            report.routine += 1;
            let grid = u32::from(frame.show.grid_for(frame.section));
            if stats.last_on_beat && (frame.beat.count % crate::cues::PHRASE_BEATS).is_multiple_of(grid) {
                report.on_grid += 1;
            } else if !stats.last_on_beat && director.shot_id() != Some("touchdown") {
                // Seules les réactions à un événement (atterrissage) partent hors temps ; préparation du drop et
                // salves partent sur un temps, mais pas forcément sur la grille.
                report.off_beat += 1;
            }
        }
    }
    report
}

/// Plus grand nombre de coupes dans une fenêtre glissante d'une seconde.
fn max_cuts_in_one_second(cuts_at: &[f64]) -> usize {
    cuts_at
        .iter()
        .enumerate()
        .map(|(i, &t)| cuts_at[i..].iter().take_while(|&&u| u < t + 1.0).count())
        .max()
        .unwrap_or(0)
}

#[test]
fn dix_minutes_avec_le_vrai_chef_coupes_sur_la_grille_et_plafond_tenu() {
    let report = play(false);
    assert!(report.shots >= 20, "{} plans seulement", report.shots);
    // Chaque coupe de routine part sur un temps musical de la grille du look (le sommet de 4 temps comme le calme de 16).
    assert!(report.routine >= 10, "{} coupes de routine", report.routine);
    // Les coupes d'événements (préparation du drop, salves de fin de phrase, atterrissage) ne suivent pas la grille du
    // look : la routine, elle, la suit, et rien ne part hors d'un temps musical.
    assert_eq!(report.off_beat, 0, "coupes de routine hors temps");
    assert!(
        report.on_grid * 3 >= report.routine * 2,
        "seulement {} coupes de routine sur {} sont sur la grille",
        report.on_grid,
        report.routine
    );
    assert!(
        max_cuts_in_one_second(&report.cuts_at) <= MAX_CUTS_PER_SECOND as usize,
        "plafond de {MAX_CUTS_PER_SECOND} coupes/s dépassé"
    );
}

#[test]
fn le_drop_prepare_du_chef_est_visible_du_realisateur() {
    // `drop_planned` ne vient pas de `ShowState` mais de `ShowDirector::drop_plan` : le monde le recopie (J8-07).
    assert!(play(false).drop_planned_seen, "aucun drop préparé pendant 10 minutes");
}

#[test]
fn strobe_n_est_jamais_choisi_quand_les_flashs_sont_coupes() {
    let report = play(true);
    assert!(report.shots >= 20, "{} plans seulement", report.shots);
    assert!(
        report.frames_without_flashes > 30_000,
        "les flashs n'ont pas été coupés"
    );
    assert_eq!(report.strobe_without_flashes, 0, "plan strobe avec les flashs coupés");
}
