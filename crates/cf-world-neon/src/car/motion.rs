//! Le mouvement de la carrosserie et des roues : suspension sur ressorts, dérive de l'arrière, rotation des roues,
//! braquage. Logique pure (aucun Bevy), testable seule.
//!
//! Portage du contrôleur du modèle (`CyberCar.ts` : `updateSuspension`, `update`) et de `CyberCarModel.update`
//! (`cyber-car.ts`) pour ce qui reste en mode cinéma : le jeu (boîte de vitesses, pédales, compteur, autoradio,
//! essuie-glaces) n'est pas repris.

use bevy::math::{EulerRot, Quat, Vec3};

/// Pesanteur, m/s² : les accélérations de la voiture sont converties en g pour la suspension.
pub const G: f32 = 9.81;
/// Rayon du pneu, m (`WHEEL_RADIUS`).
pub const WHEEL_RADIUS: f32 = 0.39;
/// Braquage maximal des roues avant, rad (`MAX_WHEEL_STEER`, ~31°).
pub const MAX_WHEEL_STEER: f32 = 0.55;
/// Tangage par g d'accélération longitudinale, rad (~2,3°/g).
const PITCH_PER_G: f32 = 0.04;
/// Roulis par g d'accélération latérale, rad (~2,9°/g).
const ROLL_PER_G: f32 = 0.05;
/// Raideur du ressort de la caisse (1/s²) : fréquence propre ~1,5 Hz.
const SUSP_K: f32 = 85.0;
/// Amortissement (1/s) : légèrement sous-amorti, un dépassement visible.
const SUSP_C: f32 = 9.5;
/// Centre de roulis et de tangage, à hauteur d'essieu (repère du GLB).
pub const SUSP_PIVOT: Vec3 = Vec3::new(0.0, -0.35, 0.0);
/// La suspension du modèle est réaliste (~3° par g) ; le jeu en demande un peu plus pour que les virages se sentent
/// (`SUSPENSION` de `cyber-car.ts`).
const SUSPENSION: f32 = 2.0;
/// Ressort amorti du lacet seul (la dérive de l'arrière) : raideur et amortissement (`SPRING`, `DAMPING`).
const YAW_SPRING: f32 = 70.0;
const YAW_DAMPING: f32 = 8.0;
/// Pas maximal d'intégration, s : un à-coup d'image (chargement, pause) ne fait pas diverger les ressorts.
const MAX_STEP_S: f32 = 0.05;

/// Un ressort amorti à un degré de liberté.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Spring {
    x: f32,
    v: f32,
}

impl Spring {
    fn step(&mut self, target: f32, stiffness: f32, damping: f32, dt: f32) {
        self.v += (stiffness * (target - self.x) - damping * self.v) * dt;
        self.x += self.v * dt;
    }
}

/// Ce que la voiture « fait » à cet instant : écrit par le pilote (J5-06) ou par le point suivi provisoire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionInput {
    /// Durée de l'image, s.
    pub dt_s: f32,
    /// Vitesse, m/s : fait tourner les roues.
    pub speed_mps: f32,
    /// Vitesse rapportée à la croisière (1 : croisière).
    pub speed_ratio: f32,
    /// Vitesse latérale, m/s (positive vers la droite de la route).
    pub lateral_mps: f32,
    /// Accélération longitudinale, m/s² (négative au freinage).
    pub accel_mps2: f32,
    /// Angle du volant, −1 (gauche) à 1 (droite) : braque les roues.
    pub steer: f32,
}

/// La pose qui en résulte, dans le repère du GLB (nez vers +Z).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionPose {
    /// Rotation de la caisse autour de son centre de roulis.
    pub chassis_rotation: Quat,
    /// Translation de la caisse : le centre de roulis reste en place, plus l'affaissement.
    pub chassis_translation: Vec3,
    /// Lacet de la voiture entière (dérive de l'arrière), rad.
    pub yaw_rad: f32,
    /// Angle de roulement des roues, rad (0 à 2π).
    pub wheel_spin_rad: f32,
    /// Braquage des roues avant autour de la verticale, rad.
    pub wheel_steer_rad: f32,
}

/// L'état de la suspension et des roues.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Motion {
    pitch: Spring,
    roll: Spring,
    heave: Spring,
    yaw: Spring,
    /// Accélération latérale filtrée, g (`latG`).
    lat_g: f32,
    previous_lateral: f32,
    wheel_spin: f32,
}

impl Motion {
    /// Avance d'une image.
    pub fn update(&mut self, input: &MotionInput) -> MotionPose {
        let dt = input.dt_s.clamp(0.0, MAX_STEP_S);
        let grip = 0.55 + 0.45 * input.speed_ratio.min(1.0);

        // Lacet seul, sur ressort : la voiture dérive de l'arrière en virage serré.
        let yaw_target = (-input.lateral_mps * 0.012 * grip).clamp(-0.32, 0.32);
        self.yaw.step(yaw_target, YAW_SPRING, YAW_DAMPING, dt);

        // Suspension : nos vraies accélérations, en g. `long_g` > 0 accélère ; `lat_g` > 0 tourne à gauche.
        let lateral_accel = if dt > 0.0 {
            (input.lateral_mps - self.previous_lateral) / dt
        } else {
            0.0
        };
        self.previous_lateral = input.lateral_mps;
        let target = (-lateral_accel / G).clamp(-1.5, 1.5);
        self.lat_g += (target - self.lat_g) * (1.0 - (-dt * 10.0).exp());
        let long_g = (input.accel_mps2 / G).clamp(-1.5, 1.5);
        let k = SUSPENSION;
        self.pitch.step(-long_g * PITCH_PER_G * k, SUSP_K, SUSP_C, dt);
        self.roll.step(self.lat_g * ROLL_PER_G * k, SUSP_K, SUSP_C, dt);
        let heave_target = -(long_g.abs() * 0.006 + self.lat_g.abs() * 0.01) * k;
        self.heave.step(heave_target, SUSP_K, SUSP_C, dt);

        let chassis_rotation = Quat::from_euler(EulerRot::XYZ, self.pitch.x, 0.0, self.roll.x);
        // La caisse pivote autour de `SUSP_PIVOT` : la translation compense pour que ce point reste fixe.
        let chassis_translation = SUSP_PIVOT - chassis_rotation * SUSP_PIVOT + Vec3::Y * self.heave.x;

        // Roues : l'angle avance de ce que la voiture parcourt, braquage proportionnel au volant (le modèle compte le
        // braquage positif vers la gauche : signe inversé).
        self.wheel_spin = (self.wheel_spin + input.speed_mps * dt / WHEEL_RADIUS).rem_euclid(std::f32::consts::TAU);
        MotionPose {
            chassis_rotation,
            chassis_translation,
            yaw_rad: self.yaw.x,
            wheel_spin_rad: self.wheel_spin,
            wheel_steer_rad: -input.steer.clamp(-1.0, 1.0) * MAX_WHEEL_STEER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> MotionInput {
        MotionInput {
            dt_s: 1.0 / 60.0,
            speed_mps: 40.0,
            speed_ratio: 1.0,
            lateral_mps: 0.0,
            accel_mps2: 0.0,
            steer: 0.0,
        }
    }

    fn run(motion: &mut Motion, frames: usize, input: &MotionInput) -> MotionPose {
        let mut pose = motion.update(input);
        for _ in 1..frames {
            pose = motion.update(input);
        }
        pose
    }

    #[test]
    fn a_vitesse_constante_la_caisse_est_droite() {
        let mut motion = Motion::default();
        let pose = run(&mut motion, 120, &input());
        assert!(pose.chassis_rotation.angle_between(Quat::IDENTITY) < 1e-5);
        assert!(pose.chassis_translation.length() < 1e-5);
        assert!(pose.yaw_rad.abs() < 1e-5);
    }

    /// Au freinage (accélération négative) le nez plonge : rotation positive autour de +X pour un nez vers +Z.
    #[test]
    fn au_freinage_le_nez_plonge_puis_la_caisse_se_stabilise() {
        let mut motion = Motion::default();
        let braking = MotionInput {
            accel_mps2: -9.81,
            ..input()
        };
        let pose = run(&mut motion, 240, &braking);
        let (pitch, _, _) = pose.chassis_rotation.to_euler(EulerRot::XYZ);
        // 1 g de freinage : 0,04 × 2 = 0,08 rad (~4,6°), le nez vers +Z plonge.
        assert!((pitch - 0.08).abs() < 0.005, "tangage {pitch}");
        // Le point de pivot reste en place malgré la rotation.
        let pivot_moved = pose.chassis_rotation * SUSP_PIVOT + pose.chassis_translation - SUSP_PIVOT;
        assert!(pivot_moved.x.abs() < 1e-5 && pivot_moved.z.abs() < 1e-5);
    }

    /// Le ressort est sous-amorti : un à-coup dépasse sa cible une fois (`SUSP_C` < 2 √`SUSP_K`).
    #[test]
    fn la_suspension_depasse_sa_cible_une_fois() {
        assert!(SUSP_C < 2.0 * SUSP_K.sqrt());
        let mut motion = Motion::default();
        let braking = MotionInput {
            accel_mps2: -9.81,
            ..input()
        };
        let mut peak = 0.0_f32;
        for _ in 0..180 {
            let (pitch, _, _) = motion.update(&braking).chassis_rotation.to_euler(EulerRot::XYZ);
            peak = peak.max(pitch);
        }
        assert!(peak > 0.08 * 1.05, "dépassement visible : {peak}");
    }

    #[test]
    fn un_virage_roule_la_caisse_et_fait_deriver_l_arriere() {
        let mut motion = Motion::default();
        // Une vitesse latérale qui s'établit vers la droite : accélération latérale puis dérive.
        let mut pose = motion.update(&input());
        for i in 1..=30 {
            pose = motion.update(&MotionInput {
                lateral_mps: i as f32 * 0.4,
                ..input()
            });
        }
        let (_, _, roll) = pose.chassis_rotation.to_euler(EulerRot::XYZ);
        assert!(roll.abs() > 0.005, "roulis {roll}");
        assert!(pose.yaw_rad < 0.0, "lacet vers la droite : {}", pose.yaw_rad);
        assert!(pose.yaw_rad.abs() <= 0.32);
    }

    #[test]
    fn les_roues_tournent_avec_la_vitesse_et_braquent_avec_le_volant() {
        let mut motion = Motion::default();
        let one_second = (0..60).fold(0.0, |_, _| motion.update(&input()).wheel_spin_rad);
        // 40 m/s pendant 1 s = 102,6 rad de roulement, ramenés à 0..2π.
        let expected = (40.0 / WHEEL_RADIUS).rem_euclid(std::f32::consts::TAU);
        assert!((one_second - expected).abs() < 0.05, "{one_second} vs {expected}");
        let right = motion.update(&MotionInput { steer: 1.0, ..input() });
        let left = motion.update(&MotionInput { steer: -1.0, ..input() });
        assert!(
            (right.wheel_steer_rad + MAX_WHEEL_STEER).abs() < 1e-6,
            "{}",
            right.wheel_steer_rad
        );
        assert!((left.wheel_steer_rad - MAX_WHEEL_STEER).abs() < 1e-6);
    }

    #[test]
    fn un_a_coup_d_image_ne_fait_pas_diverger_les_ressorts() {
        let mut motion = Motion::default();
        let jolt = MotionInput {
            dt_s: 2.0,
            accel_mps2: -20.0,
            ..input()
        };
        let pose = motion.update(&jolt);
        assert!(pose.chassis_translation.is_finite() && pose.chassis_rotation.is_finite());
        let pose = run(&mut motion, 600, &input());
        assert!(
            pose.chassis_rotation.angle_between(Quat::IDENTITY) < 1e-3,
            "retour au calme"
        );
    }
}
