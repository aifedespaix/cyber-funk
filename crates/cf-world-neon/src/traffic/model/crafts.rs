//! Les engins de Kenney (`crafts.ts`) : un GLB du Space Kit (CC0) devient un seul maillage, avec ses néons et ses phares.
//!
//! Les modèles regardent vers −Z, comme la voiture. Les matériaux d'origine servent de code : « metalRed » (l'orange)
//! devient la pièce d'accent teintée par instance. On les recentre (sol à y = 0, centre en x et z) et on les ramène à
//! une longueur de référence ([`CRAFT_LENGTH`]) : la flotte les met ensuite à l'échelle.
//!
//! Le GLB est lu par le lecteur de la voiture ([`crate::car::Gltf`]) : pas de dépendance de plus, et les GLB de Kenney
//! (sans compression) en sont un cas simple.

use bevy::math::{Mat4, Quat, Vec3};

use super::builder::MeshBuilder;
use super::{CraftModel, ModelError};
use crate::car::Gltf;
use crate::traffic::variant::{CRAFT_LENGTH, kind};

/// La surface d'un matériau de Kenney (`BY_MATERIAL`) ; la coque claire à défaut.
fn kind_of(material: &str) -> f32 {
    match material {
        "metalDark" => kind::HULL_DARK,
        "dark" => kind::GLASS,
        "metalRed" => kind::ACCENT,
        _ => kind::HULL,
    }
}

/// Décode un GLB de Kenney en un modèle. `police` ajoute la rampe de gyrophares sur le toit.
///
/// # Errors
///
/// Si le fichier n'est pas un GLB lisible, ou s'il ne porte aucun triangle.
pub fn build_craft(bytes: &[u8], police: bool) -> Result<CraftModel, ModelError> {
    let gltf = Gltf::from_glb(bytes)?;
    let mut builder = MeshBuilder::default();
    for &root in gltf.root_nodes() {
        add_node(&gltf, root, Mat4::IDENTITY, &mut builder, 0)?;
    }
    if builder.is_empty() {
        return Err(ModelError::Empty);
    }
    Ok(finish(builder, police))
}

/// Un engin de secours (une coque en boîte) quand le fichier manque : le trafic existe quand même.
#[must_use]
pub fn fallback_craft(police: bool) -> CraftModel {
    let mut builder = MeshBuilder::default();
    builder.add_box(Vec3::new(1.0, 0.35, CRAFT_LENGTH), Vec3::new(0.0, 0.18, 0.0), kind::HULL);
    builder.add_box(Vec3::new(0.6, 0.25, 0.8), Vec3::new(0.0, 0.45, 0.2), kind::GLASS);
    finish(builder, police)
}

fn add_node(gltf: &Gltf, index: usize, parent: Mat4, out: &mut MeshBuilder, depth: usize) -> Result<(), ModelError> {
    // Un glTF mal formé pourrait boucler : les engins de Kenney ont un ou deux niveaux.
    let (Some(node), true) = (gltf.nodes().get(index), depth < 16) else {
        return Ok(());
    };
    let local = Mat4::from_scale_rotation_translation(
        node.scale.map_or(Vec3::ONE, Vec3::from),
        node.rotation.map_or(Quat::IDENTITY, |[x, y, z, w]| Quat::from_xyzw(x, y, z, w)),
        node.translation.map_or(Vec3::ZERO, Vec3::from),
    );
    let world = parent * local;
    if let Some(mesh) = node.mesh.and_then(|m| gltf.meshes().get(m)) {
        for primitive in &mesh.primitives {
            let attribute = |name: &str| primitive.attributes.get(name).copied();
            let Some(position) = attribute("POSITION") else { continue };
            let positions = gltf.floats(position)?;
            let normals = attribute("NORMAL").map(|a| gltf.floats(a)).transpose()?.unwrap_or_default();
            let indices = primitive.indices.map(|i| gltf.indices(i)).transpose()?;
            let name = primitive
                .material
                .and_then(|m| gltf.materials().get(m))
                .map_or("", |m| m.name.as_str());
            out.add_triangles(
                positions.as_chunks::<3>().0,
                normals.as_chunks::<3>().0,
                indices.as_deref(),
                world,
                kind_of(name),
            );
        }
    }
    for &child in &node.children {
        add_node(gltf, child, world, out, depth + 1)?;
    }
    Ok(())
}

/// Recentre, met à longueur, puis ajoute les bandes néon, les phares et, pour la police, les gyrophares.
fn finish(mut builder: MeshBuilder, police: bool) -> CraftModel {
    let (raw_min, raw_max) = builder.bounds();
    let raw_size = raw_max - raw_min;
    let raw_center = (raw_min + raw_max) / 2.0;
    let unit = CRAFT_LENGTH / raw_size.z.max(0.001);
    builder.transform_all(
        Mat4::from_scale(Vec3::splat(unit)) * Mat4::from_translation(Vec3::new(-raw_center.x, -raw_min.y, -raw_center.z)),
    );
    let (min, max) = builder.bounds();
    let size = max - min;
    let half_width = size.x / 2.0;

    // Bandes néon sous la coque, barre arrière et phares avant.
    for side in [-1.0, 1.0] {
        builder.add_box(
            Vec3::new(0.05, 0.05, size.z * 0.55),
            Vec3::new(side * half_width * 0.82, min.y + 0.1, size.z * 0.02),
            kind::NEON,
        );
    }
    builder.add_box(
        Vec3::new(half_width * 1.1, 0.05, 0.05),
        Vec3::new(0.0, min.y + 0.32, max.z + 0.01),
        kind::NEON,
    );
    for side in [-1.0, 1.0] {
        builder.add_box(
            Vec3::new(0.16, 0.07, 0.05),
            Vec3::new(side * half_width * 0.34, min.y + 0.3, min.z - 0.01),
            kind::HEAD,
        );
    }

    if police {
        // Rampe de gyrophares sur le toit.
        let roof = max.y;
        builder.add_box(Vec3::new(0.58, 0.05, 0.2), Vec3::new(0.0, roof + 0.02, 0.0), kind::HULL_DARK);
        builder.add_box(Vec3::new(0.26, 0.11, 0.17), Vec3::new(-0.15, roof + 0.09, 0.0), kind::BEACON_LEFT);
        builder.add_box(Vec3::new(0.26, 0.11, 0.17), Vec3::new(0.15, roof + 0.09, 0.0), kind::BEACON_RIGHT);
    }
    CraftModel {
        mesh: builder.finish(),
        half_width,
        rear: max.z,
    }
}
