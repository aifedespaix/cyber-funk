//! La cinématique directe d'un robot : les pivots emboîtés deviennent la pose de chaque pièce.

use bevy::math::{EulerRot, Mat4, Quat, Vec3, Vec4};
use bevy::transform::components::Transform;

use super::{FORE, LEG_DOWN, LEG_H, Robot, TORSO_D, TORSO_H, TORSO_W, UPPER};
use crate::landmarks::part::PartPaint;

impl Robot {
    /// Écrit la pose et les paramètres des quatorze pièces dans `out` (dans l'ordre de [`parts`]) et retient la position
    /// de la poitrine pour la lumière.
    pub fn paint(&mut self, out: &mut [PartPaint]) {
        let spec = self.spec;
        let side = f32::from(spec.side);
        let (pose, pulse, bar, time) = (self.pose, self.pulse, self.bar, self.time);
        let (ls, le, rs, re) = if spec.mirror {
            (
                pose.right_shoulder,
                pose.right_elbow,
                pose.left_shoulder,
                pose.left_elbow,
            )
        } else {
            (
                pose.left_shoulder,
                pose.left_elbow,
                pose.right_shoulder,
                pose.right_elbow,
            )
        };
        let twist = (if spec.mirror { -pose.twist } else { pose.twist }) * spec.twist_gain;
        let torso_y =
            twist + (bar * std::f32::consts::FRAC_PI_2).sin() * 0.1 * spec.twist_gain + (time * 0.9).sin() * 0.02;
        let torso_rot = Quat::from_euler(EulerRot::XYZ, -0.05 * pulse, torso_y, 0.0);
        let head_rot = Quat::from_euler(
            EulerRot::XYZ,
            -0.2 * pulse,
            ((bar * std::f32::consts::FRAC_PI_4) * spec.look_speed).sin() * 0.35,
            (bar * std::f32::consts::PI).sin() * 0.06,
        );
        let drop = pose.squat * 2.5 + pulse * 1.2;
        let hips_y = LEG_H - drop;
        let leg_scale = (LEG_H - drop) / LEG_H;

        let facing = Quat::from_rotation_y(if spec.side > 0 {
            -std::f32::consts::FRAC_PI_2
        } else {
            std::f32::consts::FRAC_PI_2
        });
        let group = Mat4::from_scale_rotation_translation(
            Vec3::splat(spec.scale),
            facing,
            Vec3::new(side * spec.x, 0.0, spec.z),
        );
        let t = |x: f32, y: f32, z: f32| Mat4::from_translation(Vec3::new(x, y, z));
        let sc = |x: f32, y: f32, z: f32| Mat4::from_scale(Vec3::new(x, y, z));

        let torso = group * t(0.0, hips_y, 0.0) * Mat4::from_quat(torso_rot);
        let head = torso * t(0.0, TORSO_H + 4.0, 0.0) * Mat4::from_quat(head_rot);
        let mut seed = spec.seed;
        let mut put = |slot: usize, matrix: Mat4, size: Vec3, out: &mut [PartPaint], count_seed: bool| {
            let this_seed = seed;
            if count_seed {
                seed += 1;
            }
            if let Some(part) = out.get_mut(slot) {
                *part = PartPaint {
                    visible: true,
                    transform: Transform::from_matrix(matrix * Mat4::from_scale(size)),
                    mood: self.tone,
                    a: (size * 0.5).extend(this_seed as f32),
                    c: Vec4::new(self.high, self.bass, 0.0, 0.0),
                    ..PartPaint::default()
                };
            }
        };
        // Jambes : deux piliers qui plongent sous la route, mis à l'échelle depuis le sol pour fléchir sur les temps forts.
        for (slot, x) in [(0, -9.0), (1, 9.0)] {
            put(
                slot,
                group * t(x, 0.0, 0.0) * sc(1.0, leg_scale, 1.0) * t(0.0, (LEG_H - LEG_DOWN) / 2.0, 0.0),
                Vec3::new(10.0, LEG_H + LEG_DOWN, 14.0),
                out,
                true,
            );
        }
        put(
            2,
            torso * t(0.0, TORSO_H / 2.0, 0.0),
            Vec3::new(TORSO_W, TORSO_H, TORSO_D),
            out,
            true,
        );
        put(
            3,
            torso * t(0.0, TORSO_H + 2.0, 0.0),
            Vec3::new(8.0, 4.0, 8.0),
            out,
            true,
        );
        put(4, head * t(0.0, 10.0, 0.0), Vec3::new(24.0, 20.0, 18.0), out, true);
        // L'écran : un plan sur la face de la tête, sans échelle (le maillage fait déjà 20 × 16).
        if let Some(screen) = out.get_mut(5) {
            *screen = PartPaint {
                visible: true,
                transform: Transform::from_matrix(head * t(0.0, 10.0, 9.06)),
                mood: self.tone,
                a: Vec4::new(f32::from(self.current as u8), self.swap, 0.0, 0.0),
                ..PartPaint::default()
            };
        }
        // Épaule → bras → coude → avant-bras → main : trois pivots emboîtés par côté. Le bras gauche tourne dans le sens
        // inverse du droit (`rotation.z = −angle` à gauche).
        for (base, sign, shoulder_angle, elbow_angle) in [(6, -1.0_f32, -ls, -le), (10, 1.0, rs, re)] {
            let shoulder =
                torso * t(sign * (TORSO_W / 2.0 + 4.0), TORSO_H - 5.0, 0.0) * Mat4::from_rotation_z(shoulder_angle);
            let elbow = shoulder * t(0.0, -UPPER, 0.0) * Mat4::from_rotation_z(elbow_angle);
            put(base, shoulder, Vec3::splat(9.0), out, true);
            put(
                base + 1,
                shoulder * t(0.0, -UPPER / 2.0, 0.0),
                Vec3::new(7.0, UPPER, 7.0),
                out,
                true,
            );
            put(
                base + 2,
                elbow * t(0.0, -FORE / 2.0, 0.0),
                Vec3::new(6.2, FORE, 6.2),
                out,
                true,
            );
            put(base + 3, elbow * t(0.0, -FORE - 3.0, 0.0), Vec3::splat(9.0), out, true);
        }
        self.chest = (torso * t(0.0, TORSO_H * 0.6, TORSO_D / 2.0 + 3.0)).transform_point3(Vec3::ZERO);
    }
}
