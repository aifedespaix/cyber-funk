//! Du directeur au rendu : recopie la pose des véhicules dans leurs entités, réécrit les maillages dynamiques, pousse
//! la palette et les niveaux du limiteur dans le matériau. Rien ici ne décide : le directeur a déjà tout calculé.

use bevy::color::Srgba;
use bevy::math::{Vec3, Vec4};
use bevy::mesh::MeshTag;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, ScenePalette};

use super::{TrafficBody, TrafficEntities, TrafficTrail, VehicleMaterial, VehicleParams};
use crate::traffic::director::TrafficDirector;
use crate::traffic::flash::TrafficFlash;

/// La teinte et le déphasage d'un véhicule, dans le `MeshTag` : trois octets sRGB, puis le déphasage sur 8 bits (voir
/// `traffic_vehicle.wesl`).
#[must_use]
pub fn pack_tag(tint: LinearRgba, flash: f32) -> u32 {
    let srgb = Srgba::from(tint);
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    byte(srgb.red) | (byte(srgb.green) << 8) | (byte(srgb.blue) << 16) | (byte(flash) << 24)
}

/// La teinte qu'un `MeshTag` porte (à l'arrondi de l'octet près).
#[cfg(test)]
#[must_use]
pub fn unpack_tint(tag: u32) -> LinearRgba {
    let channel = |shift: u32| ((tag >> shift) & 0xFF) as f32 / 255.0;
    LinearRgba::from(Srgba::new(channel(0), channel(8), channel(16), 1.0))
}

type BodyItem<'a> = (
    &'a mut AbsolutePosition,
    &'a mut Transform,
    &'a mut Visibility,
    &'a mut MeshTag,
);

/// `CfSet::Simulate`, après le pas du trafic : recopie la pose de chaque véhicule dans son corps et sa traînée.
///
/// Un véhicule qui naît (ou qui prend un autre emplacement) reçoit sa teinte et devient visible ; un véhicule rendu
/// à la flotte est caché. Les autres entités ne sont touchées que si leur valeur change.
pub(crate) fn sync_fleet(
    director: Res<TrafficDirector>,
    mut entities: ResMut<TrafficEntities>,
    mut bodies: Query<BodyItem<'_>, (With<TrafficBody>, Without<TrafficTrail>)>,
    mut trails: Query<BodyItem<'_>, (With<TrafficTrail>, Without<TrafficBody>)>,
) {
    for craft in director.fleet.crafts() {
        let id = craft.id;
        let index = usize::from(id);
        let Some(&shown) = entities.shown.get(index) else {
            continue;
        };
        let body = craft.active.then(|| entities.body(craft.variant, craft.slot)).flatten();
        let trail = entities.trail(id);

        // L'emplacement a changé (ou le véhicule est parti) : l'ancien corps se cache.
        if shown.body != body
            && let Some(old) = shown.body
            && let Ok((_, _, mut visibility, _)) = bodies.get_mut(old)
        {
            visibility.set_if_neq(Visibility::Hidden);
        }
        let Some(body) = body else {
            if shown.trail_visible
                && let Some(trail) = trail
                && let Ok((_, _, mut visibility, _)) = trails.get_mut(trail)
            {
                visibility.set_if_neq(Visibility::Hidden);
            }
            entities.shown[index] = super::Shown {
                body: None,
                trail_visible: false,
                ..shown
            };
            continue;
        };

        let reborn = shown.generation != craft.generation || shown.body != Some(body);
        let tag = MeshTag::new(pack_tag(craft.tint, craft.flash));
        if let Ok((mut position, mut transform, mut visibility, mut body_tag)) = bodies.get_mut(body) {
            position.set_if_neq(AbsolutePosition(craft.position));
            transform.rotation = craft.pose.rotation;
            transform.scale = Vec3::splat(craft.scale);
            if reborn {
                *body_tag = tag.clone();
            }
            visibility.set_if_neq(Visibility::Inherited);
        }

        let trail_visible = craft.pose.trail_scale != Vec3::ZERO;
        if let Some(trail) = trail
            && let Ok((mut position, mut transform, mut visibility, mut trail_tag)) = trails.get_mut(trail)
        {
            if trail_visible {
                position.set_if_neq(AbsolutePosition(craft.pose.trail_position));
                transform.rotation = craft.pose.rotation;
                transform.scale = craft.pose.trail_scale;
                if reborn {
                    *trail_tag = tag;
                }
            }
            visibility.set_if_neq(if trail_visible {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
        entities.shown[index] = super::Shown {
            generation: craft.generation,
            body: Some(body),
            trail_visible,
        };
    }
}

/// `CfSet::Present` : réécrit les maillages des points lumineux et des murs de lumière, les centre sur la voiture et les
/// cache quand il n'y a rien à dessiner.
pub(crate) fn sync_dynamic(
    director: Res<TrafficDirector>,
    mut entities: ResMut<TrafficEntities>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut places: Query<(&mut AbsolutePosition, &mut Visibility)>,
) {
    let origin = director.glints.origin();
    let glints = director.glints.points();
    let previous = entities.last_glints;
    if !glints.is_empty() || previous > 0 {
        let handle = entities.glint_mesh.clone();
        if let Some(mut mesh) = meshes.get_mut(&handle) {
            entities.last_glints = super::meshes::write_glints(&mut mesh, glints, previous);
        }
    }
    if let Ok((mut position, mut visibility)) = places.get_mut(entities.glints) {
        position.set_if_neq(AbsolutePosition(origin));
        visibility.set_if_neq(if glints.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        });
    }

    let walls_active = director.walls.active() > 0;
    if walls_active
        && let Some(mut mesh) = meshes.get_mut(&entities.wall_mesh)
        && let Some(mut vertices) = super::meshes::wall_vertices(&mut mesh)
    {
        director.walls.write(origin, time.elapsed_secs_f64(), &mut vertices);
    }
    if let Ok((mut position, mut visibility)) = places.get_mut(entities.walls) {
        position.set_if_neq(AbsolutePosition(origin));
        visibility.set_if_neq(if walls_active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// `CfSet::Present` : la palette de l'image et les niveaux du limiteur dans le matériau des véhicules. Le matériau n'est
/// touché (donc ré-envoyé au GPU) que si une valeur change.
pub(crate) fn update_materials(
    palette: Res<ScenePalette>,
    flash: Res<TrafficFlash>,
    entities: Res<TrafficEntities>,
    mut materials: ResMut<Assets<VehicleMaterial>>,
) {
    let anim = Vec4::new(flash.pulse(), flash.beacon_left(), flash.beacon_right(), 0.0);
    let params = VehicleParams::from_palette(&palette, anim);
    if materials
        .get(&entities.vehicle_material)
        .is_some_and(|m| m.params == params)
    {
        return;
    }
    if let Some(mut material) = materials.get_mut(&entities.vehicle_material) {
        material.params = params;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_tag_garde_la_teinte_a_l_octet_pres_et_le_dephasage() {
        use bevy::color::color_difference::EuclideanDistance;
        let tint = LinearRgba::rgb(0.2, 0.6, 0.9);
        let tag = pack_tag(tint, 0.5);
        let back = unpack_tint(tag);
        assert!(back.distance(&tint) < 0.02, "{back:?} vs {tint:?}");
        assert_eq!(tag >> 24, 128);
    }

    #[test]
    fn noir_et_blanc_font_l_aller_retour() {
        use bevy::color::color_difference::EuclideanDistance;
        assert_eq!(pack_tag(LinearRgba::BLACK, 0.0) & 0x00FF_FFFF, 0);
        assert_eq!(pack_tag(LinearRgba::WHITE, 1.0), 0xFFFF_FFFF);
        assert!(unpack_tint(0xFFFF_FFFF).distance(&LinearRgba::WHITE) < 1e-4);
    }
}
