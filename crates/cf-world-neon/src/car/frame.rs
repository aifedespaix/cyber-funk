//! Repère de la voiture : de l'espace du GLB (nez vers +Z, centre au milieu des essieux) à l'espace de la voiture (nez
//! vers −Z, sens de la route, sol à y = 0, centrée en x et z), et position des optiques.
//!
//! Portage de `CyberCarModel.load` (`cyber-car.ts`) et de `lampCenters` (`cyber/anchors.ts`) : « les feux sont des zones
//! de la carrosserie, pas des objets », leur centre se lit donc sur les sommets qui portent leur matériau.

use bevy::math::{Mat4, Quat, Vec3};
use bevy::mesh::Mesh;
use bevy::transform::components::Transform;
use std::collections::HashMap;

use super::model::{CarModelAsset, MaterialRole, Side};

/// Une boîte alignée sur les axes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    /// Coin minimum.
    pub min: Vec3,
    /// Coin maximum.
    pub max: Vec3,
}

impl Bounds {
    const EMPTY: Self = Self {
        min: Vec3::splat(f32::INFINITY),
        max: Vec3::splat(f32::NEG_INFINITY),
    };

    fn include(&mut self, point: Vec3) {
        self.min = self.min.min(point);
        self.max = self.max.max(point);
    }

    fn is_empty(&self) -> bool {
        self.min.x > self.max.x
    }

    /// Centre de la boîte.
    #[must_use]
    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    /// Dimensions de la boîte.
    #[must_use]
    pub fn size(&self) -> Vec3 {
        self.max - self.min
    }
}

/// Le repère de la voiture, calculé une fois au chargement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CarFrame {
    /// Du GLB à la voiture : demi-tour autour de Y (le modèle regarde vers +Z, la route va vers −Z), puis la boîte est
    /// posée sur le sol et centrée (`holder` de `CyberCarModel`).
    pub to_car: Transform,
    /// Boîte de la voiture dans son propre repère (`min.y` = 0 : les pneus touchent le sol).
    pub bounds: Bounds,
}

impl CarFrame {
    /// Demi-longueur (m) : l'arrière de la voiture, où part la traînée.
    #[must_use]
    pub fn half_length(&self) -> f32 {
        self.bounds.size().z / 2.0
    }
}

impl CarModelAsset {
    /// Matrices des nœuds dans l'espace du GLB (chaîne des parents comprise), aux indices de [`Self::nodes`].
    #[must_use]
    pub fn node_matrices(&self) -> Vec<Mat4> {
        let mut matrices = vec![Mat4::IDENTITY; self.nodes.len()];
        let mut stack: Vec<(usize, Mat4)> = self.roots.iter().map(|&r| (r, Mat4::IDENTITY)).collect();
        while let Some((index, parent)) = stack.pop() {
            let matrix = parent * self.nodes[index].transform.to_matrix();
            matrices[index] = matrix;
            stack.extend(self.nodes[index].children.iter().map(|&c| (c, matrix)));
        }
        matrices
    }

    /// Sommets (espace du GLB) des pièces dont le matériau a ce rôle ; `None` : toutes les pièces.
    fn vertices(&self, role: impl Fn(MaterialRole) -> bool) -> Vec<Vec3> {
        let matrices = self.node_matrices();
        let mut out = Vec::new();
        for (node, matrix) in self.nodes.iter().zip(&matrices) {
            for &(mesh, material) in &node.pieces {
                if !role(self.materials[material].role) {
                    continue;
                }
                let positions = self.meshes[mesh]
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .and_then(|a| a.as_float3());
                out.extend(positions.into_iter().flatten().map(|&p| matrix.transform_point3(Vec3::from(p))));
            }
        }
        out
    }

    /// Le repère de la voiture (voir [`CarFrame`]).
    #[must_use]
    pub fn frame(&self) -> CarFrame {
        let mut bounds = Bounds::EMPTY;
        // Demi-tour autour de Y : (x, y, z) → (−x, y, −z).
        for p in self.vertices(|_| true) {
            bounds.include(Vec3::new(-p.x, p.y, -p.z));
        }
        if bounds.is_empty() {
            return CarFrame {
                to_car: Transform::from_rotation(Quat::from_rotation_y(std::f32::consts::PI)),
                bounds: Bounds {
                    min: Vec3::ZERO,
                    max: Vec3::ZERO,
                },
            };
        }
        let center = bounds.center();
        let shift = Vec3::new(-center.x, -bounds.min.y, -center.z);
        CarFrame {
            to_car: Transform {
                translation: shift,
                rotation: Quat::from_rotation_y(std::f32::consts::PI),
                scale: Vec3::ONE,
            },
            bounds: Bounds {
                min: bounds.min + shift,
                max: bounds.max + shift,
            },
        }
    }

    /// Centre de la zone portée par un matériau, un point par côté de la voiture, dans le repère de la voiture
    /// (`lampCenters` de `anchors.ts`). Vide si aucun sommet ne porte ce rôle.
    #[must_use]
    pub fn lamp_centers(&self, frame: &CarFrame, role: MaterialRole) -> HashMap<Side, Vec3> {
        let to_car = frame.to_car.to_matrix();
        let mut bounds: HashMap<Side, Bounds> = HashMap::new();
        for p in self.vertices(|r| r == role) {
            let p = to_car.transform_point3(p);
            let side = if p.x < 0.0 { Side::Left } else { Side::Right };
            bounds.entry(side).or_insert(Bounds::EMPTY).include(p);
        }
        bounds.into_iter().map(|(side, b)| (side, b.center())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car::model::Fore;

    fn car() -> CarModelAsset {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/car/car-low.glb");
        CarModelAsset::from_glb(&std::fs::read(path).expect("car-low.glb")).expect("modèle lisible")
    }

    #[test]
    fn la_voiture_est_posee_sur_le_sol_et_centree() {
        let car = car();
        let frame = car.frame();
        let size = frame.bounds.size();
        assert!(frame.bounds.min.y.abs() < 1e-4, "les pneus touchent y = 0");
        assert!(frame.bounds.center().x.abs() < 1e-4 && frame.bounds.center().z.abs() < 1e-4);
        // Quatre à cinq mètres de long (empattement 2,76 m), ~2 m de large, ~1,3 m de haut.
        assert!((4.0..5.5).contains(&size.z), "longueur {}", size.z);
        assert!((1.6..2.4).contains(&size.x), "largeur {}", size.x);
        assert!((1.0..1.8).contains(&size.y), "hauteur {}", size.y);
        assert!((frame.half_length() * 2.0 - size.z).abs() < 1e-5);
    }

    /// Le nez regarde vers −Z : les phares sont plus loin devant (z < 0) que les feux arrière.
    #[test]
    fn les_phares_sont_devant_et_les_feux_arriere_derriere() {
        let car = car();
        let frame = car.frame();
        let heads = car.lamp_centers(&frame, MaterialRole::LightHead);
        let tails = car.lamp_centers(&frame, MaterialRole::LightBrake);
        assert_eq!(heads.len(), 2, "un phare par côté");
        assert_eq!(tails.len(), 2, "un feu stop par côté");
        for (side, head) in &heads {
            assert!(head.z < 0.0, "phare {side:?} devant : {head}");
            assert!(tails[side].z > 0.0, "feu {side:?} derrière : {}", tails[side]);
            assert!(head.y > 0.2 && head.y < 1.0, "phare à hauteur de capot : {head}");
        }
        assert!(heads[&Side::Left].x < 0.0 && heads[&Side::Right].x > 0.0, "gauche à −x, droite à +x");
    }

    #[test]
    fn les_quatre_clignotants_se_retrouvent_a_l_avant_et_a_l_arriere() {
        let car = car();
        let frame = car.frame();
        for (fore, front) in [(Fore::Front, true), (Fore::Rear, false)] {
            for side in [Side::Left, Side::Right] {
                let centers = car.lamp_centers(&frame, MaterialRole::Signal(side, fore));
                assert!(!centers.is_empty(), "clignotant {side:?} {fore:?}");
                for c in centers.values() {
                    assert_eq!(c.z < 0.0, front, "{side:?} {fore:?} : {c}");
                }
            }
        }
    }
}
