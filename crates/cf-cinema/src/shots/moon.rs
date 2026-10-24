//! Le passage de la lune (`cinema/shots/moon.ts`) : court voyage jusqu'à la lune voisine, qu'on frôle sur le côté, puis
//! retour derrière la voiture. Aucune lumière touchée (règle 7) : seule la pose et le décalage du ciel bougent.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use super::sky_trip::{SkyTrip, TripGoal};
use crate::pose::{smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::lerp;
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// La lune voisine, telle que `planets.ts` la place dans le ciel : azimut 16°, élévation 30°, 300 m.
const AZIMUTH_DEG: f32 = 16.0;
const ELEVATION_DEG: f32 = 30.0;
const DISTANCE: f32 = 300.0;
const MOON_RADIUS: f32 = 20.0;

const CLIMB: f32 = 4.0;
const APPROACH: f32 = 3.5;
const PASS: f32 = 4.5;
const RETURN: f32 = 4.0;
const TOTAL: f32 = CLIMB + APPROACH + PASS + RETURN;
const CEILING: f64 = 130.0;
/// Distance au centre de la lune pendant le passage : ~50 m de la surface.
const ORBIT: f32 = MOON_RADIUS + 52.0;
const SWING: f32 = 1.1;

/// Repère de la lune dans le ciel : son centre, la direction de la ligne de visée et l'axe horizontal qui la croise.
#[derive(Debug, Clone, Copy)]
struct MoonFrame {
    moon: Vec3,
    to_moon: Vec3,
    /// Axe horizontal perpendiculaire à la direction de la lune : le passage se fait de part et d'autre de la ligne de
    /// visée.
    across: Vec3,
}

impl MoonFrame {
    fn new() -> Self {
        let (az, el) = (AZIMUTH_DEG.to_radians(), ELEVATION_DEG.to_radians());
        let dir = Vec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos());
        let to_moon = dir.normalize();
        Self {
            moon: dir * DISTANCE,
            to_moon,
            across: Vec3::Y.cross(to_moon).try_normalize().unwrap_or(Vec3::X),
        }
    }

    /// Point de l'orbite horizontale autour de la lune ; `swing` = 0 face à elle, côté opposé à la voiture.
    fn orbit(&self, swing: f32, rho: f32, rise: f32) -> Vec3 {
        let mut p = self.moon - self.to_moon * (swing.cos() * rho) + self.across * (swing.sin() * rho);
        p.y += rise;
        p
    }
}

/// Court voyage jusqu'à la lune : on la frôle sur le côté, puis on redescend derrière la voiture.
#[derive(Debug, Clone, Copy, Default)]
pub struct MoonPass;

impl ShotSpec for MoonPass {
    fn id(&self) -> &'static str {
        "moon-pass"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Rare
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 0.8)
    }
    fn min_seconds(&self) -> f32 {
        TOTAL - 1.0
    }
    fn max_seconds(&self) -> f32 {
        TOTAL + 3.0
    }
    fn scripted(&self) -> bool {
        true
    }
    fn cooldown(&self) -> f32 {
        240.0
    }
    fn weight(&self) -> f32 {
        1.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Swoop
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.altitude < 90.0
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        let dir = rng.coin();
        let scale = 0.95 + rng.next_f32() * 0.15;
        Box::new(MoonPassShot {
            trip: SkyTrip::default(),
            sky: MoonFrame::new(),
            dir,
            scale,
            elapsed: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct MoonPassShot {
    trip: SkyTrip,
    sky: MoonFrame,
    dir: f32,
    scale: f32,
    elapsed: f32,
}

/// Avancement dans chacune des quatre phases (0 → 1) : montée, approche, passage, retour.
fn phases(t: f32) -> [f32; 4] {
    [
        (t / CLIMB).clamp(0.0, 1.0),
        ((t - CLIMB) / APPROACH).clamp(0.0, 1.0),
        ((t - CLIMB - APPROACH) / PASS).clamp(0.0, 1.0),
        ((t - CLIMB - APPROACH - PASS) / RETURN).clamp(0.0, 1.0),
    ]
}

impl MoonPassShot {
    /// Retour : on redescend derrière la voiture, à la pose de la caméra de poursuite, le ciel revient à l'infini.
    fn come_back(&self, frame: &CinemaFrame<'_>, u4: f32, goal: &mut TripGoal) -> [f32; 3] {
        let car = frame.car;
        let e = f64::from(smootherstep((u4 - 0.05) / 0.85));
        let eye = DVec3::new(car.x * 0.4, car.y + CEILING, car.z + 24.0);
        let low = DVec3::new(
            car.x * 0.55,
            2.3 + f64::from(frame.altitude) * 1.04,
            car.z + 6.2,
        );
        goal.pos = low.lerp(eye, 1.0 - e);
        let end = self.sky.orbit(SWING * self.dir, ORBIT, 14.0);
        goal.drift = end * (1.0 - smootherstep(u4 * 1.05));
        goal.sky_look = 1.0 - smoothstep((u4 - 0.25) / 0.6);
        goal.fov = lerp(74.0, 78.0, u4);
        let tail = smoothstep((u4 - 0.6) / 0.4);
        [3.0 + tail * 7.0, 3.0 + tail * 4.0, 3.5 + tail * 4.0]
    }
}

impl Shot for MoonPassShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        self.elapsed += frame.dt;
        let t = self.elapsed / self.scale;
        if t >= TOTAL {
            return false;
        }
        let car = frame.car;
        let sky = self.sky;
        let dir = self.dir;
        let mut goal = TripGoal {
            car_target: DVec3::new(car.x * 0.35, 1.0 + f64::from(frame.altitude) * 0.9, car.z - 8.0),
            sky_target: sky.moon,
            ..TripGoal::default()
        };
        let [u1, u2, u3, u4] = phases(t);
        let perch = DVec3::new(car.x * 0.4, car.y + CEILING, car.z + 24.0);
        let mut rates = [3.0, 3.0, 3.5];
        if t < CLIMB {
            let climb = f64::from(lerp(8.0, CEILING as f32, smootherstep(u1)));
            goal.pos = DVec3::new(car.x * 0.4, car.y + climb, car.z + f64::from(lerp(8.0, 24.0, u1)));
            goal.sky_look = smoothstep((u1 - 0.15) / 0.7);
            goal.fov = lerp(62.0, 70.0, u1);
        } else if t < CLIMB + APPROACH {
            goal.pos = perch;
            goal.drift = sky.orbit(-SWING * dir, ORBIT, 6.0) * smootherstep(u2);
            goal.sky_look = 1.0;
            goal.fov = lerp(70.0, 74.0, u2);
            rates[1] = 2.5;
        } else if t < CLIMB + APPROACH + PASS {
            // Passage : on glisse le long de la lune, qui occupe tout un flanc du cadre, avec un léger roulis.
            goal.pos = perch;
            let swing = lerp(-SWING * dir, SWING * dir, smootherstep(u3));
            goal.drift = sky.orbit(swing, ORBIT, lerp(6.0, 14.0, u3));
            goal.sky_look = 1.0;
            goal.fov = 74.0;
            goal.roll = 0.1 * (u3 * std::f32::consts::PI).sin() * dir;
            rates[1] = 6.0;
        } else {
            rates = self.come_back(frame, u4, &mut goal);
        }
        self.trip.step(frame, &goal, rates[0], rates[1], rates[2]);
        self.trip.write(out);
        true
    }

    fn holding(&self) -> bool {
        self.elapsed / self.scale < TOTAL
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.trip.kick = 1.0;
        }
    }
}
