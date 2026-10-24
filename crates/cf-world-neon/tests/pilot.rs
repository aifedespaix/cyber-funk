//! Le pilote en simulation (critère de J5-06) : aux niveaux bas la voiture ne vole jamais ; au niveau 5 elle vole,
//! monte aux étages, fait des figures et esquive les arches. Les portes sont celles des vrais looks de niveau.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod pilot_rig;

use cf_world_neon::pilot::{DodgeSide, FlightPhase, Pilot, PilotTick, ShowInput, TrickKind};
use pilot_rig::{DT, music, quiet, settings, show};

#[derive(Debug, Default)]
struct Stats {
    airborne_frames: u64,
    max_altitude: f64,
    max_floor: u8,
    tricks: Vec<TrickKind>,
    takeoffs: u32,
    max_abs_x: f64,
    distance: f64,
    trace: Vec<(f64, f64)>,
}

/// Fait rouler `seconds` secondes ; `edit` retouche le spectacle de chaque image (drop, respiration…).
fn simulate(seed: u64, level: u8, heat: f64, seconds: f64, edit: impl Fn(u64, &mut ShowInput)) -> Stats {
    let settings = settings(seed);
    let traffic = quiet();
    let mut pilot = Pilot::new(seed, 0.0);
    let mut stats = Stats::default();
    let mut was = FlightPhase::Ground;
    let start_z = pilot.frame().car_z;
    for frame in 0..(seconds / DT) as u64 {
        let mut s = show(level, heat);
        edit(frame, &mut s);
        let out = pilot.update(&PilotTick {
            dt: DT,
            beat: music(frame, 0.85),
            show: Some(s),
            settings: &settings,
            traffic: &traffic,
            overtakes: 0,
        });
        let a = out.animation;
        if a.airborne() {
            stats.airborne_frames += 1;
        }
        if was == FlightPhase::Ground && a.phase != FlightPhase::Ground {
            stats.takeoffs += 1;
        }
        was = a.phase;
        stats.max_altitude = stats.max_altitude.max(f64::from(a.altitude));
        stats.max_floor = stats.max_floor.max(a.floor);
        stats.max_abs_x = stats.max_abs_x.max(out.car_x.abs());
        if let Some(kind) = out.events.trick_started {
            stats.tricks.push(kind);
        }
        if frame % 60 == 0 {
            stats.trace.push((out.car_z, f64::from(a.altitude)));
        }
        stats.distance = start_z - out.car_z;
    }
    stats
}

#[test]
fn niveau_0_la_voiture_ne_vole_jamais() {
    for seed in 1..=4 {
        // Dix minutes, au plus fort de la chaleur : rien n'y fait, la porte `max_floor` est à 0.
        let s = simulate(seed, 0, 1.0, 600.0, |_, _| {});
        assert_eq!(s.airborne_frames, 0, "graine {seed}");
        assert!(s.max_altitude < f64::EPSILON && s.max_floor == 0 && s.tricks.is_empty());
        assert!(s.max_abs_x < 1e-9, "elle file droit au niveau 0 : {}", s.max_abs_x);
        assert!(s.distance > 5_000.0, "elle roule : {}", s.distance);
    }
}

#[test]
fn niveau_5_la_voiture_vole_et_fait_des_figures() {
    let s = simulate(7, 5, 0.9, 600.0, |_, _| {});
    assert!(s.takeoffs >= 3, "elle décolle plusieurs fois : {}", s.takeoffs);
    assert!(
        s.airborne_frames > 60 * 60,
        "au moins une minute en l'air : {}",
        s.airborne_frames
    );
    assert!(s.max_floor >= 4, "monte aux étages : {}", s.max_floor);
    assert!(s.max_altitude > 20.0, "{}", s.max_altitude);
    assert!(s.max_altitude <= 63.0, "jamais au-delà du plafond : {}", s.max_altitude);
    assert!(s.tricks.len() >= 5, "des figures : {:?}", s.tricks);
    assert!(
        s.tricks
            .iter()
            .any(|t| matches!(t, TrickKind::RollLeft | TrickKind::RollRight))
            && s.tricks
                .iter()
                .any(|t| matches!(t, TrickKind::Backflip | TrickKind::Frontflip)),
        "tonneaux et saltos : {:?}",
        s.tricks
    );
    assert!(
        s.max_abs_x > 1.0,
        "la ligne de course la fait changer de file : {}",
        s.max_abs_x
    );
    assert!(s.max_abs_x < 5.0, "dans la route accessible : {}", s.max_abs_x);
}

#[test]
fn le_plafond_de_vol_suit_la_porte_du_niveau() {
    // Niveau 3 : `max_floor` 4 ; la voiture ne dépasse jamais le quatrième étage (21 m) sauf par les esquives.
    let s = simulate(11, 3, 1.0, 900.0, |_, _| {});
    assert!(s.takeoffs >= 1);
    assert!(s.max_floor <= 4, "{}", s.max_floor);
}

#[test]
fn sans_la_porte_tricks_pas_de_figures() {
    let s = simulate(5, 5, 0.9, 600.0, |_, s| s.gates.tricks = false);
    assert!(s.airborne_frames > 0 && s.tricks.is_empty(), "{:?}", s.tricks);
}

#[test]
fn le_decollage_sur_drop_attend_le_drop() {
    let no_drop = simulate(3, 5, 0.9, 300.0, |_, s| s.gates.flight_on_drop_only = true);
    assert_eq!(no_drop.takeoffs, 0, "sans drop elle reste au sol");
    // Un drop frappe à 120 s : le décollage n'a lieu qu'à partir de cette image.
    let with_drop = simulate(3, 5, 0.9, 300.0, |frame, s| {
        s.gates.flight_on_drop_only = true;
        s.drop_now = frame == 7200;
    });
    assert!(with_drop.takeoffs >= 1, "décolle sur le drop");
    let first = with_drop
        .trace
        .iter()
        .position(|&(_, alt)| alt > 0.5)
        .expect("a décollé");
    assert!(first >= 120, "pas avant le drop : {first} s");
}

#[test]
fn la_respiration_pose_la_voiture_et_elle_ne_redecolle_pas() {
    let s = simulate(7, 5, 0.9, 400.0, |frame, s| s.breathing = frame >= 60 * 200);
    let breath_start = 200;
    assert!(
        s.trace[..breath_start].iter().any(|&(_, alt)| alt > 1.0),
        "elle volait avant"
    );
    assert!(
        s.trace[breath_start + 25..].iter().all(|&(_, alt)| alt < 0.06),
        "posée et sans redécoller : {:?}",
        &s.trace[breath_start + 25..breath_start + 45]
    );
}

#[test]
fn la_simulation_est_rejouable_et_la_graine_compte() {
    let a = simulate(42, 5, 0.9, 120.0, |_, _| {});
    let b = simulate(42, 5, 0.9, 120.0, |_, _| {});
    let c = simulate(43, 5, 0.9, 120.0, |_, _| {});
    assert_eq!(a.trace, b.trace);
    assert_eq!(a.tricks, b.tricks);
    assert!(a.trace != c.trace || a.tricks != c.tricks, "deux graines, deux vols");
}

#[test]
fn la_porte_cruise_change_l_allure() {
    let slow = simulate(1, 3, 0.5, 120.0, |_, s| s.gates.cruise = 0.7);
    let fast = simulate(1, 3, 0.5, 120.0, |_, s| s.gates.cruise = 1.15);
    assert!(fast.distance > slow.distance * 1.3, "{} {}", slow.distance, fast.distance);
}

#[test]
fn la_voiture_ne_traverse_jamais_le_tablier_d_une_arche() {
    // La voiture arrive à l'arche à toutes sortes d'étages, au fil de graines différentes.
    let first = cf_world_neon::route::bridge_z(0);
    let span = cf_world_neon::route::bridge_span();
    let traffic = quiet();
    let mut dodged = false;
    for seed in 0..12_u64 {
        let settings = settings(seed);
        let mut pilot = Pilot::new(seed, first + 1500.0);
        for frame in 0..(60 * 60) {
            let out = pilot.update(&PilotTick {
                dt: DT,
                beat: music(frame, 0.9),
                show: Some(show(5, 1.0)),
                settings: &settings,
                traffic: &traffic,
                overtakes: 0,
            });
            let in_deck = (out.car_z - first).abs() < span.half_depth + 2.0;
            let alt = f64::from(out.animation.altitude);
            // La voiture occupe [alt − 0,7 ; alt + 1,9] : elle traverse le tablier si les deux tranches se recouvrent.
            assert!(
                !(in_deck && alt + 1.9 > span.lo && alt - 0.7 < span.hi),
                "graine {seed} : traverse le tablier à {alt} m (image {frame})"
            );
            dodged |= out.animation.dodge != DodgeSide::None;
        }
    }
    assert!(dodged, "au moins une graine esquive l'arche");
}
