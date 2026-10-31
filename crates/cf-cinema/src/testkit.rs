//! Infrastructure de test du réalisateur, partagée par les tests du réalisateur (`director/tests.rs`) et ceux des plans
//! (`shots/`) : une ville de boîtes déterministe, une soirée simulée, et la vérification d'une pose.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;
use cf_show::model::CutGrid;

use crate::director::{CAMERA_RADIUS, MAX_AHEAD, MAX_BEHIND, MAX_HEIGHT, WIDE_AHEAD, WIDE_BEHIND, WIDE_HEIGHT};
use crate::rng::CinemaRng;
use crate::types::{FlightPhase, Section, ShowView};
use crate::world::{CinemaFrame, CinemaWorld, Obstacles};

pub(crate) const DT: f32 = 1.0 / 60.0;
pub(crate) const BPM: f64 = 128.0;
pub(crate) const TEN_MINUTES: f64 = 600.0;
/// Longueur d'une rangée d'immeubles (m).
pub(crate) const ROW: f64 = 24.0;

/// Ville de boîtes : à chaque rangée, une tour de chaque côté, de hauteur et de retrait tirés par graine.
pub(crate) struct BoxCity;

pub(crate) fn hash(i: i64) -> f64 {
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
pub(crate) struct Sim {
    pub(crate) world: CinemaWorld,
    next_beat: f64,
    bpm: f64,
    section_at: f64,
}

impl Sim {
    pub(crate) fn new(show: bool) -> Self {
        Self::with_bpm(show, BPM)
    }

    pub(crate) fn with_bpm(show: bool, bpm: f64) -> Self {
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

    pub(crate) fn step(&mut self) {
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
pub(crate) fn check_pose(pose: &CameraPose, frame: &CinemaFrame<'_>) {
    check_pose_in(pose, frame, false);
}

/// Comme [`check_pose`], avec les bornes de la vue large quand `wide` (plans qui demandent `ShotSpec::wide_view`).
pub(crate) fn check_pose_in(pose: &CameraPose, frame: &CinemaFrame<'_>, wide: bool) {
    let (behind, ahead, height) = if wide {
        (WIDE_BEHIND, WIDE_AHEAD, WIDE_HEIGHT)
    } else {
        (MAX_BEHIND, MAX_AHEAD, MAX_HEIGHT)
    };
    let p = pose.position;
    assert!(crate::pose::is_finite(pose), "pose non finie à t={}", frame.time);
    assert!(
        !frame.collides(p, CAMERA_RADIUS),
        "pose dans un mur à t={} : {p}",
        frame.time
    );
    assert!(
        p.z >= frame.car.z - ahead - 1e-6 && p.z <= frame.car.z + behind + 1e-6,
        "hors bornes : {p}"
    );
    assert!(p.y <= frame.ground() + height + 1e-6, "trop haut : {p}");
}
