//! Le réalisateur sur 10 minutes simulées : coupes sur la grille de la section, jamais de pose dans un mur ni hors des
//! bornes. La ville est une ville de boîtes déterministe, à la géométrie de Sillage (chaussée |x| < 9, immeubles à
//! partir de |x| > 11, quelques tours qui débordent jusqu'au bord de la chaussée).

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;
use cf_show::model::CutGrid;

use super::*;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::shots::Chase;
use crate::types::{FlightPhase, Section, ShotTier, ShowView, TransitionKind};
use crate::world::{CinemaFrame, CinemaWorld, NoObstacles, Obstacles};

pub(super) const DT: f32 = 1.0 / 60.0;
const BPM: f64 = 128.0;
const TEN_MINUTES: f64 = 600.0;
/// Longueur d'une rangée d'immeubles (m).
const ROW: f64 = 24.0;

/// Ville de boîtes : à chaque rangée, une tour de chaque côté, de hauteur et de retrait tirés par graine.
pub(super) struct BoxCity;

fn hash(i: i64) -> f64 {
    let mut rng = CinemaRng::new((i as u32).wrapping_mul(0x9e37_79b9) ^ 0x1234_5678);
    rng.next_f64()
}

impl Obstacles for BoxCity {
    fn collides(&self, at: DVec3, radius: f64) -> bool {
        let reach = radius * 0.7;
        let row = (at.z / ROW).floor() as i64;
        for index in row - 1..=row + 1 {
            for side in [-1.0, 1.0] {
                let h = hash(index * 2 + i64::from(side > 0.0));
                // Retrait de 9,5 à 14 m, hauteur de 8 à 90 m ; une rangée sur six est vide (carrefour).
                if h < 1.0 / 6.0 {
                    continue;
                }
                let inner = 9.5 + 4.5 * h;
                let top = 8.0 + 82.0 * hash(index * 7 + 3);
                let (x0, x1) = if side < 0.0 {
                    (-inner - 20.0, -inner)
                } else {
                    (inner, inner + 20.0)
                };
                let (z0, z1) = (index as f64 * ROW + 2.0, (index + 1) as f64 * ROW - 2.0);
                if at.y < top + reach
                    && at.x > x0 - reach
                    && at.x < x1 + reach
                    && at.z > z0 - reach
                    && at.z < z1 + reach
                {
                    return true;
                }
            }
        }
        false
    }
}

/// Une soirée simulée : la voiture file à 40 m/s en zigzag et vole parfois, les kicks tombent à 128 BPM, les
/// sections tournent (calme → groove → sommet) toutes les 40 s avec un drop à chaque sommet.
pub(super) struct Sim {
    pub(super) world: CinemaWorld,
    next_beat: f64,
    bpm: f64,
    section_at: f64,
}

impl Sim {
    pub(super) fn new(show: bool) -> Self {
        Self::with_bpm(show, BPM)
    }

    pub(super) fn with_bpm(show: bool, bpm: f64) -> Self {
        let mut world = CinemaWorld {
            live: true,
            dt: DT,
            speed: 40.0,
            car: DVec3::new(0.0, 0.0, -10_000.0),
            ..CinemaWorld::default()
        };
        world.beat.bpm = Some(bpm as f32);
        world.show = ShowView {
            active: show,
            cut_grid: CutGrid {
                calm: 16,
                groove: 8,
                peak: 4,
            },
            ..ShowView::default()
        };
        Self {
            world,
            next_beat: 60.0 / bpm,
            bpm,
            section_at: 0.0,
        }
    }

    pub(super) fn step(&mut self) {
        let w = &mut self.world;
        w.time += f64::from(DT);
        let t = w.time;
        w.drop = false;
        w.show.drop_now = false;
        w.section_changed = None;
        if t >= self.next_beat {
            self.next_beat += 60.0 / self.bpm;
            w.beat.count += 1;
            w.beat.pulse = 1.0;
        } else {
            w.beat.pulse *= 0.9;
        }
        if t - self.section_at >= 40.0 {
            self.section_at = t;
            w.section = match w.section {
                Section::Calm => Section::Groove,
                Section::Groove => Section::Peak,
                Section::Peak => Section::Calm,
            };
            w.section_changed = Some(w.section);
            if w.section == Section::Peak {
                w.drop = true;
                w.show.drop_now = true;
            }
        }
        w.intensity = match w.section {
            Section::Calm => 0.25,
            Section::Groove => 0.55,
            Section::Peak => 0.9,
        };
        // Zigzag sur la chaussée, envol de 30 m une minute sur trois.
        w.car.z -= f64::from(w.speed * DT);
        w.car.x = 6.0 * (t * 0.3).sin();
        let flying = (t / 60.0) as u32 % 3 == 2;
        let target = if flying { 30.0 } else { 0.0 };
        let previous = w.altitude;
        w.altitude += (target - w.altitude) * crate::pose::follow(DT, 0.8);
        w.animation.climb = (w.altitude - previous) / DT;
        w.animation.phase = if w.altitude > 1.0 {
            FlightPhase::Flying
        } else {
            FlightPhase::Ground
        };
        w.car.y = f64::from(w.altitude);
        w.car_velocity = Vec3::new(1.8 * (t * 0.3).cos() as f32, w.animation.climb, -w.speed);
    }
}

/// Vérifie une pose : finie, hors des murs, dans les bornes.
fn check_pose(pose: &CameraPose, frame: &CinemaFrame<'_>) {
    let p = pose.position;
    assert!(crate::pose::is_finite(pose), "pose non finie à t={}", frame.time);
    assert!(
        !frame.collides(p, CAMERA_RADIUS),
        "pose dans un mur à t={} : {p}",
        frame.time
    );
    assert!(
        p.z >= frame.car.z - MAX_AHEAD - 1e-6 && p.z <= frame.car.z + MAX_BEHIND + 1e-6,
        "hors bornes : {p}"
    );
    assert!(p.y <= frame.ground() + MAX_HEIGHT + 1e-6, "trop haut : {p}");
}

#[test]
fn dix_minutes_coupes_sur_la_grille_et_aucune_pose_dans_un_mur() {
    let mut sim = Sim::new(true);
    let mut director = CinemaDirector::new(vec![Box::new(Chase), Box::new(Chase)], false, 42);
    let city = BoxCity;
    let start = CameraPose {
        position: sim.world.car + DVec3::new(0.0, 3.0, 8.0),
        ..CameraPose::default()
    };
    director.start(&start, &CinemaFrame::new(&sim.world, &city, &[]));
    let (mut shots, mut on_grid, mut drops) = (director.stats().shots, 0, 0);
    while sim.world.time < TEN_MINUTES {
        sim.step();
        let frame = CinemaFrame::new(&sim.world, &city, &[]);
        let pose = *director.update(&frame);
        check_pose(&pose, &frame);
        let stats = director.stats();
        if stats.shots == shots {
            continue;
        }
        shots = stats.shots;
        match stats.last_mode {
            Some(PickMode::Drop) => {
                assert!(frame.drop, "coupe de drop sans drop à t={}", frame.time);
                drops += 1;
            }
            Some(PickMode::Normal) => {
                assert!(stats.last_on_beat, "coupe hors temps à t={}", frame.time);
                let grid = u32::from(frame.show.grid_for(frame.section));
                let beat_in_phrase = frame.beat.count % crate::cues::PHRASE_BEATS;
                assert_eq!(
                    beat_in_phrase % grid,
                    0,
                    "coupe hors grille ({grid} temps) à t={}",
                    frame.time
                );
                assert_eq!(stats.last_beat_no % grid, 0);
                on_grid += 1;
            }
            other => unreachable!("tirage inattendu {other:?} à t={}", frame.time),
        }
    }
    // ~600 s à des plans de 7 à 21 s : plusieurs dizaines de coupes, un drop par sommet après la garde de 6 s.
    assert!(on_grid >= 30, "{on_grid} coupes seulement");
    assert!(drops >= 3, "{drops} coupes de drop");
    let stats = director.stats();
    // La poursuite se dégage seule des façades : le dernier bon cadrage n'a jamais servi.
    assert_eq!(stats.fallbacks, 0, "{stats:?}");
}

#[test]
fn hors_spectacle_on_coupe_sur_la_phrase() {
    let mut sim = Sim::new(false);
    let mut director = CinemaDirector::new(vec![Box::new(Chase)], false, 7);
    director.start(&CameraPose::default(), &CinemaFrame::new(&sim.world, &NoObstacles, &[]));
    let mut shots = director.stats().shots;
    let mut cuts = 0;
    while sim.world.time < 240.0 {
        sim.step();
        let frame = CinemaFrame::new(&sim.world, &NoObstacles, &[]);
        director.update(&frame);
        let stats = director.stats();
        if stats.shots != shots {
            shots = stats.shots;
            if stats.last_mode == Some(PickMode::Normal) {
                assert_eq!(frame.beat.count % crate::cues::PHRASE_BEATS, 0, "t={}", frame.time);
                cuts += 1;
            }
        }
    }
    assert!(cuts >= 5, "{cuts}");
}

/// Un plan qui se pose dans un immeuble, puis produit un NaN.
#[derive(Debug)]
struct Faulty;

struct FaultyShot {
    frames: u32,
}

impl Shot for FaultyShot {
    fn update(&mut self, w: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.frames += 1;
        out.position = if self.frames < 400 {
            // En plein dans la tour de droite, 30 m de haut.
            DVec3::new(25.0, 3.0, w.car.z - 20.0)
        } else {
            DVec3::NAN
        };
        true
    }
}

impl ShotSpec for Faulty {
    fn id(&self) -> &'static str {
        "faulty"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        60.0
    }
    fn max_seconds(&self) -> f32 {
        90.0
    }
    fn cooldown(&self) -> f32 {
        0.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Cut
    }
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        true
    }
    fn create(&self, _frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(FaultyShot { frames: 0 })
    }
}

/// Un mur partout à droite de x = 11, jusqu'à 200 m de haut : indégageable en six pas.
struct WallRight;

impl Obstacles for WallRight {
    fn collides(&self, at: DVec3, radius: f64) -> bool {
        at.x + radius * 0.7 > 11.0 && at.y < 200.0
    }
}

#[test]
fn mur_et_nan_retombent_sur_le_dernier_bon_cadrage() {
    let mut sim = Sim::new(false);
    sim.world.live = false;
    let mut director = CinemaDirector::new(vec![Box::new(Faulty)], false, 1);
    let start = CameraPose {
        position: sim.world.car + DVec3::new(0.0, 2.0, 6.0),
        ..CameraPose::default()
    };
    director.start(&start, &CinemaFrame::new(&sim.world, &WallRight, &[]));
    for _ in 0..600 {
        sim.step();
        let frame = CinemaFrame::new(&sim.world, &WallRight, &[]);
        let pose = *director.update(&frame);
        assert!(crate::pose::is_finite(&pose));
        assert!(!frame.collides(pose.position, CAMERA_RADIUS), "{}", pose.position);
    }
    let stats = director.stats();
    assert!(stats.wall_saves > 0 && stats.fallbacks > 0, "{stats:?}");
    // Le plan fautif a été relancé après son NaN (fin de plan), pas gardé.
    assert!(stats.shots >= 2, "{stats:?}");
}

#[test]
fn le_degagement_sauve_une_pose_au_bord_d_un_toit() {
    let mut director = CinemaDirector::new(vec![Box::new(Chase)], false, 3);
    let world = CinemaWorld::default();
    let city = BoxCity;
    let frame = CinemaFrame::new(&world, &city, &[]);
    director.start(&CameraPose::default(), &frame);
    // Une pose juste dans la façade de gauche d'une rangée pleine : on la rapproche de l'axe et on la monte.
    let row = (0..100).find(|i| hash(i * 2) >= 1.0 / 6.0).unwrap_or(0);
    let inner = 9.5 + 4.5 * hash(row * 2);
    director.out.position = DVec3::new(-inner - 0.5, 2.0, row as f64 * ROW + ROW / 2.0);
    assert!(frame.collides(director.out.position, CAMERA_RADIUS));
    director.avoid_walls(&frame);
    assert!(
        !frame.collides(director.out.position, CAMERA_RADIUS),
        "{}",
        director.out.position
    );
    assert_eq!(director.stats().wall_saves, 1);
}
