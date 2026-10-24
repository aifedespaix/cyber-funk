//! Les entités de préchauffe : copies de rendu minuscules, posées devant la caméra qui les voit.
//!
//! Un pipeline de matériau est spécialisé quand une entité **visible** passe devant une caméra, pour la combinaison
//! (disposition des sommets, mode alpha, format et MSAA de la caméra…). Pour compiler exactement les pipelines du show,
//! on fait donc voir à chaque caméra une copie de chaque combinaison : même `Mesh3d`, même matériau, mêmes
//! `RenderLayers`, mais réduite et posée au milieu de son volume de vision. Sans masque de visibilité à lever ni
//! système du monde à tromper : les entités du monde (cachées, en pool) ne sont pas touchées.

use std::collections::HashSet;

use bevy::camera::primitives::MeshAabb;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Camera, Projection};
use bevy::pbr::{Material, MeshMaterial3d};
use bevy::prelude::*;

use super::curtain::Curtain;

/// Fraction du volume de vision (en distance) que le rayon d'une copie occupe : assez petite pour ne rien couvrir.
const ANGULAR_SIZE: f32 = 0.05;

/// Une copie de préchauffe. Retirée au passage à `Ready`.
#[derive(Component, Debug, Clone, Copy)]
pub struct WarmEntity {
    /// Rayon de l'enveloppe du maillage (m), pour fixer l'échelle ; 0 si inconnu.
    pub radius: f32,
}

/// Copie une entité de rendu de chaque combinaison (maillage, mode alpha, couches) du matériau `M`.
pub(super) fn mirror_material<M: Material>(world: &mut World) -> Vec<Entity> {
    let sources: Vec<(Handle<Mesh>, Handle<M>, RenderLayers)> = world
        .query_filtered::<(&Mesh3d, &MeshMaterial3d<M>, Option<&RenderLayers>), (Without<WarmEntity>, Without<Curtain>)>()
        .iter(world)
        .map(|(mesh, material, layers)| (mesh.0.clone(), material.0.clone(), layers.cloned().unwrap_or_default()))
        .collect();
    let mut seen = HashSet::new();
    let mut picked = Vec::new();
    {
        let meshes = world.resource::<Assets<Mesh>>();
        let Some(materials) = world.get_resource::<Assets<M>>() else {
            return Vec::new();
        };
        for (mesh_handle, material_handle, layers) in sources {
            let (Some(mesh), Some(material)) = (meshes.get(&mesh_handle), materials.get(&material_handle)) else {
                continue;
            };
            let names: Vec<&str> = mesh.attributes().map(|(attribute, _)| attribute.name).collect();
            let key = format!(
                "{:?}|{names:?}|{:?}|{layers:?}",
                mesh.primitive_topology(),
                material.alpha_mode()
            );
            if !seen.insert(key) {
                continue;
            }
            let radius = mesh.get_aabb().map_or(0.0, |aabb| aabb.half_extents.length());
            picked.push((mesh_handle, material_handle, layers, radius));
        }
    }
    picked
        .into_iter()
        .map(|(mesh, material, layers, radius)| {
            world
                .spawn((
                    Name::new("préchauffe"),
                    WarmEntity { radius },
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    layers,
                    Transform::default(),
                    Visibility::Visible,
                ))
                .id()
        })
        .collect()
}

/// Distance à laquelle poser une copie devant une caméra : au milieu (géométrique) de son volume de vision.
fn view_distance(projection: &Projection) -> f32 {
    let (near, far) = match projection {
        Projection::Perspective(p) => (p.near, p.far),
        Projection::Orthographic(o) => (o.near, o.far),
        Projection::Custom(_) => (0.1, 100.0),
    };
    if near > 0.0 {
        (near * far).sqrt()
    } else {
        // Caméra orthographique à plans symétriques (la 2D) : le milieu.
        f32::midpoint(near, far)
    }
}

/// Pose chaque copie au milieu du volume de vision de la première caméra qui partage une de ses couches.
///
/// Rejouée à chaque image de chargement : les caméras bougent (réalisateur, point suivi). Une copie sans caméra
/// correspondante reste où elle est (elle n'est alors vue de personne, aucun pipeline à compiler pour elle).
pub(super) fn place_warm_entities(
    cameras: Query<(&Camera, &GlobalTransform, &Projection, Option<&RenderLayers>), Without<Curtain>>,
    mut warm: Query<(&WarmEntity, &RenderLayers, &mut Transform)>,
) {
    for (entity, layers, mut transform) in &mut warm {
        let Some((_, camera_transform, projection, _)) = cameras.iter().find(|(camera, _, _, camera_layers)| {
            camera.is_active && camera_layers.cloned().unwrap_or_default().intersects(layers)
        }) else {
            continue;
        };
        let distance = view_distance(projection);
        let scale = if entity.radius > 1e-4 {
            ANGULAR_SIZE * distance / entity.radius
        } else {
            ANGULAR_SIZE * distance
        };
        transform.translation = camera_transform.translation() + camera_transform.forward() * distance;
        transform.scale = Vec3::splat(scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_distance_est_dans_le_volume_de_vision() {
        let perspective = Projection::Perspective(PerspectiveProjection {
            near: 0.1,
            far: 2_600.0,
            ..default()
        });
        let d = view_distance(&perspective);
        assert!(d > 0.1 && d < 2_600.0, "d = {d}");
        let overlay = Projection::Orthographic(OrthographicProjection {
            near: -1_000.0,
            far: 1_000.0,
            ..OrthographicProjection::default_3d()
        });
        assert!(view_distance(&overlay).abs() < f32::EPSILON);
    }
}
