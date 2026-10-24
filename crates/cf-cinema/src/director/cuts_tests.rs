//! Sécurité du public : le plafond de cadence des coupes (3 par seconde glissante) et le plan `strobe`.
//!
//! Une « coupe » est mesurée sur la pose finale que lit le rig, dans le repère de la voiture : un saut de position, de
//! regard ou de champ de vision qu'aucun aucun mouvement continu (même définition que le plafond lui-même).

use bevy::math::DVec3;
use cf_render::CameraPose;

use super::tests::Sim;
use super::*;
use crate::choreo::{MAX_CUTS_PER_SECOND, WINDOW_SECONDS, is_jump};
use crate::editor::testkit::{Jumpy, MockSpec, sillage_specs};
use crate::world::NoObstacles;

/// Rejoue `seconds` de soirée et renvoie l'instant de chaque coupe vue sur la pose finale.
fn cut_times(director: &mut CinemaDirector, sim: &mut Sim, seconds: f64) -> Vec<f64> {
    let city = NoObstacles;
    let start = CameraPose {
        position: sim.world.car + DVec3::new(0.0, 3.0, 8.0),
        ..CameraPose::default()
    };
    director.start(&start, &CinemaFrame::new(&sim.world, &city, &[]));
    let mut cuts = Vec::new();
    let mut previous = start;
    let mut previous_car = sim.world.car;
    while sim.world.time < seconds {
        sim.step();
        let frame = CinemaFrame::new(&sim.world, &city, &[]);
        let pose = *director.update(&frame);
        if is_jump(&previous, previous_car, &pose, frame.car, frame.dt) {
            cuts.push(frame.time);
        }
        previous = pose;
        previous_car = frame.car;
    }
    cuts
}

/// Le plus grand nombre de coupes dans une fenêtre glissante d'une seconde.
fn busiest_window(cuts: &[f64]) -> usize {
    window_report(cuts).0
}

/// Le plus grand nombre de coupes dans une fenêtre d'une seconde, et ces coupes (pour le message d'échec).
fn window_report(cuts: &[f64]) -> (usize, Vec<f64>) {
    let in_window = |t: f64| -> Vec<f64> {
        cuts.iter()
            .copied()
            .filter(|&c| c > t - WINDOW_SECONDS && c <= t)
            .collect()
    };
    cuts.iter()
        .map(|&t| in_window(t))
        .max_by_key(Vec::len)
        .map_or((0, Vec::new()), |w| (w.len(), w))
}

fn strobe(jumpy: Jumpy) -> Box<dyn ShotSpec> {
    let spec = sillage_specs().into_iter().find(|s| s.id == "strobe").expect("strobe");
    Box::new(MockSpec { jumpy, ..spec })
}

/// À 174 BPM le `strobe` de Sillage coupe à chaque temps, soit ~2,9 fois par seconde : au plus 3 par seconde glissante,
/// quelle que soit la fenêtre.
#[test]
fn strobe_a_174_bpm_ne_depasse_pas_trois_coupes_par_seconde_glissante() {
    let mut sim = Sim::with_bpm(true, 174.0);
    let mut director = CinemaDirector::new(vec![strobe(Jumpy::OnBeat(1))], false, 5);
    let cuts = cut_times(&mut director, &mut sim, 180.0);
    assert!(
        cuts.len() > 200,
        "le plan coupe bien à chaque temps : {} coupes",
        cuts.len()
    );
    let (busiest, window) = window_report(&cuts);
    assert!(
        busiest <= MAX_CUTS_PER_SECOND as usize,
        "{busiest} coupes dans une seconde glissante : {window:?} ; {:?}",
        director.stats()
    );
}

/// Pire cas : un plan qui coupe six fois par seconde (tempo détecté qui s'emballe). Le plafond tient, et les coupes
/// refusées sont retardées, pas perdues : le plan finit par couper aussi souvent que le plafond le permet.
#[test]
fn un_plan_qui_coupe_six_fois_par_seconde_est_plafonne_a_trois() {
    let mut sim = Sim::with_bpm(true, 174.0);
    let mut director = CinemaDirector::new(vec![strobe(Jumpy::EverySeconds(0.15))], false, 5);
    let cuts = cut_times(&mut director, &mut sim, 120.0);
    let (busiest, window) = window_report(&cuts);
    assert!(
        busiest <= MAX_CUTS_PER_SECOND as usize,
        "{busiest} coupes dans une seconde glissante : {window:?} ; {:?}",
        director.stats()
    );
    let stats = director.stats();
    assert!(stats.cuts_held > 100, "{stats:?}");
    // Le plafond n'est pas qu'un mur : ~3 coupes par seconde passent.
    assert!(cuts.len() > 250, "{} coupes sur 120 s", cuts.len());
    // Ce que le réalisateur a compté est ce qu'on voit à l'écran (une coupe de transition = une coupe vue).
    assert!(
        u32::try_from(cuts.len()).unwrap_or(u32::MAX) <= stats.cuts,
        "{} vues, {} comptées",
        cuts.len(),
        stats.cuts
    );
}

/// Le plafond vaut pour tous les plans, pas seulement `strobe` : trois coupes demandées d'affilée passent, la
/// quatrième devient un fondu.
#[test]
fn une_transition_cut_en_trop_devient_un_fondu() {
    let mut director = CinemaDirector::new(vec![strobe(Jumpy::No)], false, 1);
    for t in [10.0, 10.2, 10.4] {
        assert_eq!(director.cap_cut(TransitionKind::Cut, t, 5.0), TransitionKind::Cut);
    }
    assert_eq!(director.cap_cut(TransitionKind::Cut, 10.6, 5.0), TransitionKind::Blend);
    assert_eq!(
        director.cap_cut(TransitionKind::Cut, 10.7, 100.0),
        TransitionKind::Swoop
    );
    // Les autres entrées ne sont pas des coupes et ne consomment rien.
    assert_eq!(director.cap_cut(TransitionKind::Whip, 10.7, 5.0), TransitionKind::Whip);
    assert_eq!(director.cap_cut(TransitionKind::Cut, 11.0, 5.0), TransitionKind::Cut);
    assert_eq!(director.stats().cuts, 4);
    assert_eq!(director.stats().cuts_held, 2);
}

/// Dix minutes, flashs coupés : `strobe` n'est jamais à l'antenne, même quand il est le seul plan énergique du catalogue
/// et que le drop le demande.
#[test]
fn strobe_jamais_a_l_antenne_quand_les_flashs_sont_coupes() {
    for flashes in [false, true] {
        let mut sim = Sim::new(true);
        sim.world.show.flashes = flashes;
        let mut director = CinemaDirector::new(catalog(&["chase", "orbit", "side-track", "strobe"]), false, 21);
        let city = NoObstacles;
        director.start(&CameraPose::default(), &CinemaFrame::new(&sim.world, &city, &[]));
        let mut strobe_seconds = 0;
        while sim.world.time < 1200.0 {
            sim.step();
            let frame = CinemaFrame::new(&sim.world, &city, &[]);
            director.update(&frame);
            strobe_seconds += u32::from(director.shot_id() == Some("strobe"));
        }
        if flashes {
            assert!(strobe_seconds > 0, "témoin : strobe passe quand les flashs sont permis");
        } else {
            assert_eq!(strobe_seconds, 0, "strobe à l'antenne flashs coupés");
        }
    }
}

fn catalog(ids: &[&str]) -> Vec<Box<dyn ShotSpec>> {
    sillage_specs()
        .into_iter()
        .filter(|s| ids.contains(&s.id))
        .map(|s| Box::new(s) as Box<dyn ShotSpec>)
        .collect()
}

/// Un plan qui suit la voiture sans couper n'est jamais pris pour une coupe (pas de faux positif).
#[test]
fn une_poursuite_douce_ne_declenche_aucune_coupe_retenue() {
    let mut sim = Sim::new(true);
    let mut director = CinemaDirector::new(catalog(&["chase"]), false, 8);
    let cuts = cut_times(&mut director, &mut sim, 120.0);
    assert!(busiest_window(&cuts) <= MAX_CUTS_PER_SECOND as usize);
    assert_eq!(director.stats().cuts_held, 0, "{:?}", director.stats());
}
