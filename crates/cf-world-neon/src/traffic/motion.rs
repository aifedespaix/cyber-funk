//! Le mouvement d'un véhicule (`motion.ts`). Sillage donnait à chaque véhicule une fermeture `step` ; ici une énumération,
//! sans allocation ni fermeture : un véhicule qui naît ne crée rien.

use std::f32::consts::TAU;

use bevy::math::{DVec3, Vec3};

use crate::pilot::rand::Rand;

/// Ondulation d'une trajectoire : amplitude et fréquence latérales (x) et verticales (y).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Weave {
    /// Amplitude latérale, m.
    pub ax: f32,
    /// Fréquence latérale, rad/s.
    pub fx: f32,
    /// Amplitude verticale, m.
    pub ay: f32,
    /// Fréquence verticale, rad/s.
    pub fy: f32,
    /// Déphasage, rad.
    pub phase: f32,
}

impl Weave {
    /// Une ondulation d'amplitudes données, aux fréquences et à la phase tirées (`randomWeave`).
    pub fn random(rng: &mut Rand, ax: f32, ay: f32) -> Self {
        Self {
            ax,
            fx: 0.5 + rng.unit() as f32 * 1.1,
            ay,
            fy: 0.4 + rng.unit() as f32 * 0.9,
            phase: rng.unit() as f32 * TAU,
        }
    }
}

/// Comment un véhicule avance.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Motion {
    /// Trajectoire rectiligne : `position += vitesse × dt` (le mouvement par défaut de Sillage).
    #[default]
    Linear,
    /// « Ligne droite + ondulation », évaluée au temps `âge + offset` (`followPath`). Un `offset` négatif fait suivre
    /// exactement le même chemin avec retard : c'est ainsi que la voiture de police colle au véhicule poursuivi.
    Path {
        /// Point de départ (absolu, m).
        start: DVec3,
        /// Vitesse dans le monde, m/s.
        velocity: Vec3,
        /// Ondulation, s'il y en a une.
        weave: Option<Weave>,
        /// Retard (< 0) ou avance (> 0) sur le chemin, s.
        offset: f32,
    },
    /// L'appelant écrit la position et la vitesse à chaque image (avant la mise à jour) : la flotte ne l'avance pas.
    Scripted,
}

impl Motion {
    /// Une trajectoire `followPath`.
    #[must_use]
    pub const fn path(start: DVec3, velocity: Vec3, weave: Option<Weave>, offset: f32) -> Self {
        Self::Path {
            start,
            velocity,
            weave,
            offset,
        }
    }

    /// Avance le véhicule de `dt` secondes. `age` est déjà à jour. Pas d'effet pour [`Motion::Scripted`].
    pub fn step(&self, position: &mut DVec3, velocity: &mut Vec3, age: f32, dt: f32) {
        match *self {
            Self::Linear => *position += velocity.as_dvec3() * f64::from(dt),
            Self::Scripted => {}
            Self::Path {
                start,
                velocity: v,
                weave,
                offset,
            } => {
                let t = age + offset;
                *position = start + v.as_dvec3() * f64::from(t);
                *velocity = v;
                if let Some(w) = weave {
                    let px = w.fx * t + w.phase;
                    let py = w.fy * t + w.phase * 1.7;
                    position.x += f64::from(w.ax * px.sin());
                    position.y += f64::from(w.ay * py.sin());
                    velocity.x += w.ax * w.fx * px.cos();
                    velocity.y += w.ay * w.fy * py.cos();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_trajectoire_rectiligne_integre_la_vitesse() {
        let mut p = DVec3::ZERO;
        let mut v = Vec3::new(0.0, 0.0, -60.0);
        for _ in 0..60 {
            Motion::Linear.step(&mut p, &mut v, 0.0, 1.0 / 60.0);
        }
        assert!((p.z + 60.0).abs() < 1e-3, "{}", p.z);
    }

    #[test]
    fn un_chemin_ne_depend_que_de_l_age_donc_pas_de_la_cadence() {
        let mut rng = Rand::new(3);
        let weave = Some(Weave::random(&mut rng, 1.0, 0.5));
        let motion = Motion::path(DVec3::new(2.0, 10.0, -5.0), Vec3::new(0.0, 0.0, -80.0), weave, 0.0);
        let (mut a, mut av) = (DVec3::ZERO, Vec3::ZERO);
        let (mut b, mut bv) = (DVec3::ZERO, Vec3::ZERO);
        // Un pas de 1/60 s et un pas de 8/60 s qui tombent au même âge : même position.
        motion.step(&mut a, &mut av, 8.0 / 60.0, 1.0 / 60.0);
        motion.step(&mut b, &mut bv, 8.0 / 60.0, 8.0 / 60.0);
        assert_eq!(a, b);
        assert_eq!(av, bv);
    }

    #[test]
    fn un_retard_negatif_suit_le_meme_chemin() {
        let mut rng = Rand::new(5);
        let weave = Some(Weave::random(&mut rng, 1.2, 0.4));
        let lead = Motion::path(DVec3::ZERO, Vec3::new(0.0, 0.0, -70.0), weave, 0.0);
        let chaser = Motion::path(DVec3::ZERO, Vec3::new(0.0, 0.0, -70.0), weave, -0.5);
        let (mut p1, mut v1, mut p2, mut v2) = (DVec3::ZERO, Vec3::ZERO, DVec3::ZERO, Vec3::ZERO);
        lead.step(&mut p1, &mut v1, 1.0, 0.0);
        chaser.step(&mut p2, &mut v2, 1.5, 0.0);
        assert_eq!(p1, p2);
    }

    #[test]
    fn un_vehicule_scenarise_n_est_pas_avance() {
        let mut p = DVec3::new(1.0, 2.0, 3.0);
        let mut v = Vec3::X;
        Motion::Scripted.step(&mut p, &mut v, 5.0, 1.0);
        assert_eq!(p, DVec3::new(1.0, 2.0, 3.0));
    }
}
