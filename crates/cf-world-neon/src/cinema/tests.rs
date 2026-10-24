//! Le réalisateur dans la vraie ville : 10 minutes simulées avec le pilote automatique (zigzags, envols, esquives) et
//! les immeubles de Neon City. Aucune pose dans un mur ni hors des bornes, coupes sur la grille de la section.

use bevy::math::DVec3;
use cf_analysis::{AudioFeatures as Features, Section};
use cf_cinema::{CAMERA_RADIUS, CinemaDirector, CinemaFrame, CinemaWorld, MAX_AHEAD, MAX_BEHIND, MAX_HEIGHT, shots};
use cf_render::CameraPose;
use cf_show::model::CutGrid;

use super::*;
use crate::pilot::{BeatInput, Pilot, PilotSettings, PilotTick, PilotTraffic};

const DT: f64 = 1.0 / 60.0;
const BPM: f64 = 128.0;

fn features(time: f64) -> Features {
    let beats = (time * BPM / 60.0).floor() as u32;
    // Sections de 40 s : calme, groove, sommet.
    let section = match (time / 40.0) as u32 % 3 {
        0 => Section::Calm,
        1 => Section::Groove,
        _ => Section::Peak,
    };
    Features {
        live: true,
        count: beats,
        bpm: Some(BPM as f32),
        pulse: (1.0 - (time * BPM / 60.0).fract() as f32 * 3.0).max(0.0),
        section,
        intensity: match section {
            Section::Calm => 0.25,
            Section::Groove => 0.55,
            Section::Peak => 0.9,
        },
        ..Features::NEUTRAL
    }
}

#[test]
fn dix_minutes_dans_neon_city_sans_pose_dans_un_mur() {
    let city = NeonWorld::default();
    let settings = PilotSettings::default();
    let traffic = PilotTraffic::default();
    let mut pilot = Pilot::new(settings.seed, 0.0);
    let specs = shots::catalog();
    let mut director = CinemaDirector::new(shots::catalog(), false, 0xC1_7E);
    let mut world = CinemaWorld::default();
    let mut time = 0.0;
    let mut previous_section = Section::Calm;
    let (mut flights, mut shots_seen, mut on_grid) = (0_u32, 0_u32, 0_u32);
    let mut was_flying = false;
    let mut started = false;
    while time < 600.0 {
        time += DT;
        let f = features(time);
        let frame = pilot.update(&PilotTick {
            dt: DT,
            beat: BeatInput {
                kick: f.count != features(time - DT).count,
                ..BeatInput::from(&f)
            },
            show: None,
            settings: &settings,
            traffic: &traffic,
            overtakes: 0,
        });
        let car = DVec3::new(frame.car_x, f64::from(frame.animation.altitude), frame.car_z);
        fill_cinema_world(
            &mut world,
            &CinemaInput {
                time,
                dt: DT,
                car,
                speed: frame.speed,
                animation: &frame.animation,
                features: &f,
                show: None,
                ring: None,
            },
        );
        // Un spectacle actif (grille 16 / 8 / 4), drop au début de chaque sommet.
        world.show.active = true;
        world.show.cut_grid = CutGrid {
            calm: 16,
            groove: 8,
            peak: 4,
        };
        world.drop = f.section == Section::Peak && previous_section != Section::Peak;
        world.show.drop_now = world.drop;
        previous_section = f.section;

        let view = CinemaFrame::new(&world, &city, &[]);
        if !started {
            let from = CameraPose {
                position: car + DVec3::new(0.0, 3.0, 8.0),
                ..CameraPose::default()
            };
            director.start(&from, &view);
            shots_seen = director.stats().shots;
            started = true;
        }
        let pose = *director.update(&view);
        let p = pose.position;
        assert!(p.is_finite() && pose.rotation.is_finite(), "pose non finie à t={time}");
        assert!(
            !city.collides(p, CAMERA_RADIUS),
            "pose dans un mur à t={time:.2} : {p} (voiture {car})"
        );
        assert!(
            p.z >= car.z - MAX_AHEAD - 1e-6 && p.z <= car.z + MAX_BEHIND + 1e-6,
            "hors bornes à t={time}"
        );
        assert!(p.y <= view.ground() + MAX_HEIGHT + 1e-6, "trop haut à t={time}");

        let flying = frame.animation.airborne();
        flights += u32::from(flying && !was_flying);
        was_flying = flying;
        let stats = director.stats();
        if stats.shots != shots_seen {
            shots_seen = stats.shots;
            // Un plan réactif (décollage, atterrissage, esquive) bascule dès le début de l'animation de la voiture, et un
            // plan d'occasion (figure, cascade) dès le prochain temps : ni l'un ni l'autre n'attend la grille (J6-04).
            let on_the_spot = specs
                .iter()
                .find(|spec| Some(spec.id()) == director.shot_id())
                .is_some_and(|spec| {
                    spec.reactive() || spec.opportunity(&view) > cf_cinema::editor::OPPORTUNITY_THRESHOLD
                });
            if stats.last_mode == Some(cf_cinema::editor::PickMode::Normal) && !on_the_spot {
                let grid = u32::from(world.show.grid_for(world.section));
                assert!(stats.last_on_beat, "coupe hors temps à t={time}");
                assert_eq!((f.count % 16) % grid, 0, "coupe hors grille ({grid} temps) à t={time}");
                on_grid += 1;
            }
        }
    }
    let stats = director.stats();
    assert!(
        flights >= 1,
        "la voiture n'a jamais volé : le test ne couvre pas les envols"
    );
    assert!(on_grid >= 30, "{on_grid} coupes sur la grille");
    assert_eq!(stats.fallbacks, 0, "{stats:?}");
}

#[test]
fn le_monde_du_realisateur_suit_la_voiture() {
    let animation = CarAnimation {
        altitude: 12.0,
        phase: FlightPhase::Flying,
        dodge: DodgeSide::Over,
        ..CarAnimation::default()
    };
    let mut world = CinemaWorld::default();
    let f = Features {
        live: true,
        section: Section::Peak,
        ..Features::NEUTRAL
    };
    let mut input = CinemaInput {
        time: 1.0,
        dt: 0.5,
        car: DVec3::new(0.0, 12.0, -100.0),
        speed: 40.0,
        animation: &animation,
        features: &f,
        show: None,
        ring: None,
    };
    fill_cinema_world(&mut world, &input);
    assert_eq!(world.section_changed, None, "pas de changement à la première image");
    assert!((world.dt - MAX_DT).abs() < 1e-6, "pas de temps borné");
    assert_eq!(world.animation.phase, CinemaPhase::Flying);
    assert_eq!(world.animation.dodge, 1);
    assert!((world.altitude - 12.0).abs() < 1e-6);
    input.time = 1.02;
    input.dt = 0.02;
    input.car = DVec3::new(0.0, 12.0, -100.8);
    let groove = Features {
        section: Section::Groove,
        ..f
    };
    input.features = &groove;
    fill_cinema_world(&mut world, &input);
    assert_eq!(world.section_changed, Some(Section::Groove));
    assert!((world.car_velocity.z + 40.0).abs() < 1e-3, "{}", world.car_velocity);
    assert!((world.cabin.forward - Vec3::NEG_Z).length() < 1e-4);
    assert!(!world.show.active);
}
