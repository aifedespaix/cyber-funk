//! Le plan d'ouverture : un seul plan-séquence, écrit, de `Démarrer` jusqu'au premier cue du show.
//!
//! ```text
//!  attente          la voiture entre         poursuite        portique           la ville se lève        le show
//!  ├ caméra au sol ─┤ par l'arrière, frôle ──┤ la caméra ─────┤ contre-plongée ──┤ grue : on monte, ─────┤ on redescend
//!  │ route vide     │ l'objectif et file     │ décolle et     │ sur le nom, on   │ les tours jaillissent │ derrière la
//!  │                │ vers l'horizon         │ la rattrape    │ passe dessous    │ des deux côtés        │ voiture
//! ```
//!
//! Rien n'« apparaît » : la voiture arrive de derrière la caméra (elle s'est allumée hors champ), puis tout ce qui
//! suit est provoqué par elle. La caméra, d'abord immobile, la laisse filer avant de décoller et de la rattraper : c'est
//! elle qui donne l'échelle de la route, puis de la ville.
//!
//! Le plan lit la séquence dans `CinemaWorld::staging` (publiée par le monde). Il n'est jamais tiré par le montage
//! automatique : le monde l'impose avec `ShotOrder::Hold("opening")` et le relâche quand le show commence.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Staging};

use super::waiting;
use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// La voiture a « passé » la caméra quand elle est cette distance devant elle (m).
const PASS_M: f64 = 2.0;
/// Durée sur laquelle la caméra, immobile au passage, se cale sur la voiture (s) : avant, la voiture file ; après, la
/// caméra la suit.
const GRIP_S: f32 = 3.4;
/// Raideur du rattrapage une fois calée (1/s).
const FOLLOW_RATE: f32 = 1.25;
/// Raideur du regard (1/s).
const LOOK_RATE: f32 = 1.8;
/// Distance au portique (m) à laquelle la caméra commence à lever les yeux vers le nom.
const GANTRY_NOTICE_M: f64 = 125.0;
/// Distance au portique (m) sous laquelle le regard revient sur la route : on passe dessous sans se tordre le cou.
const GANTRY_RELEASE_M: f64 = 16.0;
/// Hauteur du nom au-dessus de la route (m) : le milieu de la poutre du portique.
const NAME_HEIGHT_M: f64 = 11.5;
/// Hauteur minimale de l'œil au-dessus de la route (m).
const MIN_HEIGHT_M: f64 = 0.4;

/// Le cadrage voulu autour de la voiture, à un instant de l'ouverture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rig {
    /// Recul derrière la voiture (m).
    pub behind: f32,
    /// Hauteur de l'œil au-dessus de la route (m).
    pub height: f32,
    /// Écart latéral (m, positif à droite).
    pub side: f32,
    /// Point visé, par rapport à la voiture : hauteur (m) et distance devant elle (m).
    pub look_up: f32,
    /// Distance devant la voiture du point visé (m).
    pub look_ahead: f32,
    /// Focale (°).
    pub fov: f32,
}

impl Rig {
    /// La poursuite basse, après le passage : la voiture devant, la route qui file.
    pub const FOLLOW: Self = Self {
        behind: 8.5,
        height: 1.5,
        side: 1.3,
        look_up: 1.3,
        look_ahead: 40.0,
        fov: 56.0,
    };
    /// Le sommet de la grue : haut et en retrait, la voiture petite dans le canyon qui se lève.
    pub const CRANE: Self = Self {
        behind: 30.0,
        height: 16.5,
        side: 0.0,
        look_up: 10.0,
        look_ahead: 150.0,
        fov: 66.0,
    };
    /// L'atterrissage : derrière la voiture, à hauteur de poursuite, prêt pour le premier plan du show.
    pub const LANDING: Self = Self {
        behind: 9.5,
        height: 3.2,
        side: 0.0,
        look_up: 1.4,
        look_ahead: 30.0,
        fov: 58.0,
    };

    fn mix(self, other: Self, t: f32) -> Self {
        let m = |a: f32, b: f32| a + (b - a) * t;
        Self {
            behind: m(self.behind, other.behind),
            height: m(self.height, other.height),
            side: m(self.side, other.side),
            look_up: m(self.look_up, other.look_up),
            look_ahead: m(self.look_ahead, other.look_ahead),
            fov: m(self.fov, other.fov),
        }
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Début de la montée de la grue (s après le début du quartier) : juste après le portique.
pub const CRANE_UP_S: f32 = 0.6;
/// La grue est au sommet (s).
pub const CRANE_TOP_S: f32 = 7.0;
/// Début de la descente (s).
pub const CRANE_DOWN_S: f32 = 9.0;
/// La caméra est revenue derrière la voiture (s).
pub const CRANE_LANDED_S: f32 = 13.5;

/// Le cadrage voulu `city_s` secondes après que le quartier a commencé à se lever (négatif avant : poursuite).
#[must_use]
pub fn rig(city_s: f32) -> Rig {
    let up = smooth((city_s - CRANE_UP_S) / (CRANE_TOP_S - CRANE_UP_S));
    let down = smooth((city_s - CRANE_DOWN_S) / (CRANE_LANDED_S - CRANE_DOWN_S));
    Rig::FOLLOW.mix(Rig::CRANE, up).mix(Rig::LANDING, down)
}

/// Part du regard qui monte vers le nom du portique, pour une voiture à `distance_m` de lui (positif avant de passer).
#[must_use]
pub fn gantry_gaze(distance_m: f64) -> f32 {
    if distance_m <= 0.0 {
        return 0.0;
    }
    let notice = smooth(((GANTRY_NOTICE_M - distance_m) / (GANTRY_NOTICE_M * 0.6)) as f32);
    let release = smooth((distance_m / GANTRY_RELEASE_M) as f32);
    0.62 * notice * release
}

/// La fiche du plan d'ouverture.
#[derive(Debug, Clone, Copy, Default)]
pub struct Opening;

impl ShotSpec for Opening {
    fn id(&self) -> &'static str {
        "opening"
    }

    fn tier(&self) -> ShotTier {
        ShotTier::Common
    }

    fn energy(&self) -> (f32, f32) {
        (0.0, 1.0)
    }

    /// Court : tenu par le monde il reste à l'antenne ; relâché, le montage coupe au premier temps du show.
    fn min_seconds(&self) -> f32 {
        1.0
    }

    fn max_seconds(&self) -> f32 {
        4.0
    }

    fn cooldown(&self) -> f32 {
        0.0
    }

    fn weight(&self) -> f32 {
        0.0
    }

    /// On entre depuis le plan d'attente, sur la même pose : un fondu de mouvement, invisible.
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }

    /// La caméra attend loin devant la voiture, puis la laisse filer : hors des bornes ordinaires.
    fn wide_view(&self) -> bool {
        true
    }

    /// Jamais tiré par le montage automatique : seul le monde l'impose.
    fn available(&self, _frame: &CinemaFrame<'_>) -> bool {
        false
    }

    fn create(&self, frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        let (eye, target) = waiting::rest(frame.car);
        // Pris en cours de route (passation : la voiture roule déjà), le plan commence directement en poursuite.
        let rolling = frame.speed > 4.0;
        let follow = Rig::FOLLOW;
        Box::new(OpeningShot {
            eye,
            look: target,
            fov: waiting::FOV_DEG,
            passed: rolling,
            since_pass: if rolling { GRIP_S } else { 0.0 },
            offset: DVec3::new(
                f64::from(follow.side),
                f64::from(follow.height),
                f64::from(follow.behind),
            ),
            look_rel: DVec3::new(0.0, f64::from(follow.look_up), -f64::from(follow.look_ahead)),
            clock: 0.0,
            lift: 0.0,
        })
    }
}

/// Le plan en cours.
#[derive(Debug, Clone, Copy)]
struct OpeningShot {
    /// L'œil avant le passage de la voiture (absolu) : la pose de repos de l'attente.
    eye: DVec3,
    /// Le point visé avant le passage (absolu).
    look: DVec3,
    fov: f32,
    /// La voiture est passée devant la caméra.
    passed: bool,
    /// Secondes depuis le passage.
    since_pass: f32,
    /// L'œil par rapport à la voiture au sol (x à droite, y au-dessus de la route, z positif derrière).
    offset: DVec3,
    /// Le point visé par rapport à la voiture au sol.
    look_rel: DVec3,
    /// Horloge propre (s), quand le monde ne publie pas la séquence.
    clock: f32,
    /// Altitude de la voiture suivie par la caméra, lissée (m).
    lift: f64,
}

impl Shot for OpeningShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        let dt = frame.dt;
        self.clock += dt;
        let ground = DVec3::new(frame.car.x, frame.ground(), frame.car.z);
        if !self.passed {
            // Immobile, au ras de la route. Si la voiture se déporte avant d'arriver, la caméra garde son écart.
            let side = frame.car.x + waiting::SIDE_M;
            self.eye.x += (side - self.eye.x) * f64::from(1.0 - (-dt * 1.5).exp());
            aim(out, self.eye, self.look, 0.0, Vec3::Y, false);
            out.fov_deg = self.fov;
            if frame.car.z < self.eye.z - PASS_M {
                self.passed = true;
                self.offset = self.eye - ground;
                self.look_rel = self.look - ground;
            }
            return true;
        }
        self.since_pass += dt;
        let staging = frame.staging.filter(|s| s.kind == Staging::Opening);
        // Sans séquence publiée (plan imposé à la main), la grue ne part pas : poursuite simple.
        let mut want = rig(staging.map_or(-1.0, |s| s.city_s));
        let mut gaze = 0.0;
        let mut name = DVec3::ZERO;
        if let Some(gantry) = staging.and_then(|s| s.gantry) {
            let distance = frame.car.z - gantry.z;
            gaze = gantry_gaze(distance);
            name = DVec3::new(gantry.x - ground.x, NAME_HEIGHT_M, gantry.z - ground.z);
            // Sous le nom : plus bas et plus près, la voiture en amorce.
            want.height += (1.0 - want.height) * gaze;
            want.behind += (7.0 - want.behind) * gaze;
        }
        let grip = smooth(self.since_pass / GRIP_S);
        // Tant qu'elle n'est pas calée, la caméra reste sur place : la voiture s'éloigne à sa propre vitesse.
        self.offset.z += f64::from((1.0 - grip) * frame.speed * dt);
        let wanted = DVec3::new(f64::from(want.side), f64::from(want.height), f64::from(want.behind));
        self.offset += (wanted - self.offset) * f64::from(1.0 - (-dt * FOLLOW_RATE * grip).exp());
        self.offset.y = self.offset.y.max(MIN_HEIGHT_M);
        let ahead = DVec3::new(0.0, f64::from(want.look_up), -f64::from(want.look_ahead));
        let wanted_look = ahead + (name - ahead) * f64::from(gaze);
        self.look_rel += (wanted_look - self.look_rel) * f64::from(1.0 - (-dt * LOOK_RATE * grip.max(0.15)).exp());
        self.fov += (want.fov - self.fov) * (1.0 - (-dt * 1.2 * grip).exp());
        // Si la voiture s'envole, la caméra monte avec elle (un peu moins) et garde le regard sur elle.
        self.lift += (f64::from(frame.altitude.max(0.0)) - self.lift) * f64::from(1.0 - (-dt * 3.0).exp());
        let eye = ground + self.offset + DVec3::new(0.0, 0.7 * self.lift, 0.0);
        let look = ground + self.look_rel + DVec3::new(0.0, self.lift, 0.0);
        aim(out, eye, look, 0.0, Vec3::Y, false);
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

    /// Joue l'ouverture : la voiture part de l'arrêt, accélère jusqu'à 22 m/s, le portique est 300 m devant.
    fn play(seconds: f32, mut each: impl FnMut(f32, &CinemaWorld, &CameraPose)) {
        let mut world = CinemaWorld {
            dt: DT,
            ..CinemaWorld::default()
        };
        let obstacles = NoObstacles;
        let mut shot = Opening.create(&CinemaFrame::new(&world, &obstacles, &[]), &mut CinemaRng::new(1));
        let mut pose = CameraPose::default();
        let gantry = DVec3::new(0.0, 0.0, -300.0);
        let mut t = 0.0_f32;
        while t < seconds {
            t += DT;
            world.speed = 22.0 * smooth((t - 2.4) / 3.6);
            world.car.z -= f64::from(world.speed * DT);
            world.time = f64::from(t);
            world.staging = Some(StagingView {
                kind: Staging::Opening,
                t_s: t,
                duration_s: 30.0,
                city_s: t - 15.5,
                gantry: (t > 8.0).then_some(gantry),
                car_visible: true,
            });
            assert!(shot.update(&CinemaFrame::new(&world, &obstacles, &[]), &mut pose));
            each(t, &world, &pose);
        }
    }

    #[test]
    fn the_camera_waits_still_on_the_road_until_the_car_passes_it() {
        let (rest_eye, _) = waiting::rest(DVec3::ZERO);
        play(4.0, |t, world, pose| {
            if world.car.z > rest_eye.z {
                assert!(pose.position.distance(rest_eye) < 1e-6, "{t} : {:?}", pose.position);
            }
        });
    }

    #[test]
    fn the_car_enters_from_behind_then_the_camera_takes_off_and_catches_up() {
        let mut worst_lag = 0.0_f64;
        let mut final_lag = 0.0_f64;
        play(14.0, |_, world, pose| {
            let lag = pose.position.z - world.car.z;
            if world.car.z < pose.position.z {
                worst_lag = worst_lag.max(lag);
            }
            final_lag = lag;
        });
        assert!(worst_lag > 25.0, "la voiture file d'abord : {worst_lag}");
        assert!(worst_lag < 85.0, "sans sortir de l'image : {worst_lag}");
        assert!(
            final_lag > 5.0 && final_lag < 14.0,
            "puis la caméra la rattrape : {final_lag}"
        );
    }

    #[test]
    fn the_camera_moves_without_a_jump_through_the_whole_opening() {
        let mut prev: Option<(DVec3, DVec3)> = None;
        play(30.0, |t, world, pose| {
            if let Some((eye, car)) = prev {
                // Par rapport à la voiture, jamais plus de 1,2 m par image (72 m/s) : aucun saut.
                let step = (pose.position - world.car).distance(eye - car);
                assert!(step < 1.2, "{t} : saut de {step} m");
            }
            assert!(pose.position.y > 0.3, "jamais sous la chaussée");
            prev = Some((pose.position, world.car));
        });
    }

    #[test]
    fn the_crane_rises_over_the_city_then_lands_behind_the_car() {
        let mut top = 0.0_f64;
        let mut last = CameraPose::default();
        let mut car = DVec3::ZERO;
        play(30.0, |_, world, pose| {
            top = top.max(pose.position.y);
            last = *pose;
            car = world.car;
        });
        assert!(top > 13.0 && top < 20.0, "sous le tablier des arches (24 m) : {top}");
        assert!(last.position.y < 4.5, "redescendue : {}", last.position.y);
        let behind = last.position.z - car.z;
        assert!(behind > 6.0 && behind < 14.0, "derrière la voiture : {behind}");
    }

    #[test]
    fn the_camera_passes_under_the_gantry_low() {
        // Clairance du portique : 9 m.
        play(22.0, |t, _, pose| {
            if (pose.position.z + 300.0).abs() < 4.0 {
                assert!(pose.position.y < 6.0, "{t} : {}", pose.position.y);
            }
        });
    }

    #[test]
    fn the_gaze_rises_to_the_name_then_returns_to_the_road() {
        assert_eq!(gantry_gaze(200.0), 0.0);
        assert!(gantry_gaze(40.0) > 0.4);
        assert!(gantry_gaze(2.0) < 0.1);
        assert_eq!(gantry_gaze(-5.0), 0.0);
    }

    #[test]
    fn the_rig_is_the_follow_before_the_city_and_the_landing_after() {
        assert_eq!(rig(-3.0), Rig::FOLLOW);
        let landed = rig(CRANE_LANDED_S + 1.0);
        assert!(
            (landed.height - Rig::LANDING.height).abs() < 1e-3 && (landed.behind - Rig::LANDING.behind).abs() < 1e-3
        );
        let top = rig(CRANE_TOP_S + 0.5);
        assert!((top.height - Rig::CRANE.height).abs() < 0.01);
    }

    #[test]
    fn the_shot_is_never_picked_by_the_automatic_editing() {
        assert_eq!(Opening.weight(), 0.0);
        assert_eq!(Opening.id(), "opening");
    }
}
