//! Aucune allocation par image dans la vie d'un événement (règle 4), pour les 23 événements des lots A (J7-04) et B (J7-05) :
//! chacun est fabriqué hors mesure (sa fabrique alloue une fois, à son démarrage : la boîte, ses tampons de capacité fixe),
//! puis son `update` tourne image par image sous `assert_no_alloc`, qui remplace l'allocateur de ce binaire de test et fait
//! avorter le processus à la moindre allocation. Les points lumineux, les panneaux, les projecteurs et la flotte sont ceux du
//! directeur : préalloués.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_world_neon::pilot::rand::Rand;
use cf_world_neon::traffic::events::{DEFAULT_TITLE, calm_factories};
use cf_world_neon::traffic::grand::grand_factories;
use cf_world_neon::traffic::glints::Glints;
use cf_world_neon::traffic::walls::LightWalls;
use cf_world_neon::traffic::{
    EventCtx, EventName, EventRegistry, Fleet, Scene, TrafficBeat, TrafficFrame,
};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

const DT: f32 = 1.0 / 60.0;
/// Durée mesurée de chaque événement, en secondes simulées : de quoi voir naître et mourir presque tous.
const SECONDS: usize = 40;

fn cycle() -> [LinearRgba; 6] {
    [
        LinearRgba::rgb(1.0, 0.2, 0.4),
        LinearRgba::rgb(0.2, 1.0, 0.6),
        LinearRgba::rgb(0.3, 0.4, 1.0),
        LinearRgba::rgb(1.0, 0.8, 0.2),
        LinearRgba::rgb(0.8, 0.3, 1.0),
        LinearRgba::rgb(0.2, 0.9, 1.0),
    ]
}

/// L'image `i` : la voiture roule à 30 m/s à 400 km du départ, un kick toutes les 28 images, un drop toutes les 10 s.
fn frame_at(i: usize) -> TrafficFrame<'static> {
    let time = i as f64 * f64::from(DT);
    TrafficFrame {
        dt: DT,
        time,
        car: DVec3::new(0.0, 0.0, -400_000.0 - time * 30.0),
        speed: 30.0,
        beat: TrafficBeat {
            kick: i % 28 == 0,
            bass: 0.7,
            mid: 0.5,
            pulse: if i % 28 < 7 { 1.0 - (i % 28) as f32 / 7.0 } else { 0.0 },
            count: (i / 28) as u32,
            beat: i as f64 / 28.0,
            ..TrafficBeat::SILENT
        },
        drive: cf_world_neon::traffic::TrafficDrive {
            drop: i % 600 == 599,
            intensity: 0.8,
            ..cf_world_neon::traffic::TrafficDrive::START
        },
        dance_beat: (i / 28) as f64,
        cycle: cycle(),
        ..TrafficFrame::default()
    }
}

#[test]
fn la_vie_de_chaque_evenement_n_alloue_jamais() {
    let mut registry = EventRegistry::default();
    for (name, build) in calm_factories(DEFAULT_TITLE).into_iter().chain(grand_factories(None)) {
        registry.register(name, build);
    }
    for name in EventName::ALL {
        let mut fleet = Fleet::new();
        let mut glints = Glints::new();
        let mut walls = LightWalls::new();
        let mut scene = Scene::new();
        let mut rng = Rand::new(0xA110C);
        let first = frame_at(0);
        // Fabrication : hors mesure.
        let mut event = {
            let mut ctx = EventCtx {
                frame: &first,
                fleet: &mut fleet,
                glints: &mut glints,
                walls: &mut walls,
                rng: &mut rng,
                scene: &mut scene,
                signs: None,
            };
            registry.create(name, &mut ctx).expect("inscrit")
        };
        let mut alive = true;
        for i in 1..(SECONDS * 60) {
            if !alive {
                break;
            }
            let frame = frame_at(i);
            assert_no_alloc(|| {
                glints.begin(frame.car);
                scene.begin(frame.car);
                let mut ctx = EventCtx {
                    frame: &frame,
                    fleet: &mut fleet,
                    glints: &mut glints,
                    walls: &mut walls,
                    rng: &mut rng,
                    scene: &mut scene,
                    signs: None,
                };
                alive = event.update(DT, &mut ctx);
                fleet.update(DT, frame.car.z, |_| true);
                walls.update(frame.time);
            });
        }
        // La fin rend tout (hors mesure : elle peut vider des tampons).
        let last = frame_at(SECONDS * 60);
        let mut ctx = EventCtx {
            frame: &last,
            fleet: &mut fleet,
            glints: &mut glints,
            walls: &mut walls,
            rng: &mut rng,
            scene: &mut scene,
            signs: None,
        };
        event.end(&mut ctx);
        assert_eq!(
            fleet.crafts().iter().filter(|c| c.active).count(),
            0,
            "{name:?} : véhicules non rendus"
        );
    }
}
