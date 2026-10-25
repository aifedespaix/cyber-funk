//! Aucune allocation par image en régime établi (règle 4) : le directeur du trafic tourne sous `assert_no_alloc`, qui
//! remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime couvre le
//! trafic de fond, le trafic au sol, le tirage et la vie des événements, les points lumineux et les murs de lumière.
//!
//! L'événement de test est de taille nulle : `Box::new` d'un type de taille nulle n'alloue rien. Un vrai événement alloue
//! une fois à son démarrage (voir la documentation du module `traffic`), jamais par image.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_show::model::EventName as ShowEventName;
use cf_world_neon::traffic::scheduler::NoShow;
use cf_world_neon::traffic::{
    EventCtx, EventName, EventRegistry, TrafficBeat, TrafficDirector, TrafficEvent, TrafficFrame, TrafficShow,
};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f32 = 1.0 / 60.0;

/// Un événement de taille nulle : il pose un mur de lumière et des points, et finit quand l'horloge passe un multiple de 25 s.
#[derive(Debug)]
struct Probe;

impl TrafficEvent for Probe {
    fn update(&mut self, _dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        let car = ctx.frame.car;
        ctx.glints
            .add(car + DVec3::new(0.0, 6.0, -30.0), 1.5, LinearRgba::WHITE, 2.0);
        (ctx.frame.time / 25.0).floor() as i64 == ((ctx.frame.time - f64::from(ctx.frame.dt)) / 25.0).floor() as i64
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}

fn frame_at(i: u32, slugs: &[ShowEventName], cycle: [LinearRgba; 6]) -> TrafficFrame<'_> {
    let time = f64::from(i) * f64::from(DT);
    TrafficFrame {
        dt: DT,
        time,
        // La voiture roule à 60 m/s, à 400 km du départ.
        car: DVec3::new(0.0, 0.0, -400_000.0 - time * 60.0),
        beat: TrafficBeat {
            kick: i.is_multiple_of(28),
            bass: 0.6,
            mid: 0.5,
            ..TrafficBeat::SILENT
        },
        dance_beat: f64::from(i / 28),
        cycle,
        show: Some(TrafficShow {
            level: 3,
            breathing: false,
            flashes: true,
            ground_traffic: 1.0,
            event_rate: 4.0,
            events: slugs,
            city_pulse: 0.0,
            titles: false,
            dj_seq: 0,
            level_seq: 0,
        }),
        ..TrafficFrame::default()
    }
}

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let slugs: Vec<ShowEventName> = [EventName::Fish, EventName::Lanterns, EventName::Refuel]
        .iter()
        .map(|n| ShowEventName::new(n.as_str()).expect("slug"))
        .collect();
    let mut registry = EventRegistry::default();
    for name in [EventName::Fish, EventName::Lanterns, EventName::Refuel] {
        registry.register(name, Box::new(|_| Box::new(Probe)));
    }
    let mut director = TrafficDirector::new(5);
    let mut link = NoShow;
    let mut cycle = [LinearRgba::WHITE; 6];
    cycle[1] = LinearRgba::rgb(0.1, 0.8, 1.0);

    // Échauffement hors zone : les tampons grandissent une fois (les événements démarrent, leur boîte est de taille nulle).
    let warmup = 60 * 90;
    for i in 0..warmup {
        director.update(&frame_at(i, &slugs, cycle), &mut registry, &mut link, |_| true);
    }
    let mut started = 0;
    for i in warmup..(warmup + 60 * 240) {
        let frame = frame_at(i, &slugs, cycle);
        let before = director.active_count();
        assert_no_alloc(|| {
            director.update(&frame, &mut registry, &mut link, |_| true);
        });
        started += usize::from(director.active_count() > before);
    }
    assert!(started >= 2, "des événements ont démarré pendant la mesure : {started}");
    assert!(director.fleet.stats().stepped_total > 10_000, "le trafic a vécu");
    assert!(director.ground.overtakes > 0, "le trafic au sol aussi");
}
