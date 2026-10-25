//! De la logique au rendu : recopie la pose de chaque projecteur dans son entité et écrit le niveau d'éclat. Rien ici ne
//! décide : [`Beams`] a déjà tout calculé, et le limiteur de flashs a déjà dit quel pouls passe (règle 7).
//!
//! `CfSet::Present`. Les matériaux ne sont réécrits que si leurs uniformes changent ; la teinte (`MeshTag`) et la pose ne
//! le sont que si elles changent.

use bevy::math::{DVec3, Quat, Vec3};
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use cf_render::AbsolutePosition;

use super::Beams;
use super::material::{BeamKind, BeamMaterial, BeamParams};
use super::rig::{BeamPiece, BeamRig};

/// Pose d'un faisceau : rotation qui mène l'axe +y vers la cible, échelle `(1, longueur, 1)`. `None` si l'origine et la
/// cible se confondent.
#[must_use]
pub fn cone_pose(origin: DVec3, target: DVec3) -> Option<(Quat, Vec3)> {
    let delta = target - origin;
    let length = delta.length() as f32;
    if length < 1e-3 || !length.is_finite() {
        return None;
    }
    let dir = (delta / f64::from(length)).as_vec3();
    Some((Quat::from_rotation_arc(Vec3::Y, dir), Vec3::new(1.0, length, 1.0)))
}

/// Pose d'une tache de demi-axe `radius` en x (`radius × 1,4` en z) : couchée sur la route, face vers le haut.
#[must_use]
pub fn pool_pose(radius: f32) -> (Quat, Vec3) {
    (
        Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
        // Le rectangle unité a pour côté 1 : le diamètre est le double du rayon. Son axe y devient l'axe z de la route.
        Vec3::new(radius * 2.0, radius * 2.0 * 1.4, 1.0),
    )
}

/// Empaquette une teinte (RVB linéaire, 0..1) dans un `MeshTag` : 10 bits par canal (`beam.wesl` le dépaquette).
#[must_use]
pub fn pack_tint(tint: [f32; 3]) -> u32 {
    let q = |c: f32| (c.clamp(0.0, 1.0) * 1023.0).round() as u32;
    q(tint[0]) | (q(tint[1]) << 10) | (q(tint[2]) << 20)
}

/// Écrit un matériau seulement s'il a changé.
fn write(assets: &mut Assets<BeamMaterial>, handle: &Handle<BeamMaterial>, value: BeamMaterial) {
    if assets.get(handle).is_some_and(|m| *m == value) {
        return;
    }
    if let Some(mut material) = assets.get_mut(handle) {
        *material = value;
    }
}

/// `CfSet::Present` : pose, teinte et niveau de chaque projecteur.
pub(super) fn sync_beams(
    beams: Res<Beams>,
    rig: Res<BeamRig>,
    mut materials: ResMut<Assets<BeamMaterial>>,
    mut pieces: Query<(
        &BeamPiece,
        &mut AbsolutePosition,
        &mut Transform,
        &mut Visibility,
        &mut MeshTag,
    )>,
) {
    let field = beams.field();
    write(
        &mut materials,
        &rig.cone,
        BeamMaterial {
            params: BeamParams::new(BeamKind::Cone, field.cone_level()),
        },
    );
    write(
        &mut materials,
        &rig.pool,
        BeamMaterial {
            params: BeamParams::new(BeamKind::Pool, field.pool_level()),
        },
    );
    let pool = pool_pose(field.radius());
    for (piece, mut absolute, mut transform, mut visibility, mut tag) in &mut pieces {
        let (BeamPiece::Cone(i) | BeamPiece::Pool(i)) = *piece;
        let slot = field.slots().get(i);
        let shown = match (*piece, slot) {
            (BeamPiece::Cone(_), Some(slot)) => cone_pose(slot.origin, slot.target).is_some_and(|(rotation, scale)| {
                absolute.set_if_neq(AbsolutePosition(slot.origin));
                transform.rotation = rotation;
                transform.scale = scale;
                tag.set_if_neq(MeshTag::new(pack_tint(slot.tint)));
                true
            }),
            (BeamPiece::Pool(_), Some(slot)) => {
                absolute.set_if_neq(AbsolutePosition(slot.target));
                transform.rotation = pool.0;
                transform.scale = pool.1;
                tag.set_if_neq(MeshTag::new(pack_tint(slot.tint)));
                true
            }
            (_, None) => false,
        };
        visibility.set_if_neq(if shown { Visibility::Visible } else { Visibility::Hidden });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_cone_part_de_l_origine_et_arrive_sur_la_cible() {
        let origin = DVec3::new(13.0, 30.0, -4000.0);
        let target = DVec3::new(-2.0, 0.06, -3990.0);
        let (rotation, scale) = cone_pose(origin, target).expect("pose");
        let tip = origin + (rotation * Vec3::new(0.0, scale.y, 0.0)).as_dvec3();
        assert!((tip - target).length() < 1e-2, "{tip:?}");
        assert!((scale.y - (target - origin).length() as f32).abs() < 1e-3);
        assert_eq!((scale.x, scale.z), (1.0, 1.0));
    }

    #[test]
    fn un_faisceau_sans_longueur_n_a_pas_de_pose() {
        let p = DVec3::new(1.0, 2.0, 3.0);
        assert!(cone_pose(p, p).is_none());
        assert!(cone_pose(p, DVec3::NAN).is_none());
    }

    #[test]
    fn la_tache_est_couchee_face_en_haut_et_plus_longue_le_long_de_la_route() {
        let (rotation, scale) = pool_pose(2.0);
        assert!((rotation * Vec3::Z).dot(Vec3::Y) > 0.9999, "normale vers le haut");
        // L'axe y du rectangle devient l'axe z de la route.
        assert!((rotation * Vec3::Y).dot(Vec3::NEG_Z) > 0.9999);
        assert!((scale.y / scale.x - 1.4).abs() < 1e-6);
    }

    #[test]
    fn la_teinte_se_range_et_se_relit_a_un_millieme_pres() {
        let tag = pack_tint([0.25, 1.0, 0.0]);
        let back = [tag & 0x3FF, (tag >> 10) & 0x3FF, (tag >> 20) & 0x3FF].map(|c| c as f32 / 1023.0);
        assert!((back[0] - 0.25).abs() < 1e-3 && (back[1] - 1.0).abs() < 1e-6 && back[2].abs() < 1e-6);
        assert_eq!(pack_tint([2.0, -1.0, 0.0]), pack_tint([1.0, 0.0, 0.0]), "borné à 0..1");
    }
}
