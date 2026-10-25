//! De la logique au rendu : recopie la pose de chaque onde dans son entité, réécrit les matériaux et la gerbe. Rien ici ne
//! décide : [`DropFx`] a déjà tout calculé, et le limiteur de flashs a déjà dit ce qui brille (règle 7).
//!
//! `CfSet::Present`, dans l'image même du `drop_now` : l'onde et la gerbe lancées en `CfSet::Simulate` sont visibles sur cette
//! image. Les matériaux ne sont réécrits que si leurs uniformes changent.

use bevy::camera::primitives::Aabb;
use bevy::math::{Quat, Vec3};
use bevy::mesh::VertexAttributeValues;
use bevy::prelude::*;
use cf_render::AbsolutePosition;

use super::DropFx;
use super::material::{DropRingMaterial, RING_HALF_M, RingParams};
use super::rig::{DropPiece, DropRig};
use super::ring::{CYCLE_LEN, PARTICLES};

/// Marge de l'`Aabb` de la gerbe (m) : un demi-côté d'étincelle, arrondi largement.
const SPARK_MARGIN_M: f32 = 0.5;

type PieceItem<'a> = (
    &'a DropPiece,
    &'a mut AbsolutePosition,
    &'a mut Transform,
    &'a mut Visibility,
    Option<&'a mut Aabb>,
);

/// Écrit un matériau seulement s'il a changé.
fn write(assets: &mut Assets<DropRingMaterial>, handle: &Handle<DropRingMaterial>, value: DropRingMaterial) {
    if assets.get(handle).is_some_and(|m| *m == value) {
        return;
    }
    if let Some(mut material) = assets.get_mut(handle) {
        *material = value;
    }
}

/// `CfSet::Present` : pose, matériau de chaque onde, et gerbe.
pub(super) fn sync_dropfx(
    fx: Res<DropFx>,
    rig: Res<DropRig>,
    mut rings: ResMut<Assets<DropRingMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut pieces: Query<PieceItem<'_>>,
) {
    for (piece, mut absolute, mut transform, mut visibility, aabb) in &mut pieces {
        let shown = match *piece {
            DropPiece::Ring(i) => {
                let ring = &fx.rings()[i];
                let level = fx.ring_level(ring);
                // Éteinte, ou si sombre qu'elle ne se voit pas (flashs coupés) : on ne la dessine pas.
                if ring.is_free() || level <= 0.0 {
                    false
                } else {
                    absolute.set_if_neq(AbsolutePosition(ring.at));
                    transform.rotation = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
                    transform.scale = Vec3::new(RING_HALF_M * 2.0, RING_HALF_M * 2.0, 1.0);
                    let tint = fx.cycle()[ring.tint % CYCLE_LEN];
                    let params = RingParams::new(tint, level, ring.scale());
                    write(&mut rings, &rig.rings[i], DropRingMaterial { params });
                    true
                }
            }
            DropPiece::Sparks => match fx.bounds(SPARK_MARGIN_M) {
                Some((min, max)) if fx.master() > 0.0 => {
                    absolute.set_if_neq(AbsolutePosition(fx.origin()));
                    transform.rotation = Quat::IDENTITY;
                    transform.scale = Vec3::ONE;
                    if let Some(mut aabb) = aabb {
                        *aabb = Aabb::from_min_max(min, max);
                    }
                    if let Some(mut mesh) = meshes.get_mut(&rig.sparks_mesh) {
                        write_sparks(&mut mesh, &fx);
                    }
                    true
                }
                _ => false,
            },
        };
        visibility.set_if_neq(if shown { Visibility::Visible } else { Visibility::Hidden });
    }
}

/// Réécrit positions et couleurs des carrés de la gerbe, en place (aucune allocation : les attributs existent déjà).
fn write_sparks(mesh: &mut Mesh, fx: &DropFx) {
    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        for (i, quad) in positions.as_chunks_mut::<4>().0.iter_mut().enumerate().take(PARTICLES) {
            quad.fill(fx.particle_offset(i).to_array());
        }
    }
    if let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        for (i, quad) in colors.as_chunks_mut::<4>().0.iter_mut().enumerate().take(PARTICLES) {
            let [r, g, b] = fx.particle_color(i);
            quad.fill([r, g, b, 1.0]);
        }
    }
}
