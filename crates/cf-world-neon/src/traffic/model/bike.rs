//! La moto lumineuse (`bike.ts`), dessinée à la main (aucun fichier) : deux roues pleines à la jante néon, une coque
//! effilée et sombre bordée de lignes de lumière, un pilote couché, phare et feu arrière. Elle regarde vers −Z comme les
//! autres engins ; l'origine est l'axe de la roue arrière, ce qui fait pivoter un wheeling autour de la roue qui touche
//! le sol.

use std::f32::consts::FRAC_PI_2;

use bevy::math::{Mat4, Vec3};
use bevy::prelude::{Cylinder, MeshBuilder as _, Meshable, Sphere, Torus};

use super::CraftModel;
use super::builder::MeshBuilder;
use crate::traffic::variant::kind;

/// Rayon des roues, en unités du modèle : l'origine est l'axe de la roue arrière, le sol est à `-WHEEL`.
pub const WHEEL: f32 = 0.36;
/// Empattement : la roue avant est à cette distance, vers −Z.
const BASE: f32 = 1.9;

/// Le modèle de la moto.
#[must_use]
pub fn bike_model() -> CraftModel {
    let mut b = MeshBuilder::default();
    // L'axe des roues est X : les cylindres et le tore de Bevy ont Y pour axe, on les couche d'un quart de tour.
    let lie_down = Mat4::from_rotation_z(FRAC_PI_2);

    for z in [0.0, -BASE] {
        let at = Mat4::from_translation(Vec3::new(0.0, 0.0, z)) * lie_down;
        b.add_mesh(
            &Cylinder::new(WHEEL, 0.16).mesh().resolution(24).build(),
            at,
            kind::HULL_DARK,
        );
        let (major, tube) = (WHEEL * 0.96, 0.035);
        let rim = Torus::new(major - tube, major + tube)
            .mesh()
            .minor_resolution(6)
            .major_resolution(28);
        b.add_mesh(&rim.build(), at, kind::NEON);
        b.add_mesh(&Cylinder::new(0.1, 0.19).mesh().resolution(10).build(), at, kind::NEON);
    }

    // Coque effilée, pilote couché, casque.
    let ellipsoid =
        |sx: f32, sy: f32, sz: f32, at: Vec3| Mat4::from_translation(at) * Mat4::from_scale(Vec3::new(sx, sy, sz));
    let unit_sphere = |sectors: u32, stacks: u32| Sphere::new(1.0).mesh().uv(sectors, stacks);
    b.add_mesh(
        &unit_sphere(14, 10),
        ellipsoid(0.2, 0.24, 1.15, Vec3::new(0.0, 0.18, -BASE / 2.0)),
        kind::HULL_DARK,
    );
    b.add_mesh(
        &unit_sphere(10, 8),
        ellipsoid(0.17, 0.2, 0.45, Vec3::new(0.0, 0.52, -BASE * 0.42)),
        kind::HULL,
    );
    b.add_mesh(
        &unit_sphere(10, 8),
        ellipsoid(0.12, 0.12, 0.12, Vec3::new(0.0, 0.6, -BASE * 0.68)),
        kind::GLASS,
    );
    b.add_box(
        Vec3::new(0.22, 0.5, 0.08),
        Vec3::new(0.0, 0.0, -BASE + 0.14),
        kind::HULL_DARK,
    );

    // Lignes de lumière le long de la coque, et signature à l'arrière.
    for side in [-1.0, 1.0] {
        b.add_box(
            Vec3::new(0.03, 0.03, BASE * 0.85),
            Vec3::new(side * 0.2, 0.2, -BASE / 2.0),
            kind::NEON,
        );
    }
    b.add_box(Vec3::new(0.34, 0.05, 0.04), Vec3::new(0.0, 0.3, 0.22), kind::NEON);
    b.add_box(
        Vec3::new(0.15, 0.06, 0.04),
        Vec3::new(0.0, 0.2, -BASE - 0.1),
        kind::HEAD,
    );

    CraftModel {
        mesh: b.finish(),
        half_width: 0.3,
        rear: WHEEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::Mesh;

    #[test]
    fn la_moto_regarde_vers_moins_z_et_pose_ses_roues_sur_le_sol() {
        let model = bike_model();
        let positions = model
            .mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("positions");
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for p in positions {
            lo = lo.min(Vec3::from(*p));
            hi = hi.max(Vec3::from(*p));
        }
        // Le bas des roues est à −WHEEL ; la roue avant est à −BASE ; la largeur reste fine.
        // Le tore de la jante dépasse du cylindre de son rayon de tube (0,035).
        assert!((lo.y + WHEEL).abs() < 0.03, "{lo}");
        assert!(lo.z < -BASE, "{lo}");
        assert!(hi.z <= WHEEL + 0.25, "{hi}");
        assert!(hi.x - lo.x < 0.7, "{}", hi.x - lo.x);
    }
}
