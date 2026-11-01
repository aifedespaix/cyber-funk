//! Animation des motos du trafic (X-10), sans squelette : les roues tournent, la moto plonge au freinage et se cabre à
//! l'accélération, le pilote respire à l'arrêt, se couche vers l'arrière quand ça accélère et se rééquilibre dans un wheeling.
//!
//! Tout se fait en déplaçant des nœuds entiers du GLB (le lecteur du jeu ne lit ni peau ni animation) : `spin_F` et `spin_R`
//! (roues, axe X, origine sur l'essieu) et `rider` (pilote figé, origine au bassin). Le pilote bouge donc d'un bloc autour de
//! sa selle ; les bras et les jambes ne se plient pas.
//!
//! Repère du modèle : l'avant est +Z, l'axe des roues est X. Une rotation positive autour de X fait avancer le haut d'un
//! objet : c'est le sens de roulage d'une roue, et celui d'un buste qui se penche en avant.
//!
//! Ici seulement la logique pure ([`MotoRig`], [`MotoMotion`], [`Reaction`]) ; le système qui la nourrit de la flotte est dans
//! `traffic::render::moto_sync`.

use std::f32::consts::TAU;

use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::math::{Quat, Vec3};
use bevy::transform::components::Transform;

use super::moto::MOTO_COUNT;
use super::spawn::MotoNodes;

/// Diamètre du pneu de chaque roue, en m dans le repère du modèle (avant, arrière) : mesuré sur les GLB, voir
/// `tools/moto/split_wheels.py`. Un test relit les GLB et vérifie ces valeurs.
pub const WHEEL_DIAMETER_M: [[f32; 2]; MOTO_COUNT] = [[0.55, 0.48], [0.73, 0.73], [0.61, 0.72]];

/// Au plus ce pas de rotation par image (rad) : au-delà, la roue serait échantillonnée à plus d'un tour par image et
/// tournerait à l'envers à l'écran (effet de roue de chariot). Elle garde un rythme rapide, toujours dans le bon sens.
pub const MAX_SPIN_STEP: f32 = 0.9;

/// Plongée maximale au freinage et cabrage maximal à l'accélération, en rad (quelques centimètres à l'avant).
const DIVE_MAX: f32 = -0.05;
const SQUAT_MAX: f32 = 0.03;
/// Gain de la plongée (rad par m/s² d'accélération).
const DIVE_GAIN: f32 = 0.0035;
/// Le pilote se couche vers l'arrière en accélérant (rad par m/s²), jusqu'à cette limite.
const LEAN_BACK_GAIN: f32 = 0.012;
const LEAN_LIMIT: f32 = 0.12;
/// Dans un wheeling, le pilote se redresse de cette part du cabrage de la moto.
const WHEELIE_COUNTER: f32 = 0.35;
/// Respiration au repos : amplitude de l'inclinaison (rad) et du bercement vertical (m), fréquence (Hz).
const BREATH_TILT: f32 = 0.014;
const BREATH_RISE_M: f32 = 0.006;
const BREATH_HZ: f32 = 0.7;
/// Constantes de temps (s⁻¹) du lissage de l'accélération et de la posture du pilote.
const ACCEL_SMOOTHING: f32 = 6.0;
const POSTURE_SMOOTHING: f32 = 8.0;

/// Les entités animées d'une moto du pool et son état d'animation. Posé une fois, au démarrage, sur le corps du véhicule.
#[derive(Component, Debug, Clone)]
pub struct MotoRig {
    /// Quelle moto (0 à 2) : choisit les diamètres de roue.
    pub which: usize,
    /// La racine du modèle, qui porte le calage (`moto::fit`) : elle plonge et se cabre.
    pub model: Entity,
    /// Le calage de repos de cette racine.
    pub fit: Transform,
    /// Roues et pilote.
    pub nodes: MotoNodes,
    /// État de l'animation.
    pub motion: MotoMotion,
}

/// L'état d'animation d'une moto : de quoi calculer l'image suivante sans rien allouer.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MotoMotion {
    /// Dernière génération du véhicule vue : un changement remet l'état à zéro (nouvelle moto dans le même emplacement).
    pub generation: u32,
    /// Angles des roues, rad, entre 0 et τ.
    pub angle_front: f32,
    /// Idem, arrière.
    pub angle_rear: f32,
    /// Vitesse à l'image précédente, m/s.
    pub speed: f32,
    /// Accélération lissée, m/s².
    pub accel: f32,
    /// Cabrage à l'image précédente, rad.
    pub pitch: f32,
    /// Inclinaison actuelle du pilote autour du bassin, rad (positif : vers l'avant).
    pub rider_tilt: f32,
    /// Horloge de respiration, s (décalée par moto).
    pub breath: f32,
}

/// Ce que l'animation décide pour l'image : à écrire dans les transformations des nœuds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reaction {
    /// Rotation des roues autour de X, rad.
    pub front: f32,
    /// Idem, arrière.
    pub rear: f32,
    /// Plongée (négatif) ou cabrage (positif) de la moto entière, autour de l'essieu arrière, rad.
    pub dive: f32,
    /// Inclinaison du pilote autour du bassin, rad.
    pub rider_tilt: f32,
    /// Bercement vertical du pilote, m.
    pub rider_rise: f32,
}

/// Le pas de rotation d'une roue de rayon `radius` (m) qui roule à `speed` (m/s) pendant `dt` s, borné par [`MAX_SPIN_STEP`].
#[must_use]
pub fn spin_step(speed: f32, dt: f32, radius: f32) -> f32 {
    if radius <= 1e-4 {
        return 0.0;
    }
    (speed * dt / radius).clamp(0.0, MAX_SPIN_STEP)
}

/// La plongée de la moto pour une accélération `accel` (m/s²) : négative en freinant (le nez descend), positive en
/// accélérant (la moto s'assoit).
#[must_use]
pub fn dive_for(accel: f32) -> f32 {
    (accel * DIVE_GAIN).clamp(DIVE_MAX, SQUAT_MAX)
}

/// L'inclinaison visée du pilote : en arrière quand ça accélère, en avant quand ça freine, et il se redresse dans un wheeling
/// (`pitch` : cabrage de la moto, rad ; `pitch_rate` : sa vitesse, rad/s, pour se jeter en arrière au départ du wheeling).
#[must_use]
pub fn rider_target(accel: f32, pitch: f32, pitch_rate: f32) -> f32 {
    let lean = (-accel * LEAN_BACK_GAIN).clamp(-LEAN_LIMIT, LEAN_LIMIT);
    let wheelie = WHEELIE_COUNTER * pitch - 0.06 * pitch_rate.clamp(-3.0, 3.0);
    lean + wheelie
}

impl MotoMotion {
    /// Une moto qui vient de naître : l'état repart de zéro, les roues à un angle quelconque (`phase`, 0..1) et la respiration
    /// décalée, pour que deux motos côte à côte ne soient pas synchrones.
    pub fn reset(&mut self, generation: u32, speed: f32, phase: f32) {
        *self = Self {
            generation,
            angle_front: phase * TAU,
            angle_rear: (phase * 7.0).fract() * TAU,
            speed,
            breath: phase * 10.0,
            ..Self::default()
        };
    }

    /// Avance d'une image de `dt` s : la moto roule à `speed` m/s, cabrée de `pitch` rad ; `radii` (m) : rayons des pneus,
    /// avant puis arrière (dans le monde : modèle × calage × échelle du véhicule).
    pub fn step(&mut self, dt: f32, speed: f32, pitch: f32, radii: [f32; 2]) -> Reaction {
        let dt = dt.clamp(0.0, 0.25);
        self.angle_front = (self.angle_front + spin_step(speed, dt, radii[0])) % TAU;
        self.angle_rear = (self.angle_rear + spin_step(speed, dt, radii[1])) % TAU;
        let (raw_accel, pitch_rate) = if dt > 1e-4 {
            ((speed - self.speed) / dt, (pitch - self.pitch) / dt)
        } else {
            (self.accel, 0.0)
        };
        let k = 1.0 - (-dt * ACCEL_SMOOTHING).exp();
        self.accel += (raw_accel.clamp(-40.0, 40.0) - self.accel) * k;
        self.speed = speed;
        self.pitch = pitch;
        self.breath = (self.breath + dt) % (1.0 / BREATH_HZ * 100.0);
        let wave = (self.breath * BREATH_HZ * TAU).sin();
        let target = rider_target(self.accel, pitch, pitch_rate) + BREATH_TILT * wave;
        self.rider_tilt += (target - self.rider_tilt) * (1.0 - (-dt * POSTURE_SMOOTHING).exp());
        Reaction {
            front: self.angle_front,
            rear: self.angle_rear,
            dive: dive_for(self.accel),
            rider_tilt: self.rider_tilt,
            rider_rise: BREATH_RISE_M * wave,
        }
    }
}

impl Reaction {
    /// Transformation d'une roue.
    #[must_use]
    pub fn wheel(angle: f32) -> Quat {
        Quat::from_rotation_x(angle)
    }

    /// Transformation du pilote : sa translation de repos (le bassin) plus le bercement, tournée autour du bassin.
    #[must_use]
    pub fn rider(&self, rest: Vec3) -> Transform {
        Transform {
            translation: rest + Vec3::Y * self.rider_rise,
            rotation: Quat::from_rotation_x(self.rider_tilt),
            scale: Vec3::ONE,
        }
    }

    /// Transformation de la racine du modèle : son calage, plongé autour de l'axe des roues arrière (l'origine du corps).
    #[must_use]
    pub fn model(&self, fit: Transform) -> Transform {
        let dive = Quat::from_rotation_x(self.dive);
        Transform {
            translation: dive * fit.translation,
            rotation: dive * fit.rotation,
            scale: fit.scale,
        }
    }
}

/// Le rayon d'un pneu dans le monde : rayon du modèle, grossi par le calage et l'échelle du véhicule.
#[must_use]
pub fn world_radius(which: usize, front: bool, fit_scale: f32, craft_scale: f32) -> f32 {
    WHEEL_DIAMETER_M[which % MOTO_COUNT][usize::from(!front)] * 0.5 * fit_scale * craft_scale
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Une roue qui roule à `v` fait `v / r` radians par seconde : la moto avance d'une circonférence par tour.
    #[test]
    fn la_roue_avance_d_une_circonference_par_tour() {
        let r = 0.4;
        let step = spin_step(10.0, 0.01, r);
        assert!((step - 0.25).abs() < 1e-6, "{step}");
    }

    /// À grande vitesse, le pas est borné : sinon la roue paraîtrait tourner à l'envers.
    #[test]
    fn le_pas_de_rotation_est_borne() {
        assert!((spin_step(80.0, DT, 0.3) - MAX_SPIN_STEP).abs() < 1e-6);
        assert_eq!(spin_step(-5.0, DT, 0.3), 0.0);
        assert_eq!(spin_step(10.0, DT, 0.0), 0.0);
    }

    #[test]
    fn les_roues_tournent_dans_le_sens_de_la_marche() {
        let mut m = MotoMotion::default();
        m.reset(1, 20.0, 0.0);
        let a0 = m.angle_rear;
        m.step(DT, 20.0, 0.0, [0.5, 0.5]);
        assert!(m.angle_rear > a0, "la roue arrière avance");
        assert!(m.angle_rear < TAU && m.angle_front < TAU);
    }

    /// À l'arrêt, les roues ne bougent pas ; le pilote respire quand même.
    #[test]
    fn a_l_arret_les_roues_sont_immobiles_et_le_pilote_respire() {
        let mut m = MotoMotion::default();
        m.reset(1, 0.0, 0.3);
        let first = m.step(DT, 0.0, 0.0, [0.5, 0.5]);
        let mut tilts = vec![first.rider_tilt];
        for _ in 0..120 {
            let r = m.step(DT, 0.0, 0.0, [0.5, 0.5]);
            assert!((r.front - first.front).abs() < 1e-6);
            tilts.push(r.rider_tilt);
        }
        let (lo, hi) = tilts
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, h), &t| (l.min(t), h.max(t)));
        assert!(hi - lo > 0.015, "le pilote bouge à l'arrêt : {lo}..{hi}");
        assert!(hi.abs() < 0.03 && lo.abs() < 0.03, "mais peu : {lo}..{hi}");
    }

    #[test]
    fn le_freinage_fait_plonger_le_nez_et_l_acceleration_asseoit_la_moto() {
        assert!(dive_for(-15.0) < -0.03);
        assert!(dive_for(10.0) > 0.0);
        assert!(dive_for(-500.0) >= DIVE_MAX && dive_for(500.0) <= SQUAT_MAX);
    }

    #[test]
    fn le_pilote_se_couche_en_arriere_en_accelerant_et_se_penche_en_freinant() {
        assert!(rider_target(8.0, 0.0, 0.0) < -0.05);
        assert!(rider_target(-8.0, 0.0, 0.0) > 0.05);
        assert!(rider_target(1000.0, 0.0, 0.0) >= -LEAN_LIMIT);
    }

    /// Dans un wheeling établi, le pilote se redresse (penché vers l'avant par rapport à la moto) ; au départ, il part en arrière.
    #[test]
    fn dans_un_wheeling_le_pilote_se_redresse_apres_s_etre_jete_en_arriere() {
        assert!(rider_target(0.0, 0.9, 0.0) > 0.25, "établi : en avant");
        assert!(
            rider_target(0.0, 0.2, 2.0) < rider_target(0.0, 0.2, 0.0),
            "au départ : en arrière"
        );
    }

    /// Une accélération brusque est lissée : la plongée n'a pas de saut d'une image à l'autre.
    #[test]
    fn la_reaction_est_continue() {
        let mut m = MotoMotion::default();
        m.reset(1, 20.0, 0.0);
        let mut previous = m.step(DT, 20.0, 0.0, [0.5, 0.5]);
        for i in 0..120 {
            let speed = if i < 30 { 20.0 } else { 8.0 };
            let r = m.step(DT, speed, 0.0, [0.5, 0.5]);
            assert!((r.dive - previous.dive).abs() < 0.02, "plongée discontinue à {i}");
            assert!(
                (r.rider_tilt - previous.rider_tilt).abs() < 0.05,
                "pilote discontinu à {i}"
            );
            previous = r;
        }
        assert!(previous.dive.is_finite() && previous.rider_tilt.is_finite());
    }

    #[test]
    fn le_pilote_pivote_autour_du_bassin_et_la_moto_autour_de_l_essieu_arriere() {
        let r = Reaction {
            front: 0.0,
            rear: 0.0,
            dive: -0.05,
            rider_tilt: 0.1,
            rider_rise: 0.005,
        };
        let rest = Vec3::new(0.0, 0.95, -0.2);
        let t = r.rider(rest);
        assert!(
            (t.translation - rest - Vec3::Y * 0.005).length() < 1e-6,
            "le bassin ne bouge que du bercement"
        );
        let fit = Transform::from_xyz(0.0, -0.3, -1.0);
        let m = r.model(fit);
        assert!(m.translation.is_finite() && (m.translation.length() - fit.translation.length()).abs() < 1e-5);
    }

    #[test]
    fn un_nouvel_occupant_repart_de_zero() {
        let mut m = MotoMotion::default();
        m.reset(1, 30.0, 0.5);
        for _ in 0..50 {
            m.step(DT, 5.0, 0.4, [0.5, 0.5]);
        }
        m.reset(2, 12.0, 0.1);
        assert_eq!((m.generation, m.accel, m.rider_tilt, m.pitch), (2, 0.0, 0.0, 0.0));
        assert!((m.speed - 12.0).abs() < 1e-6);
    }

    #[test]
    fn les_rayons_dependent_de_la_moto_et_de_l_echelle() {
        let small = world_radius(0, true, 1.0, 1.0);
        let big = world_radius(0, true, 1.19, 1.3);
        assert!((small - 0.275).abs() < 1e-6 && big > small);
        assert!(
            world_radius(2, true, 1.0, 1.0) < world_radius(2, false, 1.0, 1.0),
            "moto 3 : roue avant plus petite"
        );
    }
}
