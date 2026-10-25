//! Aucune allocation par image en régime établi (règle 4) : le directeur des cascades tourne sous `assert_no_alloc`, qui
//! remplace l'allocateur de ce binaire de test et fait avorter le processus à la moindre allocation. Le régime couvre
//! l'attente, le tirage, le départ, le déroulé, le fondu de la pose sur l'animation, la fin et l'attente suivante, sur
//! une demi-heure simulée, avec le monde réel (flotte du trafic).
//!
//! La cascade de test est de taille nulle : `Box::new` d'un type de taille nulle n'alloue rien. Une vraie cascade alloue
//! une fois à son départ (le `Box` de son déroulé, voir la documentation du module `stunts`), jamais par image.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_show::model::StuntTier as ShowStuntTier;
use cf_world_neon::CarAnimation;
use cf_world_neon::pilot::rand::Rand;
use cf_world_neon::stunts::{
    NoLink, Planned, StuntAlign, StuntCtx, StuntDirector, StuntEnv, StuntFrame, StuntKind, StuntPose, StuntRegistry,
    StuntRun, StuntScript, StuntShow, StuntTier, StuntWorld, TrafficStuntWorld,
};
use cf_world_neon::traffic::{TrafficBeat, TrafficDirector};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f64 = 1.0 / 60.0;

/// Une cascade de taille nulle : un saut de 5 m avec un salto, sans véhicule.
struct Hop(StuntKind);

/// Son déroulé : de taille nulle (le script, lui, porte son genre).
struct HopRun;

impl StuntRun for HopRun {
    fn duration(&self) -> f64 {
        5.0
    }

    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let u = (t / 5.0).clamp(0.0, 1.0);
        out.x = ctx.env.car_x + 3.0 * (u * std::f64::consts::PI).sin();
        out.altitude = 5.0 * (u * std::f64::consts::PI).sin();
        out.pitch = std::f64::consts::TAU * u;
        out.hover = true;
        t < 5.0
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}

impl StuntScript for Hop {
    fn kind(&self) -> StuntKind {
        self.0
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Common
    }
    fn weight(&self) -> f64 {
        1.0
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Bar
    }
    fn available(&self, _env: &StuntEnv<'_>, _world: &dyn StuntWorld) -> bool {
        true
    }
    fn begin(&self, _ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        Box::new(HopRun)
    }
}

#[test]
fn le_regime_etabli_n_alloue_jamais() {
    let mut registry = StuntRegistry::default();
    for kind in [
        StuntKind::Slalom,
        StuntKind::NearMiss,
        StuntKind::OffRoad,
        StuntKind::Loop,
    ] {
        registry.register(Box::new(Hop(kind)));
    }
    let mut gates = cf_show::gates::open_gates();
    gates.max_floor = 4;
    gates.stunt_tiers = vec![ShowStuntTier::new("common").unwrap()];
    let mut director = StuntDirector::new(5);
    let mut traffic = TrafficDirector::new(5);
    let mut rng = Rand::new(9);
    let mut link = NoLink;
    let mut started = 0u32;
    let mut last_active = false;

    let mut run = |i: u32| {
        let time = f64::from(i) * DT;
        let frame = StuntFrame {
            dt: DT,
            now: time,
            enabled: true,
            car_x: 0.0,
            car_z: -400_000.0 - time * 60.0,
            altitude: 0.0,
            road_speed: 60.0,
            beat: TrafficBeat {
                kick: i.is_multiple_of(30),
                count: i / 30,
                bpm: Some(120.0),
                ..TrafficBeat::SILENT
            },
            drop: false,
            intensity: 0.5,
            show: Some(StuntShow {
                gates: &gates,
                breathing: false,
                drop_now: false,
                planned: Planned::None,
            }),
        };
        let mut world = TrafficStuntWorld {
            traffic: &mut traffic,
            city: None,
            cycle: [bevy::color::LinearRgba::WHITE; 6],
            rng: &mut rng,
        };
        let x = director.update(&frame, &registry, &mut world, &mut link);
        let mut animation = CarAnimation::default();
        director.apply(&mut animation);
        assert!(x.is_finite() && animation.altitude.is_finite());
        let active = director.active();
        if active && !last_active {
            started += 1;
        }
        last_active = active;
    };

    // Chauffe (hors mesure) : le premier tirage et les tampons.
    for i in 0..600 {
        run(i);
    }
    assert_no_alloc(|| {
        for i in 600..(600 + 30 * 60 * 60) {
            run(i);
        }
    });
    assert!(started > 20, "des cascades ont bien eu lieu : {started}");
}
