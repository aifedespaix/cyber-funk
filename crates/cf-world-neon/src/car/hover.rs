//! Le mode volant de la voiture (`setupHover` / `stepHover` / `applyHover` de `CyberCar.ts`, §11 de `docs/car/INTEGRATION.md`).
//!
//! Au décollage, les quatre roues **basculent à plat** (face extérieure vers le sol) avec un petit dépassement, descendent
//! de 22 cm et rentrent de 10 cm ; une lueur de répulseur s'allume sous chacune ; elles tournent alors comme des turbines.
//! À l'atterrissage, la chronologie se déroule à l'envers.
//!
//! La hauteur de la caisse n'est pas ici : le pilote la porte déjà par la position absolue de la voiture (`altitude`), et
//! son rebond sur les répulseurs par [`CarControls::bounce_m`](super::CarControls). Il ne reste à ce module que ce que
//! Sillage appelait `tuck` : l'état des roues, déduit d'un seul nombre, la progression du déploiement `p` (0 au sol, 1 en
//! vol), que le pilote publie (`hover_amount`).
//!
//! Les roues sont dans des groupes `tuck_*` (voir `spawn.rs`) : ce module ne touche ni au pivot de roulement ni à celui de
//! braquage, qu'il n'écrase que le temps du vol (la direction est ignorée en l'air, comme dans Sillage).
//!
//! # Règle 7 de `CLAUDE.md`
//!
//! La lueur monte en `smoothstep(0,25 ; 0,7)` de la progression, sur plus d'une seconde : aucune variation brusque de
//! luminance, donc rien à faire passer par le limiteur. Elle ne « respire » pas avec les basses, justement.

use bevy::math::{Quat, Vec3};
use bevy::prelude::*;

use cf_render::WorldLayer;

use super::materials::CarLightMaterial;
use super::rig::{Car, CarRig, CarState, Tuck};
use super::systems::CarControls;

/// De combien une roue basculée descend sous la caisse, m (`WHEEL_TUCK_DROP`) : elle ne touche plus la carrosserie.
pub const TUCK_DROP_M: f32 = 0.22;
/// De combien elle rentre vers l'axe, m (`WHEEL_TUCK_IN`).
pub const TUCK_IN_M: f32 = 0.10;
/// Vitesse de rotation des roues en turbines, rad/s.
pub const TURBINE_RAD_S: f32 = 26.0;
/// Rayon du pad lumineux, m (`CircleGeometry(0.36)`) ; le quadrilatère du maillage fait 1 m de côté.
const PAD_SCALE: f32 = 0.9;
/// Décalage du pad sur la face extérieure de la roue, m.
const PAD_OFFSET_M: f32 = 0.17;
/// Éclat de la lueur d'une roue, en lumière linéaire (au-delà de 1, le bloom s'en empare).
const PAD_BRIGHTNESS: f32 = 2.6;

/// L'état des roues à une progression donnée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoverPose {
    /// Bascule des roues : 0 au sol, 1 à plat, dépasse 1 un instant (rebond de la bascule).
    pub tuck: f32,
    /// Force de la lueur des répulseurs, 0 à 1.
    pub glow: f32,
}

fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let k = ((x - a) / (b - a)).clamp(0.0, 1.0);
    k * k * (3.0 - 2.0 * k)
}

/// La pose des roues pour la progression `progress` du déploiement (0 à 1).
///
/// Chronologie de Sillage : la bascule occupe les premiers 40 % (`easeOutBack`, ~6° de dépassement), la lueur monte de 25 %
/// à 70 %.
#[must_use]
pub fn hover_pose(progress: f32) -> HoverPose {
    let p = progress.clamp(0.0, 1.0);
    let k = (p / 0.4).clamp(0.0, 1.0) - 1.0;
    HoverPose {
        tuck: 1.0 + 2.701_58 * k * k * k + 1.701_58 * k * k,
        glow: smooth(0.25, 0.7, p),
    }
}

/// La transformation du groupe d'une roue, dans le repère du modèle.
///
/// `chassis` est la pose de la caisse (suspension, rebond) : plus la roue est basculée, plus elle la suit, pour ne pas
/// laisser une roue à plat au sol sous une caisse qui tangue.
#[must_use]
pub fn wheel_pose(tuck: &Tuck, pose: HoverPose, chassis: Transform) -> Transform {
    let t = pose.tuck.min(1.0);
    let local = tuck.base + Vec3::new(-tuck.sign * TUCK_IN_M * t, -TUCK_DROP_M * t, 0.0);
    let carried = chassis.transform_point(local);
    // Face extérieure vers le bas : autour de l'axe longitudinal du modèle, d'un quart de tour, avec le dépassement.
    let flip = Quat::from_rotation_z(-tuck.sign * std::f32::consts::FRAC_PI_2 * pose.tuck);
    Transform {
        translation: local.lerp(carried, t),
        rotation: Quat::IDENTITY.slerp(chassis.rotation, t) * flip,
        scale: Vec3::ONE,
    }
}

/// Le pad lumineux d'une roue : posé sur sa face extérieure, qui regarde le sol une fois la roue à plat.
#[must_use]
pub fn pad_transform(sign: f32) -> Transform {
    Transform {
        translation: Vec3::new(sign * PAD_OFFSET_M, 0.0, 0.0),
        // Le quadrilatère est couché (normale +Y) : on le redresse vers l'extérieur de l'essieu.
        rotation: Quat::from_rotation_z(-sign * std::f32::consts::FRAC_PI_2),
        scale: Vec3::new(PAD_SCALE, 1.0, PAD_SCALE),
    }
}

/// Le maillage et le matériau des pads lumineux, partagés par les quatre roues.
#[derive(Debug)]
pub(super) struct PadAssets {
    /// Quadrilatère couché (un mètre de côté).
    pub mesh: Handle<Mesh>,
    /// Lueur additive.
    pub material: Handle<CarLightMaterial>,
}

/// Pose le groupe `tuck_*` d'une roue (en son centre `base`, sous `parent`) et le pad lumineux de sa face extérieure.
pub(super) fn spawn_tuck(commands: &mut Commands, parent: Entity, wheel: &str, base: Vec3, pads: &PadAssets) -> Tuck {
    let sign = if base.x > 0.0 { 1.0 } else { -1.0 };
    let entity = commands
        .spawn((
            Name::new(format!("tuck_{wheel}")),
            Transform::from_translation(base),
            Visibility::default(),
            WorldLayer,
            ChildOf(parent),
        ))
        .id();
    let pad = commands
        .spawn((
            Name::new("pad de répulseur"),
            Mesh3d(pads.mesh.clone()),
            MeshMaterial3d(pads.material.clone()),
            pad_transform(sign),
            Visibility::Hidden,
            WorldLayer,
            ChildOf(entity),
        ))
        .id();
    Tuck {
        entity,
        base,
        pad,
        sign,
    }
}

/// `CfSet::Simulate`, après `drive_car` : bascule les roues, les fait tourner en turbines et allume leurs pads.
///
/// `drive_car` réécrit chaque image le roulement et le braquage : ici on ne les remplace que tant que les roues sont
/// basculées. Au sol (progression nulle) et si rien n'était actif à l'image d'avant, le système ne touche à rien.
pub(super) fn animate_hover(
    time: Res<Time>,
    controls: Res<CarControls>,
    mut cars: Query<(&CarRig, &mut CarState), With<Car>>,
    mut transforms: Query<&mut Transform, Without<Car>>,
    mut visibility: Query<&mut Visibility>,
    mut lights: ResMut<Assets<CarLightMaterial>>,
) {
    let dt = time.delta_secs().min(0.05);
    for (rig, mut state) in &mut cars {
        let active = controls.hover > 0.0;
        if !active && !state.hover_was_active {
            continue;
        }
        let pose = hover_pose(controls.hover);
        state.hover_was_active = active;
        state.turbine_rad =
            (state.turbine_rad + TURBINE_RAD_S * pose.tuck.min(1.0) * dt).rem_euclid(std::f32::consts::TAU);

        let chassis = transforms.get(rig.chassis).copied().unwrap_or_default();
        for tuck in &rig.tucks {
            if let Ok(mut group) = transforms.get_mut(tuck.entity) {
                *group = if active {
                    wheel_pose(tuck, pose, chassis)
                } else {
                    Transform::from_translation(tuck.base)
                };
            }
            if let Ok(mut pad) = visibility.get_mut(tuck.pad) {
                pad.set_if_neq(if pose.glow > 0.003 && active {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
        }
        // Turbines : les roues tournent autour de leur axe, qui est vertical une fois à plat ; la direction est ignorée.
        let t = pose.tuck.min(1.0);
        for &entity in &rig.spin {
            if let Ok(mut wheel) = transforms.get_mut(entity) {
                wheel.rotation =
                    Quat::IDENTITY.slerp(wheel.rotation, 1.0 - t) * Quat::from_rotation_x(state.turbine_rad * t);
            }
        }
        for &entity in &rig.steer {
            if let Ok(mut wheel) = transforms.get_mut(entity) {
                wheel.rotation = Quat::IDENTITY.slerp(wheel.rotation, 1.0 - t);
            }
        }
        // Teinte : celle du néon du moment, comme les liserés de la caisse.
        let glow = state.look.map_or(LinearRgba::rgb(0.0, 0.9, 1.0), |l| l.glow);
        if let Some(mut material) = lights.get_mut(&rig.pad_material) {
            let k = PAD_BRIGHTNESS * pose.glow;
            material.set(LinearRgba::rgb(glow.red * k, glow.green * k, glow.blue * k), 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wheel(sign: f32) -> Tuck {
        Tuck {
            entity: Entity::PLACEHOLDER,
            pad: Entity::PLACEHOLDER,
            base: Vec3::new(sign * 0.85, 0.39, 1.4),
            sign,
        }
    }

    #[test]
    fn au_sol_les_roues_ne_bougent_pas() {
        let pose = hover_pose(0.0);
        assert!(pose.tuck.abs() < 1e-6 && pose.glow.abs() < 1e-6);
        let t = wheel_pose(&wheel(1.0), pose, Transform::IDENTITY);
        assert!(t.translation.distance(wheel(1.0).base) < 1e-6);
        assert!(t.rotation.angle_between(Quat::IDENTITY) < 1e-6);
    }

    #[test]
    fn la_bascule_depasse_un_court_instant_puis_se_pose_a_plat() {
        let peak = (0..=40)
            .map(|i| hover_pose(i as f32 / 100.0).tuck)
            .fold(0.0_f32, f32::max);
        assert!(peak > 1.02 && peak < 1.12, "dépassement d'environ 6° : {peak}");
        assert!((hover_pose(0.4).tuck - 1.0).abs() < 1e-5, "à plat à 40 %");
        assert!((hover_pose(1.0).tuck - 1.0).abs() < 1e-5);
    }

    #[test]
    fn la_lueur_attend_que_les_roues_soient_basculees() {
        assert!(hover_pose(0.2).glow.abs() < 1e-6);
        assert!(hover_pose(0.4).glow > 0.1 && hover_pose(0.4).glow < 0.9);
        assert!((hover_pose(0.8).glow - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_plat_la_roue_descend_rentre_et_sa_face_exterieure_regarde_le_sol() {
        for sign in [-1.0_f32, 1.0] {
            let w = wheel(sign);
            let t = wheel_pose(&w, hover_pose(1.0), Transform::IDENTITY);
            assert!((t.translation.y - (w.base.y - TUCK_DROP_M)).abs() < 1e-5);
            assert!(
                (t.translation.x.abs() - (w.base.x.abs() - TUCK_IN_M)).abs() < 1e-5,
                "rentre vers l'axe"
            );
            // L'axe de roue sortant (côté `sign`) pointe vers le bas.
            let axle = t.rotation * Vec3::new(sign, 0.0, 0.0);
            assert!(axle.y < -0.99, "face extérieure vers le sol : {axle}");
            // Le pad, lui, est sur cette face : sa normale (Y du quadrilatère) suit l'axe sortant.
            let normal = t.rotation * pad_transform(sign).rotation * Vec3::Y;
            assert!(normal.y < -0.99, "le pad regarde le sol : {normal}");
        }
    }

    #[test]
    fn la_roue_suit_la_caisse_quand_elle_penche() {
        let chassis = Transform::from_rotation(Quat::from_rotation_x(0.1)).with_translation(Vec3::new(0.0, 0.05, 0.0));
        let flat = wheel_pose(&wheel(1.0), hover_pose(1.0), Transform::IDENTITY);
        let tilted = wheel_pose(&wheel(1.0), hover_pose(1.0), chassis);
        assert!(flat.translation.distance(tilted.translation) > 0.05);
    }
}
