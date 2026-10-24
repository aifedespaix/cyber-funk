//! La densité de Sillage et ce qu'elle coûte : cinq minutes de trafic (fond, sol, murs, points) à 60 i/s, sans rendu.
//! Le test mesure le nombre de véhicules vivants (moyenne, crête) et le temps du pas du trafic par image, et vérifie
//! qu'il tient très largement dans le budget d'une image à 60 i/s (16,6 ms ; le trafic n'en prend qu'une part infime).
//!
//! La mesure est affichée (`cargo test -p cf-world-neon --test traffic_density -- --nocapture`) : c'est la preuve de la
//! PR. Le GPU (draw calls, ms GPU) n'est pas mesuré ici, faute de carte graphique en session cloud.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::time::Instant;

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_show::model::EventName as ShowEventName;
use cf_world_neon::traffic::scheduler::NoShow;
use cf_world_neon::traffic::{EventName, EventRegistry, TrafficBeat, TrafficDirector, TrafficFrame, TrafficShow};

const DT: f32 = 1.0 / 60.0;

fn frame(i: u32, slugs: &[ShowEventName], density: f32) -> TrafficFrame<'_> {
    let time = f64::from(i) * f64::from(DT);
    TrafficFrame {
        dt: DT,
        time,
        car: DVec3::new(0.0, 0.0, -400_000.0 - time * 60.0),
        beat: TrafficBeat {
            kick: i.is_multiple_of(28),
            bass: 0.7,
            mid: 0.6,
            ..TrafficBeat::SILENT
        },
        dance_beat: f64::from(i / 28),
        cycle: [LinearRgba::WHITE; 6],
        density,
        show: Some(TrafficShow {
            level: 5,
            breathing: false,
            flashes: true,
            ground_traffic: 1.0,
            event_rate: 1.0,
            events: slugs,
            city_pulse: 0.0,
            titles: false,
            dj_seq: 0,
            level_seq: 0,
        }),
        ..TrafficFrame::default()
    }
}

/// Rejoue cinq minutes ; rend (moyenne de véhicules vivants, crête, temps moyen par image en µs, pire image en µs).
fn run(density: f32) -> (f64, u32, f64, f64) {
    let slugs: Vec<ShowEventName> = EventName::ALL
        .iter()
        .map(|n| ShowEventName::new(n.as_str()).expect("slug"))
        .collect();
    let mut registry = EventRegistry::default();
    let mut director = TrafficDirector::new(7);
    let (mut total, mut peak, mut spent, mut worst) = (0_u64, 0_u32, 0.0_f64, 0.0_f64);
    let frames = 60 * 300;
    for i in 0..frames {
        let f = frame(i, &slugs, density);
        let start = Instant::now();
        director.update(&f, &mut registry, &mut NoShow, |_| true);
        let micros = start.elapsed().as_secs_f64() * 1e6;
        spent += micros;
        worst = worst.max(micros);
        let active = director.fleet.stats().active;
        total += u64::from(active);
        peak = peak.max(active);
    }
    (total as f64 / f64::from(frames), peak, spent / f64::from(frames), worst)
}

#[test]
fn la_densite_de_sillage_tient_dans_le_budget_d_une_image_a_60_ips() {
    let (average, peak, micros, worst) = run(1.0);
    println!(
        "trafic, densité 1,0 : {average:.1} véhicules vivants en moyenne, crête {peak}, {micros:.1} µs par image (pire {worst:.0} µs)"
    );
    assert!(average > 8.0, "la ville est vivante : {average}");
    assert!(peak <= 212, "le pool tient : {peak}");
    // Budget d'une image à 60 i/s : 16 600 µs. Le trafic doit en prendre moins de 3 % en moyenne.
    assert!(micros < 500.0, "{micros} µs par image");

    let (reduced, _, _, _) = run(0.4);
    println!("trafic, densité 0,4 : {reduced:.1} véhicules vivants en moyenne");
    assert!(reduced < average, "la qualité dégradée allège le trafic");
}
