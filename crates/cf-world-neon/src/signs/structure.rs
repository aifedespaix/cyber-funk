//! Ce qui porte un panneau (`signs/structure.ts`) : caisson, liseré lumineux, pieds du portique ou de la potence,
//! pylône de toit, mât de néon, câbles sous un vaisseau. Toutes les pièces sont des boîtes (ou des cylindres) à l'échelle
//! d'une seule géométrie ; ce module ne calcule que leur échelle et leur place, en repère du panneau (origine au centre,
//! +Z vers la voiture). Il est pur : les entités sont dans `systems`.

use bevy::math::Vec3;

use super::formats::Mount;

/// Épaisseur du caisson, en m.
pub const SLAB_DEPTH: f32 = 0.5;
const STRIP: f32 = 0.3;
/// Les pieds plongent bien sous la route, comme les immeubles : le panneau semble sortir de très profond.
pub const SINK: f32 = 420.0;
/// Longueur des câbles qui retiennent un panneau sous un vaisseau, en m.
pub const ROPE: f32 = 9.0;
/// Avance du plan d'image devant le caisson, en m (`SLAB_FRONT`).
pub const SLAB_FRONT: f32 = 0.28;

/// Les emplacements de pièces d'un panneau, dans l'ordre où `systems` crée ses entités.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartKind {
    /// Le caisson.
    Slab,
    /// Un des quatre lisérés lumineux.
    Strip(u8),
    /// Un pied en pavé droit.
    Post(u8),
    /// Un pied en cylindre.
    Round(u8),
    /// La poutre.
    Beam,
    /// La corniche des pylônes de toit.
    Rim,
    /// Une balise rouge de pylône.
    Beacon(u8),
}

/// Nombre d'emplacements de pièces.
pub const PART_COUNT: usize = 13;

impl PartKind {
    /// Toutes les pièces, dans l'ordre de leur indice.
    pub const ALL: [Self; PART_COUNT] = [
        Self::Slab,
        Self::Strip(0),
        Self::Strip(1),
        Self::Strip(2),
        Self::Strip(3),
        Self::Post(0),
        Self::Post(1),
        Self::Round(0),
        Self::Round(1),
        Self::Beam,
        Self::Rim,
        Self::Beacon(0),
        Self::Beacon(1),
    ];

    /// Indice de la pièce dans [`Fit::parts`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Slab => 0,
            Self::Strip(i) => 1 + i as usize,
            Self::Post(i) => 5 + i as usize,
            Self::Round(i) => 7 + i as usize,
            Self::Beam => 9,
            Self::Rim => 10,
            Self::Beacon(i) => 11 + i as usize,
        }
    }
}

/// Échelle et position d'une pièce dans le repère du panneau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Part {
    /// Dimensions (m).
    pub scale: Vec3,
    /// Centre (m).
    pub pos: Vec3,
}

/// La structure posée pour un format donné.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Fit {
    /// Les pièces visibles (`None` : cachée), indexées par [`PartKind::index`].
    pub parts: [Option<Part>; PART_COUNT],
    /// Il y a un caisson derrière l'image.
    pub backed: bool,
    /// Il y a un liseré.
    pub lit: bool,
}

impl Fit {
    fn put(&mut self, kind: PartKind, scale: Vec3, pos: Vec3) {
        self.parts[kind.index()] = Some(Part { scale, pos });
    }

    /// Une pièce est-elle visible ? `display` : l'affichage n'est pas éteint par un bug (`setDisplay`) : le caisson et le
    /// liseré s'éteignent avec lui, les pieds, la poutre et les câbles restent.
    #[must_use]
    pub fn shown(&self, kind: PartKind, display: bool) -> bool {
        let Some(_) = self.parts[kind.index()] else { return false };
        match kind {
            PartKind::Slab => display && self.backed,
            PartKind::Strip(_) => display && self.lit,
            _ => true,
        }
    }
}

/// Pose la structure d'un panneau de `w` × `h` m. `drop` : hauteur du centre du panneau au-dessus du sol (les pieds
/// descendent jusque-là) ; `round` : pieds ronds plutôt que carrés.
#[must_use]
pub fn fit(mount: Mount, w: f32, h: f32, side: f32, drop: f32, backed: bool, lit: bool, round: bool) -> Fit {
    let mut f = Fit {
        backed,
        lit: backed && lit,
        ..Fit::default()
    };
    let feet = |i: u8| if round { PartKind::Round(i) } else { PartKind::Post(i) };
    let v = Vec3::new;
    if backed {
        f.put(PartKind::Slab, v(w + 0.8, h + 0.8, SLAB_DEPTH), Vec3::ZERO);
        if lit {
            let z = SLAB_DEPTH / 2.0 + 0.05;
            f.put(PartKind::Strip(0), v(w + 0.8, STRIP, STRIP), v(0.0, h / 2.0 + 0.4, z));
            f.put(PartKind::Strip(1), v(w + 0.8, STRIP, STRIP), v(0.0, -h / 2.0 - 0.4, z));
            f.put(PartKind::Strip(2), v(STRIP, h + 0.8, STRIP), v(-w / 2.0 - 0.4, 0.0, z));
            f.put(PartKind::Strip(3), v(STRIP, h + 0.8, STRIP), v(w / 2.0 + 0.4, 0.0, z));
        }
    }
    let legs = |f: &mut Fit, x: f32, signs: &[f32], width: f32| {
        let top = h / 2.0 + 1.5;
        for (i, sign) in signs.iter().enumerate() {
            f.put(
                feet(i as u8),
                v(width, top + drop + SINK, width),
                v(sign * x, (top - drop - SINK) / 2.0, -0.2),
            );
        }
    };
    match mount {
        Mount::Gantry => {
            legs(&mut f, w / 2.0 + 1.2, &[-1.0, 1.0], 1.2);
            f.put(PartKind::Beam, v(w + 3.6, 1.0, 1.2), v(0.0, h / 2.0 + 1.0, -0.2));
        }
        Mount::Arm => {
            legs(&mut f, w / 2.0 + 1.2, &[side], 1.2);
            f.put(PartKind::Beam, v(w + 2.4, 1.0, 1.2), v(side * 1.2, h / 2.0 + 1.0, -0.2));
        }
        Mount::Pole => {
            legs(&mut f, w / 2.0 + 0.6, &[-1.0], 0.6);
            f.put(PartKind::Beam, v(1.6, 0.3, 0.3), v(-w / 2.0, h / 2.0 - 0.4, -0.2));
        }
        Mount::Tower => {
            // Deux jambes de pylône sous une corniche, et des balises aux deux angles.
            let top = -h / 2.0 - 0.4;
            for (i, sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
                f.put(
                    feet(i as u8),
                    v(1.8, top + drop + SINK, 2.0),
                    v(sign * w * 0.3, (top - drop - SINK) / 2.0, -1.0),
                );
            }
            f.put(PartKind::Rim, v(w * 0.92, 0.8, 3.0), v(0.0, -h / 2.0 - 0.4, -0.6));
            f.put(PartKind::Beacon(0), Vec3::splat(0.9), v(-w / 2.0, h / 2.0 + 0.9, 0.0));
            f.put(PartKind::Beacon(1), Vec3::splat(0.9), v(w / 2.0, h / 2.0 + 0.9, 0.0));
        }
        Mount::Hang => {
            // Deux câbles verticaux jusqu'à un palonnier : le vaisseau s'accroche au palonnier, le panneau pend dessous.
            let top = h / 2.0 + 0.4;
            for (i, sign) in [-1.0_f32, 1.0].into_iter().enumerate() {
                f.put(feet(i as u8), v(0.22, ROPE, 0.22), v(sign * w * 0.42, top + ROPE / 2.0, -0.2));
            }
            f.put(PartKind::Beam, v(w + 1.5, 0.8, 0.8), v(0.0, top + ROPE, -0.2));
        }
        Mount::None => {}
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_portique_a_deux_pieds_une_poutre_et_un_caisson() {
        let f = fit(Mount::Gantry, 20.0, 10.0, 1.0, 12.5, true, true, false);
        let shown = |k| f.parts[PartKind::index(k)].is_some();
        assert!(shown(PartKind::Slab) && shown(PartKind::Beam) && shown(PartKind::Post(0)) && shown(PartKind::Post(1)));
        assert!(!shown(PartKind::Round(0)) && !shown(PartKind::Rim));
        // Les pieds descendent jusque sous la route : le haut est au-dessus du panneau, le bas à `SINK` sous le sol.
        let post = f.parts[PartKind::Post(0).index()].map(|p| (p.pos.y - p.scale.y / 2.0, p.pos.y + p.scale.y / 2.0));
        let (bottom, top) = post.unwrap_or_default();
        assert!((bottom + 12.5 + SINK).abs() < 1e-3, "bas = {bottom}");
        assert!((top - 6.5).abs() < 1e-3, "haut = {top}");
    }

    #[test]
    fn la_potence_n_a_qu_un_pied_du_cote_de_la_route() {
        for side in [-1.0_f32, 1.0] {
            let f = fit(Mount::Arm, 15.0, 7.5, side, 12.0, true, true, true);
            assert!(f.parts[PartKind::Round(0).index()].is_some() && f.parts[PartKind::Round(1).index()].is_none());
            let x = f.parts[PartKind::Round(0).index()].map_or(0.0, |p| p.pos.x);
            assert!((x - side * 8.7).abs() < 1e-4);
        }
    }

    #[test]
    fn un_bug_d_affichage_eteint_le_caisson_et_le_filet_pas_les_pieds() {
        let f = fit(Mount::Gantry, 20.0, 10.0, 1.0, 12.0, true, true, false);
        assert!(f.shown(PartKind::Slab, true) && f.shown(PartKind::Strip(2), true));
        assert!(!f.shown(PartKind::Slab, false) && !f.shown(PartKind::Strip(0), false));
        assert!(f.shown(PartKind::Post(0), false) && f.shown(PartKind::Beam, false));
    }

    #[test]
    fn la_projection_n_a_aucune_structure_et_le_neon_pas_de_caisson() {
        let none = fit(Mount::None, 42.0, 21.0, 1.0, 50.0, false, true, false);
        assert!(none.parts.iter().all(Option::is_none));
        let neon = fit(Mount::Pole, 11.0, 5.5, 1.0, 15.0, true, false, false);
        assert!(neon.parts[PartKind::Slab.index()].is_some() && neon.parts[PartKind::Strip(0).index()].is_none());
        let tower = fit(Mount::Tower, 44.0, 22.0, 1.0, 70.0, true, true, false);
        assert!(tower.parts[PartKind::Beacon(1).index()].is_some() && tower.parts[PartKind::Rim.index()].is_some());
    }
}
