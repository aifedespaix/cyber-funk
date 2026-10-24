//! Synchro caméra / voiture (`cinema/car-sync.ts`). Deux rôles :
//!
//! - détecter le début et la fin des animations de la voiture (décollage, atterrissage, esquive, figure) pour que le
//!   réalisateur puisse y basculer tout de suite et que le journal les date ;
//! - faire respirer la caméra avec la caisse : roulis et assiette dans l'esquive et la montée, champ qui s'ouvre quand
//!   les roues basculent, petit rebond sur les répulseurs. L'effet est doux et ne s'applique qu'aux plans voiture.

use bevy::math::{Quat, Vec3};
use cf_render::CameraPose;

use crate::kit::{FOV_MAX, FOV_MIN};
use crate::pose::follow;
use crate::types::FlightPhase;
use crate::world::CinemaWorld;

/// Un changement d'animation de la voiture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CarEventKind {
    /// Les roues commencent à se déployer.
    Takeoff,
    /// La voiture est en vol.
    Airborne,
    /// Approche d'atterrissage.
    Descent,
    /// La voiture s'est posée.
    Touchdown,
    /// Une esquive commence.
    Dodge,
    /// L'esquive finit.
    DodgeEnd,
    /// Une figure commence.
    Trick,
    /// La figure finit.
    TrickEnd,
}

/// Plans qui jouent déjà l'animation eux-mêmes (ou filment depuis l'habitacle) : pas de double effet.
const SELF_DRIVEN: &[&str] = &["takeoff", "touchdown", "dodge-pass", "cockpit"];

/// Part du roulis et de l'assiette de la caisse que la caméra reprend.
const ROLL_SHARE: f32 = 0.55;
const PITCH_SHARE: f32 = 0.4;
/// Ouverture du champ (°) au plus fort du basculement des roues, par m/s de montée, et dans une esquive.
const HOVER_FOV: f32 = 7.0;
const CLIMB_FOV: f32 = 0.55;
const DODGE_FOV: f32 = 3.5;
/// Part du rebond de la caisse (m) reprise par la hauteur de la caméra.
const BOUNCE_SHARE: f32 = 0.22;
/// Secousses (0 → 1) rendues à l'écran : la caisse se pose, les roues se déploient.
const SHAKE_TOUCHDOWN: f32 = 0.7;
const SHAKE_TAKEOFF: f32 = 0.35;
/// En dessous de cette altitude (m), une descente franche est une approche d'atterrissage.
const LANDING_HEIGHT: f32 = 10.0;

/// Le plan à l'antenne, pour la synchro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncTarget {
    /// Identifiant du plan.
    pub id: &'static str,
    /// Plan voiture.
    pub follows_car: bool,
}

/// Synchro caméra / voiture.
#[derive(Debug, Clone, Default)]
pub struct CarSync {
    roll: f32,
    pitch: f32,
    fov: f32,
    lift: f32,
    gain: f32,
    shake: f32,
    phase: FlightPhase,
    dodge: i8,
    tricking: bool,
    landing: bool,
    pending: Option<CarEventKind>,
    last: Option<(CarEventKind, f64)>,
}

impl CarSync {
    /// Repart de zéro.
    pub fn reset(&mut self) {
        *self = Self {
            last: self.last,
            ..Self::default()
        };
    }

    /// Le dernier événement non lu qui peut faire changer de plan (décollage, descente, esquive), puis rien jusqu'au
    /// suivant.
    pub fn take_event(&mut self) -> Option<CarEventKind> {
        self.pending.take()
    }

    /// Secousse à rendre à l'écran (0 → 1), lue une fois.
    pub fn take_shake(&mut self) -> f32 {
        std::mem::take(&mut self.shake)
    }

    /// Le dernier événement et son instant (horloge du monde).
    #[must_use]
    pub fn last_event(&self) -> Option<(CarEventKind, f64)> {
        self.last
    }

    /// À appeler une fois par image, avant [`Self::apply`] : repère les changements d'animation.
    pub fn watch(&mut self, world: &CinemaWorld) {
        let a = world.animation;
        if a.phase != self.phase {
            match (self.phase, a.phase) {
                (_, FlightPhase::Deploying) => self.emit(CarEventKind::Takeoff, world, SHAKE_TAKEOFF),
                (FlightPhase::Deploying, FlightPhase::Flying) => self.emit(CarEventKind::Airborne, world, 0.0),
                (_, FlightPhase::Ground) => self.emit(CarEventKind::Touchdown, world, SHAKE_TOUCHDOWN),
                _ => {}
            }
            self.phase = a.phase;
        }
        // L'approche : la voiture descend vers la route (les roues ne se replient qu'à la toute fin).
        let approaching = a.phase == FlightPhase::Retracting
            || (a.phase == FlightPhase::Flying && a.climb < -2.0 && world.altitude < LANDING_HEIGHT);
        if approaching && !self.landing {
            self.landing = true;
            self.emit(CarEventKind::Descent, world, 0.0);
        } else if self.landing
            && (a.phase == FlightPhase::Ground || (a.phase == FlightPhase::Flying && a.climb > -0.3))
        {
            self.landing = false;
        }
        if a.dodge != self.dodge {
            let kind = if a.dodge == 0 {
                CarEventKind::DodgeEnd
            } else {
                CarEventKind::Dodge
            };
            self.emit(kind, world, 0.0);
            self.dodge = a.dodge;
        }
        let tricking = world.trick.is_some();
        if tricking != self.tricking {
            let kind = if tricking {
                CarEventKind::Trick
            } else {
                CarEventKind::TrickEnd
            };
            self.emit(kind, world, 0.0);
            self.tricking = tricking;
        }
    }

    /// Ajoute le mouvement de la caisse à la pose du plan. `target` : le plan en cours (`None` sans plan). Le gain
    /// monte et descend doucement : passer d'un plan voiture à un autre plan n'a aucun à-coup.
    pub fn apply(&mut self, world: &CinemaWorld, out: &mut CameraPose, target: Option<SyncTarget>) {
        let a = world.animation;
        let dt = world.dt;
        let wants = target.is_some_and(|t| t.follows_car && !SELF_DRIVEN.contains(&t.id))
            && !out.cabin
            && world.trick.is_none();
        self.gain += (if wants { 1.0 } else { 0.0 } - self.gain) * follow(dt, 3.0);

        // Le basculement des roues ouvre le champ au milieu de sa course, puis le referme.
        let hover_dip = if matches!(a.phase, FlightPhase::Deploying | FlightPhase::Retracting) {
            (a.hover.clamp(0.0, 1.0) * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        let dodge_fov = if a.dodge == 0 { 0.0 } else { DODGE_FOV };
        let want_fov = hover_dip * HOVER_FOV + a.climb.abs().clamp(0.0, 11.0) * CLIMB_FOV + dodge_fov;
        self.fov += (want_fov - self.fov) * follow(dt, 5.0);
        self.roll += (a.roll * ROLL_SHARE - self.roll) * follow(dt, 6.0);
        self.pitch += (a.pitch * PITCH_SHARE - self.pitch) * follow(dt, 6.0);
        self.lift += (a.bounce * BOUNCE_SHARE - self.lift) * follow(dt, 14.0);

        let g = self.gain;
        if g < 0.01 {
            return;
        }
        out.fov_deg = (out.fov_deg + self.fov * g).clamp(FOV_MIN, FOV_MAX);
        out.position.y += f64::from(self.lift * g);
        // Roulis autour de l'axe du regard, assiette autour de l'axe droit de la caméra.
        let forward = out.rotation * Vec3::NEG_Z;
        out.rotation = Quat::from_axis_angle(forward, self.roll * g) * out.rotation;
        let right = out.rotation * Vec3::X;
        out.rotation = (Quat::from_axis_angle(right, self.pitch * g) * out.rotation).normalize();
    }

    fn emit(&mut self, kind: CarEventKind, world: &CinemaWorld, shake: f32) {
        tracing::debug!(
            target: "cinema::car",
            ?kind,
            altitude = world.altitude,
            hover = world.animation.hover,
            climb = world.animation.climb,
            dodge = world.animation.dodge,
            "animation de la voiture"
        );
        self.last = Some((kind, world.time));
        self.shake = self.shake.max(shake);
        // Seuls les débuts d'animation peuvent faire changer de plan.
        if matches!(kind, CarEventKind::Takeoff | CarEventKind::Descent | CarEventKind::Dodge) {
            self.pending = Some(kind);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decollage_puis_atterrissage() {
        let mut sync = CarSync::default();
        let mut world = CinemaWorld::default();
        world.animation.phase = FlightPhase::Deploying;
        sync.watch(&world);
        assert_eq!(sync.take_event(), Some(CarEventKind::Takeoff));
        assert!((sync.take_shake() - SHAKE_TAKEOFF).abs() < 1e-6);
        world.animation.phase = FlightPhase::Flying;
        sync.watch(&world);
        assert_eq!(sync.take_event(), None, "l'envol n'est pas un début d'animation");
        world.animation.phase = FlightPhase::Retracting;
        sync.watch(&world);
        assert_eq!(sync.take_event(), Some(CarEventKind::Descent));
        world.animation.phase = FlightPhase::Ground;
        sync.watch(&world);
        assert_eq!(sync.last_event().map(|e| e.0), Some(CarEventKind::Touchdown));
    }

    #[test]
    fn la_synchro_ne_touche_que_les_plans_voiture() {
        let mut sync = CarSync::default();
        let mut world = CinemaWorld {
            dt: 0.02,
            ..CinemaWorld::default()
        };
        world.animation.roll = 0.5;
        world.animation.bounce = 1.0;
        let mut pose = CameraPose::default();
        for _ in 0..200 {
            sync.apply(&world, &mut pose, Some(SyncTarget { id: "orbit", follows_car: false }));
        }
        assert_eq!(pose, CameraPose::default());
        let target = Some(SyncTarget { id: "chase", follows_car: true });
        for _ in 0..200 {
            pose = CameraPose::default();
            sync.apply(&world, &mut pose, target);
        }
        assert!((pose.position.y - 0.22).abs() < 0.01, "{}", pose.position.y);
        assert!(pose.rotation.angle_between(Quat::IDENTITY) > 0.2);
    }
}
