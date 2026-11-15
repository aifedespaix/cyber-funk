//! Le plan de clôture : le miroir de l'ouverture, en un seul plan-séquence.
//!
//! ```text
//!  la voiture met les gaz        elle n'est plus qu'un point       la route reste
//!  ├ la caméra la laisse filer ──┤ ses feux s'éteignent à l'horizon ┤ la caméra glisse, redescend au ras du sol
//!  │ et s'élève un peu           │ le quartier rentre sous terre    │ et se pose sur le plan d'attente
//! ```
//!
//! À l'ouverture la voiture entrait par l'arrière de l'image ; ici elle en sort par le fond. Une fois disparue (et
//! arrêtée, hors champ, par le monde), la caméra continue seule sur la route nue et vient se poser exactement sur la pose
//! de repos de l'attente (`waiting::rest`) : le prochain `Démarrer` rejouera l'entrée par l'arrière, sans raccord.
//!
//! La caméra a sa propre vitesse dans le monde : elle ne dépend pas de celle de la voiture, que l'on freine hors champ.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Staging};

use super::waiting;
use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Vitesse de croisière de la caméra pendant que la voiture s'éloigne (m/s).
pub const GLIDE_MPS: f64 = 20.0;
/// Raideur du passage de la vitesse de la voiture à la vitesse de croisière (1/s).
const SETTLE_RATE: f32 = 0.6;
/// Durée pendant laquelle la caméra reste accrochée derrière la voiture (s) : le temps de revenir du plan du show et de
/// la cadrer, avant qu'elle ne mette les gaz.
pub const HOLD_S: f32 = 4.0;
/// Hauteur de l'œil pendant l'adieu (m) : un peu au-dessus de la route, pour voir la voiture jusqu'au point de fuite.
const FAREWELL_HEIGHT_M: f64 = 5.0;
/// Recul minimal derrière la voiture tant qu'elle est visible (m) : la caméra ne la double pas.
const MIN_BEHIND_M: f64 = 6.0;
/// Focale pendant que la voiture s'éloigne (°).
const FAREWELL_FOV_DEG: f32 = 38.0;
/// Marge avant la fin de la séquence (s) : la caméra est posée un peu avant.
const LAND_MARGIN_S: f32 = 0.8;
/// Durée supposée de la clôture quand le monde ne la publie pas (s).
const FALLBACK_S: f32 = 20.0;
/// Instant supposé de la disparition de la voiture quand le monde ne le publie pas (s).
const FALLBACK_GONE_S: f32 = 9.4;

/// Amortissement critique (`SmoothDamp`) : rapproche `current` de `target` en ~`smooth_s`, sans dépassement.
#[must_use]
pub fn smooth_damp(current: f64, target: f64, velocity: &mut f64, smooth_s: f64, dt: f64) -> f64 {
    let omega = 2.0 / smooth_s.max(1e-3);
    let x = omega * dt;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let change = current - target;
    let temp = (*velocity + omega * change) * dt;
    *velocity = (*velocity - omega * temp) * decay;
    target + (change + temp) * decay
}

/// La fiche du plan de clôture.
#[derive(Debug, Clone, Copy, Default)]
pub struct Closing;

impl ShotSpec for Closing {
    fn id(&self) -> &'static str {
        "closing"
    }

    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }

    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }

    fn min_seconds(&self) -> f32 {
        1.0
    }

    fn max_seconds(&self) -> f32 {
        60.0
    }

    fn cooldown(&self) -> f32 {
        0.0
    }

    fn weight(&self) -> f32 {
        0.0
    }

    /// On quitte le plan du show, quel qu'il soit, en fondu de mouvement.
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }

    /// La voiture s'éloigne loin devant, puis la caméra la dépasse : hors des bornes ordinaires.
    fn wide_view(&self) -> bool {
        true
    }

    /// Jamais tiré par le montage automatique : seul le monde l'impose.
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        false
    }

    fn create(&self, frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        let eye = DVec3::new(frame.car.x + 1.2, frame.ground() + 3.0, frame.car.z + 10.0);
        Box::new(ClosingShot {
            eye,
            velocity: DVec3::new(0.0, 0.0, -f64::from(frame.speed)),
            look: frame.car + DVec3::new(0.0, 1.0, 0.0),
            fov: 56.0,
            clock: 0.0,
        })
    }
}

/// Le plan en cours : l'œil, sa vitesse dans le monde et le point visé (absolus).
#[derive(Debug, Clone, Copy)]
struct ClosingShot {
    eye: DVec3,
    velocity: DVec3,
    look: DVec3,
    fov: f32,
    clock: f32,
}

impl Shot for ClosingShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let dt = frame.dt;
        self.clock += dt;
        let staging = frame.staging.filter(|s| s.kind == Staging::Closing);
        let (t, duration, visible) = staging.map_or((self.clock, FALLBACK_S, self.clock < FALLBACK_GONE_S), |s| {
            (s.t_s, s.duration_s, s.car_visible)
        });
        let ground = frame.ground();
        let follow = |rate: f32| f64::from(1.0 - (-dt * rate).exp());
        let wanted_look = if visible && t < HOLD_S {
            // Accrochée derrière la voiture, à sa vitesse : on la retrouve avant de la laisser partir.
            self.eye = DVec3::new(frame.car.x + 1.2, ground + 3.0, frame.car.z + 10.0);
            self.velocity = DVec3::new(0.0, 0.0, -f64::from(frame.speed));
            frame.car + DVec3::new(0.0, 1.0, 0.0)
        } else if visible {
            // L'adieu : la caméra ralentit jusqu'à sa croisière et laisse la voiture partir vers le point de fuite.
            let before = self.eye;
            let speed = -self.velocity.z + (GLIDE_MPS + self.velocity.z) * follow(SETTLE_RATE);
            self.eye.z = (self.eye.z - speed * f64::from(dt)).max(frame.car.z + MIN_BEHIND_M);
            self.eye.y += (ground + FAREWELL_HEIGHT_M - self.eye.y) * follow(0.5);
            self.eye.x += (frame.car.x + 1.2 - self.eye.x) * follow(0.8);
            if dt > 0.0 {
                self.velocity = (self.eye - before) / f64::from(dt);
            }
            frame.car + DVec3::new(0.0, 1.0, 0.0)
        } else {
            // La route reste : la caméra glisse jusqu'à la pose de repos, devant la voiture arrêtée hors champ.
            let (rest_eye, rest_look) = waiting::rest(DVec3::new(frame.car.x, ground, frame.car.z));
            let remaining = f64::from((duration - LAND_MARGIN_S - t).max(0.2));
            let smooth_s = (remaining / 2.4).max(0.12);
            let dt = f64::from(dt);
            self.eye.x = smooth_damp(self.eye.x, rest_eye.x, &mut self.velocity.x, smooth_s, dt);
            self.eye.y = smooth_damp(self.eye.y, rest_eye.y, &mut self.velocity.y, smooth_s, dt);
            self.eye.z = smooth_damp(self.eye.z, rest_eye.z, &mut self.velocity.z, smooth_s, dt);
            rest_look
        };
        self.look += (wanted_look - self.look) * follow(1.4);
        // Longue focale sur le départ : la voiture reste lisible jusqu'au point de fuite ; puis la focale de l'attente.
        let wanted_fov = if visible && t >= HOLD_S {
            FAREWELL_FOV_DEG
        } else {
            waiting::FOV_DEG
        };
        self.fov += (wanted_fov - self.fov) * (1.0 - (-dt * 0.7).exp());
        aim(out, self.eye, self.look, 0.0, Vec3::Y, false);
        out.fov_deg = self.fov;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{CinemaWorld, NoObstacles};
    use cf_render::StagingView;

    const DT: f32 = 1.0 / 60.0;
    const STOP_S: f32 = 20.0;
    const GONE_S: f32 = 9.4;

    /// Joue la clôture depuis `from_mps` : la voiture met les gaz à 3,8 s, disparaît à 9,4 s, puis freine hors champ.
    fn play(from_mps: f32, mut each: impl FnMut(f32, &CinemaWorld, &CameraPose)) -> (CinemaWorld, CameraPose) {
        let mut world = CinemaWorld {
            dt: DT,
            speed: from_mps,
            ..CinemaWorld::default()
        };
        let obstacles = NoObstacles;
        let mut shot = Closing.create(&CinemaFrame::new(&world, &obstacles, &[]), &mut CinemaRng::new(1));
        let mut pose = CameraPose::default();
        let mut t = 0.0_f32;
        while t < STOP_S {
            t += DT;
            let wanted = if t < 3.8 {
                from_mps
            } else if t < GONE_S {
                from_mps.max(30.0) + 16.0
            } else {
                0.0
            };
            world.speed += (wanted - world.speed) * (1.0 - (-DT * 1.7).exp());
            world.car.z -= f64::from(world.speed * DT);
            world.staging = Some(StagingView {
                kind: Staging::Closing,
                t_s: t,
                duration_s: STOP_S,
                city_s: 0.0,
                gantry: None,
                car_visible: t < GONE_S,
            });
            assert!(shot.update(&CinemaFrame::new(&world, &obstacles, &[]), &mut pose));
            each(t, &world, &pose);
        }
        (world, pose)
    }

    #[test]
    fn the_car_rides_away_to_the_vanishing_point_before_it_disappears() {
        for from in [16.0, 40.0, 60.0] {
            let mut gap_when_gone = 0.0;
            play(from, |t, world, pose| {
                if t < GONE_S {
                    gap_when_gone = pose.position.z - world.car.z;
                    assert!(
                        gap_when_gone >= MIN_BEHIND_M - 1e-6,
                        "la caméra ne double pas la voiture visible"
                    );
                }
            });
            assert!(gap_when_gone > 90.0, "depuis {from} m/s : {gap_when_gone} m");
        }
    }

    #[test]
    fn the_camera_lands_exactly_on_the_waiting_pose() {
        for from in [16.0, 40.0, 60.0] {
            let (world, pose) = play(from, |_, _, _| {});
            let (rest_eye, _) = waiting::rest(DVec3::new(world.car.x, 0.0, world.car.z));
            let off = pose.position.distance(rest_eye);
            assert!(off < 0.6, "depuis {from} m/s : à {off} m de la pose de repos");
        }
    }

    #[test]
    fn the_camera_never_jumps_nor_goes_backwards() {
        let mut prev: Option<DVec3> = None;
        play(40.0, |t, _, pose| {
            if let Some(before) = prev {
                let step = pose.position.distance(before);
                assert!(step < 1.8, "{t} : saut de {step} m");
                assert!(pose.position.z <= before.z + 1e-6, "{t} : la caméra recule");
            }
            assert!(pose.position.y > 0.3);
            prev = Some(pose.position);
        });
    }

    #[test]
    fn smooth_damp_reaches_its_target_without_overshoot() {
        let (mut x, mut v) = (0.0, 0.0);
        for _ in 0..600 {
            x = smooth_damp(x, 10.0, &mut v, 0.5, 1.0 / 60.0);
            assert!(x <= 10.0 + 1e-9);
        }
        assert!((x - 10.0).abs() < 1e-3);
    }

    #[test]
    fn the_shot_is_never_picked_by_the_automatic_editing() {
        assert_eq!(Closing.weight(), 0.0);
        assert_eq!(Closing.id(), "closing");
    }
}
