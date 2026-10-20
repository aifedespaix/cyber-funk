//! Niveau 1 du culling : des `Aabb` justes, et un helper pour les meshes que le `Aabb` automatique ne sait pas décrire.
//!
//! Bevy calcule seul l'`Aabb` d'une entité `Mesh3d` à partir des positions du mesh (`calculate_bounds`). C'est juste
//! pour un mesh statique ou animé par le `Transform`. Trois cas, eux, ne le sont pas :
//!
//! - **le mesh suit la caméra** (brume, pluie, traînée, ciel, particules) : son entité est recentrée sur l'objectif à
//!   chaque image, mais ses sommets restent autour de l'origine locale. L'`Aabb` du mesh est alors correct *si* le
//!   mesh est bien construit autour de son origine, et faux dès qu'il est modelé loin d'elle (un dôme de 1 000 m dont
//!   l'origine est au sol) ou déformé au vertex shader. [`follow_camera_bounds`] pose un volume centré sur l'origine
//!   de l'entité, assez grand pour tout ce que le shader peut en faire ;
//! - **le vertex shader déplace les sommets** (houle, vent, étirement des gouttes) : l'`Aabb` du mesh ignore ce
//!   déplacement, les bords de l'écran voient l'objet disparaître alors qu'une partie est encore dedans.
//!   [`displaced_bounds`] élargit l'`Aabb` du mesh de la déformation maximale ;
//! - **le mesh est remplacé ou modifié** : `calculate_bounds` recalcule l'`Aabb` de toute entité dont le mesh change,
//!   et écraserait donc un volume posé à la main. Les deux helpers ajoutent [`NoAutoAabb`] (nouveau en Bevy 0.20),
//!   sans quoi le volume large serait perdu au premier rechargement d'asset.
//!
//! `NoFrustumCulling` n'est jamais la réponse : il force le dessin de l'objet à chaque image, y compris derrière le
//! spectateur. Un volume trop large ne coûte qu'un dessin de trop, jamais un clignotement. Le test
//! `tests/no_frustum_culling_guard.rs` refuse tout usage sans commentaire qui explique pourquoi aucun `Aabb` ne
//! convenait.

use bevy::camera::primitives::{Aabb, MeshAabb};
use bevy::camera::visibility::NoAutoAabb;
use bevy::math::{Vec3, Vec3A};
use bevy::mesh::{Mesh, VertexAttributeValues};

/// `Aabb` d'un mesh, en repère local. `None` si le mesh n'a pas de positions `Float32x3` (ou pas de sommet).
///
/// Lit les positions du mesh : appelé après que le mesh a quitté la mémoire du processeur (usage d'asset sans
/// `MAIN_WORLD`), il retombe sur l'étendue que Bevy a gardée à l'extraction (`Mesh::final_aabb`).
#[must_use]
pub fn mesh_aabb(mesh: &Mesh) -> Option<Aabb> {
    mesh.get_aabb()
}

/// Le même `Aabb`, élargi de `margin_m` sur chaque face (une marge négative est ramenée à zéro).
#[must_use]
pub fn padded(aabb: Aabb, margin_m: f32) -> Aabb {
    let margin = Vec3A::splat(margin_m.max(0.0));
    Aabb {
        center: aabb.center,
        half_extents: aabb.half_extents + margin,
    }
}

/// Volume d'un mesh que le vertex shader déplace de `max_displacement_m` au plus, sur n'importe quel axe : l'`Aabb`
/// du mesh élargi de ce déplacement, avec [`NoAutoAabb`] pour que Bevy ne le recalcule pas.
///
/// `None` si le mesh n'a pas de positions lisibles. À poser sur l'entité : `entity.insert(bounds)`.
///
/// Le déplacement maximal est celui que le shader ne dépasse jamais *par construction* (l'amplitude d'une houle, la
/// longueur d'une goutte), pas une mesure : une valeur trop juste ramène le clignotement au bord de l'écran.
#[must_use]
pub fn displaced_bounds(mesh: &Mesh, max_displacement_m: f32) -> Option<(Aabb, NoAutoAabb)> {
    Some((padded(mesh_aabb(mesh)?, max_displacement_m), NoAutoAabb))
}

/// Volume d'un mesh qui suit la caméra : un cube de demi-arête `reach_m` centré sur l'origine de l'entité.
///
/// `reach_m` est la distance maximale d'un sommet à l'origine de l'entité, sur chaque axe, quelle que soit la
/// déformation du shader (brume : son rayon ; pluie : la boîte d'air autour de l'objectif ; traînée : sa longueur).
/// L'entité suit la caméra (à distance fixe, éventuellement nulle) et son volume avec elle : l'objet est dessiné dès
/// qu'une part de ce cube est dans le champ, et seulement alors. Avec [`NoAutoAabb`], le volume survit aux
/// changements de mesh.
///
/// Une valeur non finie ou négative donne un volume nul (le mesh serait alors toujours coupé : mieux vaut le voir
/// disparaître tout de suite qu'à l'usage).
#[must_use]
pub fn follow_camera_bounds(reach_m: f32) -> (Aabb, NoAutoAabb) {
    let reach = if reach_m.is_finite() { reach_m.max(0.0) } else { 0.0 };
    (
        Aabb {
            center: Vec3A::ZERO,
            half_extents: Vec3A::splat(reach),
        },
        NoAutoAabb,
    )
}

/// Vrai si `aabb` contient toutes les positions de `mesh`, à `tolerance_m` près.
///
/// Un mesh sans positions `Float32x3` n'est contenu dans rien : la réponse est `false`. Sert aux tests et aux
/// vérifications de développement, pas à l'image.
#[must_use]
pub fn aabb_encloses_mesh(aabb: &Aabb, mesh: &Mesh, tolerance_m: f32) -> bool {
    let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {
        return false;
    };
    let (min, max) = (
        aabb.min() - Vec3A::splat(tolerance_m),
        aabb.max() + Vec3A::splat(tolerance_m),
    );
    positions.iter().all(|p| {
        let p = Vec3A::from(Vec3::from_array(*p));
        p.cmpge(min).all() && p.cmple(max).all()
    })
}

#[cfg(test)]
mod tests {
    use bevy::mesh::Meshable;
    use bevy::prelude::{Cuboid, Sphere};

    use super::*;

    #[test]
    fn l_aabb_d_un_cube_est_le_cube() {
        let aabb = mesh_aabb(&Mesh::from(Cuboid::new(2.0, 4.0, 6.0))).expect("positions");
        assert_eq!(aabb.center, Vec3A::ZERO);
        assert_eq!(aabb.half_extents, Vec3A::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn un_mesh_sans_positions_n_a_pas_d_aabb() {
        let mesh = Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        assert!(mesh_aabb(&mesh).is_none());
        assert!(displaced_bounds(&mesh, 1.0).is_none());
        assert!(!aabb_encloses_mesh(&Aabb::default(), &mesh, 0.0));
    }

    #[test]
    fn la_marge_elargit_chaque_face_et_ignore_le_negatif() {
        let aabb = Aabb::from_min_max(Vec3::splat(-1.0), Vec3::splat(1.0));
        assert_eq!(padded(aabb, 0.5).half_extents, Vec3A::splat(1.5));
        assert_eq!(padded(aabb, -3.0), aabb);
        assert_eq!(padded(aabb, 0.5).center, aabb.center);
    }

    #[test]
    fn un_mesh_deplace_garde_tout_son_deplacement_dans_son_volume() {
        let mesh = Sphere::new(10.0).mesh().uv(16, 8);
        let (bounds, _) = displaced_bounds(&mesh, 2.5).expect("positions");
        assert!(aabb_encloses_mesh(&bounds, &mesh, 0.0));
        assert_eq!(bounds.half_extents, Vec3A::splat(12.5));
        // Un point déplacé de 2,5 m dans n'importe quel axe reste dedans.
        let tip = Vec3A::new(10.0 + 2.5, 0.0, 0.0);
        assert!(tip.cmple(bounds.max()).all());
    }

    #[test]
    fn le_volume_du_suiveur_est_centre_sur_l_entite() {
        let (bounds, _) = follow_camera_bounds(300.0);
        assert_eq!(bounds.center, Vec3A::ZERO);
        assert_eq!(bounds.half_extents, Vec3A::splat(300.0));
        for bad in [f32::NAN, f32::INFINITY, -5.0] {
            assert_eq!(follow_camera_bounds(bad).0.half_extents, Vec3A::ZERO, "{bad}");
        }
    }

    #[test]
    fn un_volume_trop_petit_est_detecte() {
        let mesh = Mesh::from(Cuboid::new(2.0, 2.0, 2.0));
        let small = Aabb::from_min_max(Vec3::splat(-0.5), Vec3::splat(0.5));
        assert!(!aabb_encloses_mesh(&small, &mesh, 0.0));
        assert!(aabb_encloses_mesh(&small, &mesh, 0.6));
    }
}
