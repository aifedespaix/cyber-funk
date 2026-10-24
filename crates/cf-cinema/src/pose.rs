//! Outils de pose (`cinema/pose.ts`) : viser, mélanger deux poses, lisser indépendamment de la cadence.
//!
//! La pose est la [`CameraPose`] de `cf-render`, celle que lit le rig de caméras : position absolue en `f64`, le reste
//! en `f32`. Les directions se calculent en relatif (`cible − œil`, petit nombre exact) : aucune perte de précision à
//! 400 km du départ.

use bevy::math::{DVec3, Mat3, Quat, Vec3};
use cf_render::CameraPose;

/// Distance minimale (m²) entre l'œil et la cible pour qu'un regard ait un sens.
const MIN_AIM_SQ: f32 = 1e-8;

/// Place la caméra en `position`, regard vers `target`, avec un roulis optionnel (radians, autour de l'axe du regard).
/// Ne touche pas au FOV : `cabin` repasse à faux et `sky_drift` à zéro sauf si `keep_sky`.
pub fn aim(out: &mut CameraPose, position: DVec3, target: DVec3, roll: f32, up: Vec3, keep_sky: bool) {
    out.position = position;
    // Une caméra dont l'œil et la cible coïncident n'a pas de regard : on garde l'orientation précédente.
    let dir = (target - position).as_vec3();
    if dir.length_squared() > MIN_AIM_SQ
        && let Some(rotation) = look_rotation(dir, up)
    {
        out.rotation = rotation;
    }
    if roll != 0.0 {
        let axis = out.rotation * Vec3::NEG_Z;
        out.rotation = (Quat::from_axis_angle(axis, roll) * out.rotation).normalize();
    }
    out.cabin = false;
    if !keep_sky {
        out.sky_drift = Vec3::ZERO;
    }
}

/// Orientation d'une caméra qui regarde selon `dir` (−z local) avec `up` pour haut, ou `None` si `dir` est parallèle à
/// `up` (comme `Matrix4.lookAt`, qui dégénère alors).
#[must_use]
pub fn look_rotation(dir: Vec3, up: Vec3) -> Option<Quat> {
    let back = (-dir).try_normalize()?;
    let right = up.cross(back).try_normalize()?;
    let up = back.cross(right);
    Some(Quat::from_mat3(&Mat3::from_cols(right, up, back)).normalize())
}

/// Interpole deux poses : position et ciel en ligne droite, orientation en slerp, FOV linéaire. `cabin` bascule à
/// mi-chemin.
pub fn mix_pose(out: &mut CameraPose, a: &CameraPose, b: &CameraPose, t: f32) {
    out.position = a.position.lerp(b.position, f64::from(t));
    out.rotation = a.rotation.slerp(b.rotation, t);
    out.fov_deg = a.fov_deg + (b.fov_deg - a.fov_deg) * t;
    out.cabin = if t < 0.5 { a.cabin } else { b.cabin };
    out.sky_drift = a.sky_drift.lerp(b.sky_drift, t);
}

/// Lissage exponentiel indépendant de la cadence : la part du chemin à parcourir cette image pour une `rate` (1/s).
#[must_use]
pub fn follow(dt: f32, rate: f32) -> f32 {
    1.0 - (-dt * rate).exp()
}

/// Courbe en S (0 → 1), entrée bornée.
#[must_use]
pub fn smoothstep(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Courbe en S plus douce aux extrémités (0 → 1), entrée bornée.
#[must_use]
pub fn smootherstep(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// La pose est-elle utilisable (aucun NaN ni infini) ?
#[must_use]
pub fn is_finite(pose: &CameraPose) -> bool {
    pose.position.is_finite() && pose.rotation.is_finite() && pose.fov_deg.is_finite() && pose.sky_drift.is_finite()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viser_regarde_la_cible() {
        let mut pose = CameraPose::default();
        let eye = DVec3::new(0.0, 2.0, -400_000.0);
        aim(&mut pose, eye, eye + DVec3::new(0.0, 0.0, -10.0), 0.0, Vec3::Y, false);
        let forward = pose.rotation * Vec3::NEG_Z;
        assert!((forward - Vec3::NEG_Z).length() < 1e-5, "{forward}");
        let up = pose.rotation * Vec3::Y;
        assert!((up - Vec3::Y).length() < 1e-5, "{up}");
    }

    #[test]
    fn un_regard_degenere_garde_l_orientation() {
        let mut pose = CameraPose {
            rotation: Quat::from_rotation_y(0.4),
            ..CameraPose::default()
        };
        aim(&mut pose, DVec3::ONE, DVec3::ONE, 0.0, Vec3::Y, false);
        assert!(pose.rotation.angle_between(Quat::from_rotation_y(0.4)) < 1e-6);
        aim(&mut pose, DVec3::ZERO, DVec3::Y, 0.0, Vec3::Y, false);
        assert!(pose.rotation.is_finite());
    }

    #[test]
    fn le_roulis_tourne_autour_du_regard() {
        let mut pose = CameraPose::default();
        aim(&mut pose, DVec3::ZERO, DVec3::NEG_Z, 0.3, Vec3::Y, false);
        let forward = pose.rotation * Vec3::NEG_Z;
        assert!((forward - Vec3::NEG_Z).length() < 1e-5);
        let up = pose.rotation * Vec3::Y;
        assert!((up.x.atan2(up.y).abs() - 0.3).abs() < 1e-4, "{up}");
    }

    #[test]
    fn le_melange_glisse_et_bascule_l_habitacle_a_mi_chemin() {
        let a = CameraPose::default();
        let b = CameraPose {
            position: DVec3::new(10.0, 0.0, 0.0),
            fov_deg: 100.0,
            cabin: true,
            ..CameraPose::default()
        };
        let mut out = CameraPose::default();
        mix_pose(&mut out, &a, &b, 0.25);
        assert!((out.position.x - 2.5).abs() < 1e-9);
        assert!(!out.cabin);
        mix_pose(&mut out, &a, &b, 0.5);
        assert!(out.cabin);
    }

    #[test]
    fn les_courbes_sont_bornees() {
        assert!(smoothstep(-1.0).abs() < 1e-9 && (smoothstep(2.0) - 1.0).abs() < 1e-9);
        assert!((smootherstep(0.5) - 0.5).abs() < 1e-6);
        assert!((follow(1.0, 0.0)).abs() < 1e-9);
    }
}
