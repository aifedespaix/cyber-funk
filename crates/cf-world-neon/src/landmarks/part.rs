//! Les pièces d'un immeuble-vitrine : ce qu'il montre (maillages, matériau) et ce qu'il écrit à chaque image (pose,
//! paramètres de shader). Pur : aucune entité, aucun asset. La couche Bevy ([`crate::landmarks::render`]) en tire ses
//! entités une fois à l'ouverture, puis recopie le [`PartPaint`] de chaque pièce à chaque image.
//!
//! C'est le pendant, côté données, des `Mesh`, `InstancedMesh` et `ShaderMaterial` que chaque immeuble de Sillage créait dans
//! son constructeur.

use bevy::math::{Vec2, Vec3, Vec4};
use bevy::transform::components::Transform;

/// Le programme de shader d'une pièce (un fichier `landmark_*.wesl` par rendu).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Look {
    /// Le mur de code, façade ou bout du mur (`matrix-building.ts`).
    MatrixWall,
    /// Les grands glyphes qui jaillissent du mur.
    MatrixSprouts,
    /// La façade de lave (`lava-building.ts`).
    LavaFacade,
    /// La flaque de lave au pied de la façade.
    LavaPool,
    /// Les couches du panneau géant ERROR (`error-building.ts`).
    ErrorSign,
    /// La façade rouge d'ERROR.
    ErrorFacade,
    /// Le faisceau de projection (`projection.ts`).
    Beam,
    /// L'hologramme arcade (`arcade-building.ts`).
    ArcadeHolo,
    /// La façade « carte mère » (`board-tower.ts`).
    BoardFacade,
    /// Les voxels de l'arche (`voxel-arch.ts`).
    ArchVoxels,
    /// Un os de robot (`mecha/panel.ts`).
    MechaPanel,
    /// L'écran de la tête d'un robot (`mecha/face.ts`).
    MechaFace,
    /// Un aplat sans éclairage (mâts et traverse du panneau ERROR : `MeshBasicMaterial` de Sillage).
    Iron,
}

impl Look {
    /// Tous les rendus, dans l'ordre de leur numéro.
    pub const ALL: [Self; 13] = [
        Self::MatrixWall,
        Self::MatrixSprouts,
        Self::LavaFacade,
        Self::LavaPool,
        Self::ErrorSign,
        Self::ErrorFacade,
        Self::Beam,
        Self::ArcadeHolo,
        Self::BoardFacade,
        Self::ArchVoxels,
        Self::MechaPanel,
        Self::MechaFace,
        Self::Iron,
    ];

    /// Lumière pure : additif, transparent, sans écriture de profondeur (`additive` d'`overlayMaterial`).
    #[must_use]
    pub const fn additive(self) -> bool {
        matches!(
            self,
            Self::MatrixSprouts
                | Self::LavaPool
                | Self::ErrorSign
                | Self::ErrorFacade
                | Self::Beam
                | Self::ArcadeHolo
                | Self::ArchVoxels
        )
    }

    /// Pousse la surface devant le bloc d'immeuble pour éviter le z-fighting (`decal` d'`overlayMaterial`).
    #[must_use]
    pub const fn decal(self) -> bool {
        matches!(
            self,
            Self::MatrixWall
                | Self::LavaFacade
                | Self::LavaPool
                | Self::ErrorFacade
                | Self::BoardFacade
                | Self::MechaFace
        )
    }

    /// Les deux faces se voient (`DoubleSide` : tout ce qui est additif).
    #[must_use]
    pub const fn double_sided(self) -> bool {
        self.additive()
    }
}

/// La texture que lit une pièce, s'il y en a une.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MapId {
    /// L'atlas du mot ERROR, cuit à l'ouverture.
    ErrorAtlas,
    /// La grille de l'arcade (une case par octet, 72 × 48), réécrite par la partie qui joue.
    ArcadeGrid,
}

/// Le maillage d'une pièce, en données pures.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MeshData {
    /// Positions.
    pub positions: Vec<[f32; 3]>,
    /// Normales.
    pub normals: Vec<[f32; 3]>,
    /// Coordonnées de texture, **v vers le haut** (comme `vUv` de Sillage ; `Rectangle` de Bevy a v vers le bas).
    pub uvs: Vec<[f32; 2]>,
    /// Deuxième jeu de coordonnées (les données par sommet des maillages fusionnés).
    pub uv1: Vec<[f32; 2]>,
    /// Une couleur par sommet (les données par sommet des maillages fusionnés).
    pub colors: Vec<[f32; 4]>,
    /// Une tangente par sommet : quatre flottants de données par sommet de plus (les voxels de l'arche).
    pub tangents: Vec<[f32; 4]>,
    /// Triangles.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// Un plan centré de `w` × `h` m, face à +Z, v vers le haut (`PlaneGeometry`).
    #[must_use]
    pub fn quad(w: f32, h: f32) -> Self {
        let mut mesh = Self::default();
        mesh.push_quad(Vec3::ZERO, Vec2::new(w, h));
        mesh
    }

    /// Ajoute un plan centré en `center`, de `size`, face à +Z, v vers le haut. Rend l'indice de son premier sommet.
    pub fn push_quad(&mut self, center: Vec3, size: Vec2) -> u32 {
        let first = self.positions.len() as u32;
        let (hw, hh) = (size.x * 0.5, size.y * 0.5);
        for (dx, dy, u, v) in [
            (-hw, -hh, 0.0, 0.0),
            (hw, -hh, 1.0, 0.0),
            (hw, hh, 1.0, 1.0),
            (-hw, hh, 0.0, 1.0),
        ] {
            self.positions.push([center.x + dx, center.y + dy, center.z]);
            self.normals.push([0.0, 0.0, 1.0]);
            self.uvs.push([u, v]);
        }
        self.indices
            .extend([first, first + 1, first + 2, first, first + 2, first + 3]);
        first
    }

    /// Un cube unité centré sur l'origine, normales par face (`BoxGeometry(1, 1, 1)`).
    #[must_use]
    pub fn cube() -> Self {
        let mut mesh = Self::default();
        mesh.push_cube(Vec3::ZERO, 1.0);
        mesh
    }

    /// Ajoute un cube d'arête `edge` centré en `center`. Rend l'indice de son premier sommet.
    pub fn push_cube(&mut self, center: Vec3, edge: f32) -> u32 {
        let first = self.positions.len() as u32;
        let h = edge * 0.5;
        // Six faces : normale, deux axes du plan, dans le sens trigonométrique vu de l'extérieur.
        let faces = [
            (Vec3::X, Vec3::NEG_Z, Vec3::Y),
            (Vec3::NEG_X, Vec3::Z, Vec3::Y),
            (Vec3::Y, Vec3::X, Vec3::NEG_Z),
            (Vec3::NEG_Y, Vec3::X, Vec3::Z),
            (Vec3::Z, Vec3::X, Vec3::Y),
            (Vec3::NEG_Z, Vec3::NEG_X, Vec3::Y),
        ];
        for (normal, u_axis, v_axis) in faces {
            let base = self.positions.len() as u32;
            for (su, sv, u, v) in [
                (-1.0, -1.0, 0.0, 0.0),
                (1.0, -1.0, 1.0, 0.0),
                (1.0, 1.0, 1.0, 1.0),
                (-1.0, 1.0, 0.0, 1.0),
            ] {
                let p = center + (normal + u_axis * su + v_axis * sv) * h;
                self.positions.push(p.to_array());
                self.normals.push(normal.to_array());
                self.uvs.push([u, v]);
            }
            self.indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        first
    }

    /// Une pyramide ouverte : sommet à l'origine, base carrée de demi-côté 1 à z = 1 (le faisceau de projection de
    /// `projection.ts`). Quatre faces, pas de fond ; le matériau les montre des deux côtés.
    #[must_use]
    pub fn pyramid() -> Self {
        let mut mesh = Self::default();
        let corners = [[-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0]];
        for i in 0..4 {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            let base = mesh.positions.len() as u32;
            mesh.positions.extend([[0.0, 0.0, 0.0], a, b]);
            let normal = Vec3::from(a).cross(Vec3::from(b)).normalize_or_zero();
            mesh.normals.extend([normal.to_array(); 3]);
            mesh.uvs.extend([[0.5, 0.0], [0.0, 1.0], [1.0, 1.0]]);
            mesh.indices.extend([base, base + 1, base + 2]);
        }
        mesh
    }

    /// Nombre de sommets.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// La boîte englobante des positions, ou `None` sans sommet.
    #[must_use]
    pub fn bounds(&self) -> Option<(Vec3, Vec3)> {
        let first = Vec3::from(*self.positions.first()?);
        Some(self.positions.iter().fold((first, first), |(lo, hi), p| {
            (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p)))
        }))
    }
}

/// Le maillage d'une pièce : un plan, un cube unité, ou un maillage fusionné (voxels, glyphes…).
#[derive(Debug, Clone, PartialEq)]
pub enum MeshSpec {
    /// Plan centré `w` × `h`, face à +Z.
    Quad {
        /// Largeur, m.
        w: f32,
        /// Hauteur, m.
        h: f32,
    },
    /// Cube unité centré (mis à l'échelle par la pose de la pièce).
    Cube,
    /// Maillage fusionné, avec données par sommet.
    Custom(Box<MeshData>),
}

/// Une pièce de l'immeuble : maillage, rendu, texture, boîte englobante.
#[derive(Debug, Clone, PartialEq)]
pub struct PartSpec {
    /// Nom (diagnostic).
    pub name: &'static str,
    /// Le maillage.
    pub mesh: MeshSpec,
    /// Le rendu.
    pub look: Look,
    /// La texture lue, s'il y en a une.
    pub map: Option<MapId>,
    /// `Aabb` explicite (min, max) en repère de la pièce, pour ce que le vertex shader déplace : sans lui l'enveloppe des
    /// positions de repos serait fausse et la pièce disparaîtrait à tort. `None` : l'enveloppe du maillage.
    pub bounds: Option<(Vec3, Vec3)>,
}

impl PartSpec {
    /// Une pièce sans texture et sans enveloppe imposée.
    #[must_use]
    pub fn new(name: &'static str, mesh: MeshSpec, look: Look) -> Self {
        Self {
            name,
            mesh,
            look,
            map: None,
            bounds: None,
        }
    }

    /// Lit cette texture.
    #[must_use]
    pub fn with_map(mut self, map: MapId) -> Self {
        self.map = Some(map);
        self
    }

    /// Impose l'enveloppe de la pièce (`min`, `max`, repère de la pièce).
    #[must_use]
    pub fn with_bounds(mut self, min: Vec3, max: Vec3) -> Self {
        self.bounds = Some((min, max));
        self
    }
}

/// Ce qu'une pièce montre à cette image : sa pose dans le repère de l'immeuble et les paramètres de son shader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartPaint {
    /// Dessinée ? Faux : cachée (un faisceau éteint, un robot hors scène).
    pub visible: bool,
    /// Pose dans le repère de l'immeuble (+Z regarde la route, +X est à droite du conducteur).
    pub transform: Transform,
    /// Teinte d'ambiance de la pièce (`uMood`, lumière linéaire).
    pub mood: Vec3,
    /// Paramètres propres au rendu (voir le shader de chaque [`Look`]).
    pub a: Vec4,
    /// Idem.
    pub b: Vec4,
    /// Idem.
    pub c: Vec4,
    /// Idem.
    pub d: Vec4,
}

impl Default for PartPaint {
    fn default() -> Self {
        Self {
            visible: true,
            transform: Transform::IDENTITY,
            mood: Vec3::ZERO,
            a: Vec4::ZERO,
            b: Vec4::ZERO,
            c: Vec4::ZERO,
            d: Vec4::ZERO,
        }
    }
}

/// Le corps d'un immeuble : le bloc fenêtré qui plonge sous la route comme les gratte-ciel (`BlockSet`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BodySpec {
    /// Hauteur au-dessus de la route, m.
    pub height: f32,
    /// Profondeur (vers −Z), m.
    pub depth: f32,
    /// Graine des fenêtres.
    pub seed: u32,
}

/// L'emprise verticale d'une structure qui enjambe la route, pour l'esquive de la voiture (`roadBand`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadBand {
    /// Dessous, m.
    pub lo: f32,
    /// Dessus, m.
    pub hi: f32,
    /// Demi-épaisseur le long de la route, m.
    pub half_depth: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_plan_a_quatre_sommets_et_v_vers_le_haut() {
        let quad = MeshData::quad(4.0, 2.0);
        assert_eq!(quad.vertex_count(), 4);
        assert_eq!(quad.indices.len(), 6);
        // Le sommet en haut à droite a (u, v) = (1, 1) : v monte avec y.
        let top = quad
            .positions
            .iter()
            .zip(&quad.uvs)
            .find(|(p, _)| p[1] > 0.0 && p[0] > 0.0)
            .map(|(_, uv)| *uv);
        assert_eq!(top, Some([1.0, 1.0]));
    }

    #[test]
    fn un_cube_a_vingt_quatre_sommets_et_une_enveloppe_unite() {
        let cube = MeshData::cube();
        assert_eq!(cube.vertex_count(), 24);
        assert_eq!(cube.indices.len(), 36);
        let (lo, hi) = cube.bounds().unwrap_or_default();
        assert!((lo + Vec3::splat(0.5)).length() < 1e-6 && (hi - Vec3::splat(0.5)).length() < 1e-6);
    }

    #[test]
    fn les_normales_du_cube_sortent_du_cube() {
        let cube = MeshData::cube();
        for (p, n) in cube.positions.iter().zip(&cube.normals) {
            assert!(Vec3::from(*p).dot(Vec3::from(*n)) > 0.4, "normale rentrante en {p:?}");
        }
    }

    #[test]
    fn les_triangles_regardent_vers_l_exterieur() {
        let cube = MeshData::cube();
        for tri in cube.indices.chunks(3) {
            let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(cube.positions[i as usize]));
            let normal = (b - a).cross(c - a);
            assert!(normal.dot(Vec3::from(cube.normals[tri[0] as usize])) > 0.0);
        }
    }
}
