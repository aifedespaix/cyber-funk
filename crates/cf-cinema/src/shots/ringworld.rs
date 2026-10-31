//! La séquence de la géante à anneau (`cinema/shots/ringworld.ts`) : on monte au-dessus de la ville, on voyage jusqu'à la
//! planète, on survole son anneau, on en fait le tour, puis on replonge. Aucune lumière touchée (règle 7).
//!
//! # Le voyage est réel, la planète reste à l'infini
//!
//! La planète vit dans la couche du ciel : on ne s'en approche qu'en décalant le ciel (`sky_drift`). Pour que ce soit la
//! **caméra** qui parte, elle décrit en plus un vrai cercle de 240 à 300 m de rayon autour de la ville, jusqu'à ~480 m
//! d'altitude, pendant que le ciel se décale. Le subterfuge qui garde la ville présente : la fenêtre de tronçons et la
//! brume sont assez larges (`vista` des shaders, `FOG_END_M`) pour qu'elle s'étale sous la caméra jusqu'à l'horizon, et
//! le regard garde une part de la ville (`CITY_LOOK`) : elle reste dans le cadre pendant tout le tour, jamais un décor
//! que la caméra aurait quitté. Rien n'est tiré pour l'occasion : la ville est celle que la voiture traverse.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use super::sky_trip::{SkyTrip, TripGoal};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::lerp;
use crate::types::{Cue, CueKind, RingPlanet, Section, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Durée des phases (s) : montée, approche, glissade le long de l'anneau, tour de la planète, plongée.
const CLIMB: f32 = 6.0;
const APPROACH: f32 = 7.0;
const GLIDE: f32 = 9.0;
const LOOP: f32 = 5.5;
const DIVE: f32 = 7.0;
const TOTAL: f32 = CLIMB + APPROACH + GLIDE + LOOP + DIVE;
/// Hauteur (m) au-dessus de la voiture où la ville se noie dans la brume.
const CEILING: f32 = 480.0;
/// Rayon (m) du cercle que la caméra décrit autour de la voiture pendant la glissade et le tour, et angle (rad) où elle le
/// quitte : trois quarts de tour environ, de sorte que la plongée finale ramène la caméra devant la voiture.
const ORBIT_R_M: f32 = 240.0;
const ORBIT_R_END_M: f32 = 300.0;
const ANGLE_APPROACH_END: f32 = 0.35;
const ANGLE_GLIDE_END: f32 = 2.5;
const ANGLE_LOOP_END: f32 = 4.4;
/// Part du regard qui reste sur la ville (0 : tout le ciel) pendant la glissade et le tour.
const CITY_LOOK: f32 = 0.25;
/// Demi-tour d'anneau survolé et distance au centre de la planète (m, repère du ciel).
const PHI_START: f32 = -0.9;
const PHI_GLIDE_END: f32 = 0.2;
const PHI_LOOP_END: f32 = 1.5;
const RING_RADIUS: f32 = 235.0;

/// Repère de l'anneau : `e1` pointe de la planète vers l'origine du ciel, `e2` complète dans le plan, `n` est la normale.
#[derive(Debug, Clone, Copy)]
struct RingBasis {
    center: Vec3,
    n: Vec3,
    e1: Vec3,
    e2: Vec3,
}

impl RingBasis {
    fn new(planet: &RingPlanet, dir: f32) -> Self {
        let mut n = planet.ring_normal.try_normalize().unwrap_or(Vec3::Y);
        if n.y < 0.0 {
            n = -n;
        }
        let d = planet.center.try_normalize().unwrap_or(Vec3::ZERO);
        let mut e1 = (-d + n * d.dot(n)).try_normalize().unwrap_or(Vec3::ZERO);
        if e1.length_squared() < 0.5 {
            e1 = (Vec3::X - n * n.x).try_normalize().unwrap_or(Vec3::X);
        }
        // Le sens du tour est tiré au sort : e2 est inversé, le voyage se fait dans l'autre direction.
        let e2 = n.cross(e1) * dir;
        Self {
            center: planet.center,
            n,
            e1,
            e2,
        }
    }

    /// Position virtuelle dans le ciel : sur un cercle autour de la planète, dans (ou au-dessus de) le plan de l'anneau.
    fn sky_point(&self, phi: f32, rho: f32, h: f32) -> Vec3 {
        self.center + self.e1 * (phi.cos() * rho) + self.e2 * (phi.sin() * rho) + self.n * h
    }
}

/// Pose haute sur un cercle de rayon `radius` autour de la voiture, à l'angle `angle` (0 : derrière elle, ce qui redonne la
/// pose de montée de Sillage avec `radius` = 30).
fn high_position(frame: &CinemaFrame<'_>, lift: f32, radius: f32, angle: f32) -> DVec3 {
    let car = frame.car;
    DVec3::new(
        car.x * 0.4 + f64::from(radius * angle.sin()),
        car.y + f64::from(lift),
        car.z + f64::from(radius * angle.cos()),
    )
}

/// Raideurs de suivi (position, ciel, regard) en 1/s.
type Rates = [f32; 3];

/// Ce que le regard garde de la ville pendant le tour : un point sur la route, un peu devant la voiture.
fn car_focus(frame: &CinemaFrame<'_>) -> DVec3 {
    let car = frame.car;
    DVec3::new(car.x * 0.35, car.y, car.z - 60.0)
}

/// Séquence épique : on monte au-dessus de la ville, on voyage jusqu'à la géante à anneau, on la survole, puis on replonge.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ringworld;

impl ShotSpec for Ringworld {
    fn id(&self) -> &'static str {
        "ringworld"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Epic
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.6)
    }
    fn min_seconds(&self) -> f32 {
        TOTAL - 2.0
    }
    fn max_seconds(&self) -> f32 {
        TOTAL + 4.0
    }
    fn wide_view(&self) -> bool {
        true
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        420.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.ring_planet.is_some() && frame.altitude < 90.0
    }
    /// Un break (intensité basse) est le bon moment : la musique respire, le ciel prend la place.
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if frame.ring_planet.is_some() && frame.section == Section::Calm && frame.intensity < 0.2 {
            0.72
        } else {
            0.0
        }
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let scale = 0.92 + rng.next_f32() * 0.12;
        let dir = rng.coin();
        Box::new(RingworldShot {
            trip: SkyTrip::default(),
            scale,
            dir,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct RingworldShot {
    trip: SkyTrip,
    scale: f32,
    dir: f32,
    elapsed: f32,
}

/// Montée : on quitte la voiture des yeux pour s'élever et regarder la planète. La ville file en brume.
fn climb(frame: &CinemaFrame<'_>, u: f32, goal: &mut TripGoal) -> Rates {
    goal.pos = high_position(frame, lerp(8.0, CEILING, smootherstep(u)), lerp(8.0, 30.0, u), 0.0);
    goal.drift = Vec3::ZERO;
    goal.sky_look = smoothstep((u - 0.2) / 0.7);
    goal.fov = lerp(62.0, 72.0, u);
    [3.0, 3.0, 2.5]
}

/// Approche : le ciel glisse vers la planète, qui grossit jusqu'à remplir le cadre.
fn approach(frame: &CinemaFrame<'_>, ring: &RingBasis, u: f32, goal: &mut TripGoal) -> Rates {
    let e = smootherstep(u);
    goal.pos = high_position(
        frame,
        CEILING,
        lerp(30.0, ORBIT_R_M, e),
        lerp(0.0, ANGLE_APPROACH_END, e),
    );
    goal.drift = ring.sky_point(PHI_START, RING_RADIUS, 40.0) * smootherstep(u);
    goal.sky_look = 1.0;
    goal.fov = lerp(72.0, 80.0, u);
    goal.roll = 0.05 * (u * std::f32::consts::PI).sin();
    [3.0, 2.2, 3.5]
}

/// Glissade : on file juste au-dessus du plan de l'anneau, qui défile sous la caméra ; la planète roule dans le cadre.
fn glide(frame: &CinemaFrame<'_>, ring: &RingBasis, u: f32, goal: &mut TripGoal) -> Rates {
    let e = smootherstep(u);
    goal.pos = high_position(frame, CEILING, ORBIT_R_M, lerp(ANGLE_APPROACH_END, ANGLE_GLIDE_END, e));
    goal.car_target = car_focus(frame);
    goal.sky_look = 1.0 - CITY_LOOK;
    let phi = lerp(PHI_START, PHI_GLIDE_END, smootherstep(u));
    goal.drift = ring.sky_point(phi, RING_RADIUS, lerp(40.0, 22.0, u));
    // Regard entre le sens de marche et la planète : elle reste grosse, un peu décentrée.
    let ahead = (ring.e1 * -phi.sin() + ring.e2 * phi.cos()) * 220.0 + goal.drift;
    goal.sky_target = ring.center.lerp(ahead, 0.4);
    goal.fov = 80.0;
    goal.roll = 0.14 * (u * std::f32::consts::TAU).sin();
    [3.0, 6.0, 3.5]
}

/// Tour partiel : on s'écarte et l'on monte au-dessus de la planète, le regard fixé sur elle.
fn orbit_loop(frame: &CinemaFrame<'_>, ring: &RingBasis, u: f32, goal: &mut TripGoal) -> Rates {
    let e = smootherstep(u);
    goal.pos = high_position(
        frame,
        lerp(CEILING, CEILING * 0.8, e),
        lerp(ORBIT_R_M, ORBIT_R_END_M, e),
        lerp(ANGLE_GLIDE_END, ANGLE_LOOP_END, e),
    );
    goal.car_target = car_focus(frame);
    goal.drift = ring.sky_point(
        lerp(PHI_GLIDE_END, PHI_LOOP_END, e),
        lerp(RING_RADIUS, 310.0, e),
        lerp(22.0, 110.0, e),
    );
    goal.sky_look = 1.0 - CITY_LOOK;
    goal.fov = lerp(80.0, 72.0, u);
    goal.roll = -0.12 * (u * std::f32::consts::PI).sin();
    [3.0, 6.0, 3.5]
}

/// Plongée : le ciel revient à l'infini pendant que la caméra pique vers la ville et finit derrière la voiture.
fn dive(frame: &CinemaFrame<'_>, ring: &RingBasis, u: f32, goal: &mut TripGoal) -> Rates {
    let e = f64::from(smootherstep((u - 0.08) / 0.85));
    let far = ring.sky_point(PHI_LOOP_END, 310.0, 110.0);
    goal.drift = far * (1.0 - smootherstep(u * 1.05));
    let eye = high_position(frame, CEILING * 0.8, ORBIT_R_END_M, ANGLE_LOOP_END);
    // Même pose que la caméra de poursuite : la prochaine transition du réalisateur est sans à-coup.
    let car = frame.car;
    let low = DVec3::new(car.x * 0.55, 2.3 + f64::from(frame.altitude) * 1.04, car.z + 6.2);
    goal.pos = low.lerp(eye, 1.0 - e);
    goal.sky_look = 1.0 - smoothstep((u - 0.25) / 0.6);
    goal.fov = lerp(72.0, 78.0, u);
    let tail = smoothstep((u - 0.6) / 0.4);
    [3.0 + tail * 7.0, 3.0 + tail * 4.0, 3.5 + tail * 4.0]
}

impl Shot for RingworldShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some(planet) = frame.ring_planet else {
            return false;
        };
        self.elapsed += frame.dt;
        let t = self.elapsed / self.scale;
        if t >= TOTAL {
            return false;
        }
        let ring = RingBasis::new(&planet, self.dir);
        let car = frame.car;
        let mut goal = TripGoal {
            car_target: DVec3::new(car.x * 0.35, 1.0 + f64::from(frame.altitude) * 0.9, car.z - 8.0),
            sky_target: planet.center,
            up: ring.n,
            ..TripGoal::default()
        };
        let u = |from: f32, len: f32| ((t - from) / len).clamp(0.0, 1.0);
        let rates = if t < CLIMB {
            climb(frame, u(0.0, CLIMB), &mut goal)
        } else if t < CLIMB + APPROACH {
            approach(frame, &ring, u(CLIMB, APPROACH), &mut goal)
        } else if t < CLIMB + APPROACH + GLIDE {
            glide(frame, &ring, u(CLIMB + APPROACH, GLIDE), &mut goal)
        } else if t < CLIMB + APPROACH + GLIDE + LOOP {
            orbit_loop(frame, &ring, u(CLIMB + APPROACH + GLIDE, LOOP), &mut goal)
        } else {
            dive(frame, &ring, u(CLIMB + APPROACH + GLIDE + LOOP, DIVE), &mut goal)
        };
        self.trip.step(frame, &goal, rates[0], rates[1], rates[2]);
        self.trip.write(out);
        true
    }

    fn holding(&self) -> bool {
        self.elapsed / self.scale < TOTAL
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.trip.kick = 1.2;
        }
    }
}
