//! La caméra des cascades (`cinema/shots/stunt.ts`).
//!
//! Une cascade n'allume rien : le plan ne touche qu'à la pose de la caméra, jamais à la lumière (règle 7, pas de
//! variation de luminance). Le nombre de coupes que ce plan provoque reste celui du réalisateur, plafonné à
//! [`MAX_CUTS_PER_SECOND`](crate::choreo::MAX_CUTS_PER_SECOND).

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;

use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::subjects::{CamRig, RigRates, safe_position};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// La cascade est presque finie : on peut couper.
const DONE: f32 = 0.97;

/// Cadrage d'une cascade selon son genre : position voulue, regard, champ de vision (degrés) et raideur (1/s).
struct Framing {
    goal: DVec3,
    gaze: DVec3,
    fov: f32,
    rate: f32,
}

/// Cadrage d'une cascade selon son genre (`frame` de Sillage).
fn framing(kind: &str, frame: &CinemaFrame<'_>, side: f32) -> Framing {
    let car = frame.car;
    let focus = frame.stunt.map_or(car, |s| s.focus);
    let ground = frame.ground();
    let side = f64::from(side);
    // `Math.sign(car.x) || side` : sur l'axe de la route, le côté tiré au sort.
    let wall = if car.x.abs() > 0.0 { car.x.signum() } else { side };
    let (goal, gaze, fov, rate) = match kind {
        // De profil, côté route, un peu devant : on voit le ventre de la voiture contre la façade.
        "wall-ride" => (DVec3::new(car.x - wall * 9.0, car.y + 1.2, car.z - 3.0), car, 58.0, 6.0),
        "off-road" => (
            DVec3::new(car.x - wall * 8.5, ground + 1.0, car.z - 7.0),
            DVec3::new(car.x, car.y + 0.4, car.z),
            62.0,
            5.0,
        ),
        // Bas et de face, à l'écart de l'axe : le cargo grossit derrière la voiture.
        "head-on" => (
            DVec3::new(car.x + side * 9.0, ground + 1.4, car.z - 24.0),
            car.lerp(focus, 0.4),
            56.0,
            4.0,
        ),
        // Haut et large : la ville, les toits et la voiture qui court dessus.
        "rooftop" => (
            DVec3::new(car.x * 0.4, car.y + 11.0, car.z + 26.0),
            DVec3::new(car.x, car.y, car.z - 14.0),
            72.0,
            2.8,
        ),
        "sign-hop" => (DVec3::new(car.x + side * 8.0, car.y + 1.5, car.z - 6.0), car, 54.0, 6.0),
        // Bas et loin sur le côté pour embrasser tout l'arc.
        "big-jump" | "loop" => (
            DVec3::new(car.x + side * 13.0, ground + 2.2, car.z + 4.0),
            car,
            60.0,
            4.0,
        ),
        "near-miss" => (
            DVec3::new(car.x - side * 5.0, car.y + 2.0, car.z + 12.0),
            car.lerp(focus, 0.3),
            62.0,
            5.0,
        ),
        // Slalom, enfilade : derrière et un peu au-dessus, comme une caméra de poursuite.
        _ => (
            DVec3::new(car.x * 0.5, car.y + 2.5, car.z + 11.0),
            DVec3::new(car.x, car.y + 1.0, car.z - 20.0),
            66.0,
            4.0,
        ),
    };
    Framing { goal, gaze, fov, rate }
}

/// Cascade scénarisée de la voiture (pilote automatique sans points) : le plan se pose dès son départ et la suit
/// jusqu'au bout, cadré selon le genre — de profil sur un mur ou hors route, bas et de face devant un cargo, haut sur
/// les toits, tournant avec la caisse quand elle vole à l'envers.
#[derive(Debug, Clone, Copy, Default)]
pub struct StuntCam;

impl ShotSpec for StuntCam {
    fn id(&self) -> &'static str {
        "stunt-cam"
    }
    fn tier(&self) -> ShotTier {
        ShotTier::Uncommon
    }
    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }
    fn min_seconds(&self) -> f32 {
        3.0
    }
    fn max_seconds(&self) -> f32 {
        9.0
    }
    fn cooldown(&self) -> f32 {
        6.0
    }
    fn weight(&self) -> f32 {
        3.0
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        frame.stunt.is_some_and(|s| s.kind != "ascension")
    }
    fn opportunity(&self, frame: &CinemaFrame<'_>) -> f32 {
        if frame.stunt.is_some_and(|s| s.kind != "ascension" && s.progress < 0.22) {
            0.95
        } else {
            0.0
        }
    }
    fn create(&self, _frame: &CinemaFrame<'_>, rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(StuntShot {
            rig: CamRig::default(),
            side: rng.coin(),
            progress: 0.0,
        })
    }
}

#[derive(Debug, Clone)]
struct StuntShot {
    rig: CamRig,
    side: f32,
    progress: f32,
}

impl Shot for StuntShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let Some(stunt) = frame.stunt else {
            return false;
        };
        self.progress = stunt.progress;
        if stunt.kind == "inverted" {
            // À l'envers : la caméra est solidaire de la caisse, c'est le monde qui tourne.
            let local = frame.car_rotation * Vec3::new(self.side * 7.0, 0.9, 1.4);
            let cam_up = frame.car_rotation * Vec3::Y;
            let mut goal = frame.car + local.as_dvec3();
            safe_position(frame, &mut goal, 1.5);
            self.rig.pos = goal;
            aim(out, goal, frame.car, 0.0, cam_up, false);
            self.rig.quat = out.rotation;
            self.rig.fov = 58.0;
            self.rig.ready = true;
        } else {
            let Framing {
                mut goal,
                gaze,
                fov,
                rate,
            } = framing(stunt.kind, frame, self.side);
            safe_position(frame, &mut goal, 1.5);
            let rates = RigRates {
                pos: rate,
                look: rate * 1.3,
            };
            let fov = (fov + frame.speed * 0.05).clamp(40.0, 80.0);
            self.rig.track(frame.dt, goal, gaze, fov, rates, 0.0, Vec3::Y);
        }
        self.rig.write(out);
        true
    }

    fn holding(&self) -> bool {
        self.progress < DONE
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if cue.kind == CueKind::Beat {
            self.rig.kick = 1.0;
        }
    }
}
