//! L'ossature métallique de l'enseigne (`dj-sign-frame.ts`) : une poutre-treillis sur toute la largeur, deux pieds et une
//! traverse en croix derrière chaque lettre. Ici, seulement la liste des boîtes (pure) ; la couche Bevy en fait un maillage
//! unique, reconstruit quand la mise en page change.

use bevy::math::Vec3;

use super::layout::SignLayout;

/// Nombre de boîtes au plus (`CAPACITY`) : 20 lettres en demandent environ 180.
pub const CAPACITY: usize = 320;
/// Hauteur des pieds : la base de la lettre est à cette altitude au-dessus du toit.
pub const LEG_HEIGHT: f32 = 5.5;
const BEAM: f32 = 0.55;
/// Pas du treillis, m.
const CELL_M: f32 = 5.0;

/// Une poutre : un pavé centré en `center`, de dimensions `size`, tourné de `roll` autour de z.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameBox {
    /// Centre, repère de l'enseigne.
    pub center: Vec3,
    /// Dimensions (x, y, z), m.
    pub size: Vec3,
    /// Rotation autour de z, rad.
    pub roll: f32,
}

struct Boxes(Vec<FrameBox>);

impl Boxes {
    fn put(&mut self, px: f32, py: f32, pz: f32, width: f32, height: f32, depth: f32, roll: f32) {
        if self.0.len() < CAPACITY {
            self.0.push(FrameBox {
                center: Vec3::new(px, py, pz),
                size: Vec3::new(width, height, depth),
                roll,
            });
        }
    }
}

/// Les boîtes de l'ossature sous les lettres de `layout` (repère de l'enseigne : x le long du panneau, y vers le haut,
/// z vers la route). Vide si l'enseigne n'a pas de lettre.
#[must_use]
pub fn build(layout: &SignLayout) -> Vec<FrameBox> {
    let mut b = Boxes(Vec::with_capacity(CAPACITY));
    if layout.letters.is_empty() {
        return b.0;
    }
    let cap = layout.height;
    let half = layout.width / 2.0 + 3.0;
    // Treillis de base : deux membrures et des montants en zigzag.
    b.put(0.0, 0.5, 0.0, half * 2.0, 0.9, 1.6, 0.0);
    b.put(0.0, LEG_HEIGHT - 0.6, 0.0, half * 2.0, 0.9, 1.6, 0.0);
    let cells = ((half * 2.0 / CELL_M).round() as usize).max(2);
    let step = half * 2.0 / cells as f32;
    let rise = LEG_HEIGHT - 1.2;
    for i in 0..cells {
        let x = -half + (i as f32 + 0.5) * step;
        let angle = rise.atan2(step) * if i % 2 == 0 { 1.0 } else { -1.0 };
        b.put(x, LEG_HEIGHT / 2.0 - 0.05, 0.0, step.hypot(rise), 0.35, 0.45, angle);
        b.put(
            x - half / cells as f32,
            LEG_HEIGHT / 2.0 - 0.05,
            0.0,
            0.4,
            rise,
            0.5,
            0.0,
        );
    }
    // Derrière chaque lettre : deux pieds jusqu'au haut, une traverse, une diagonale.
    for letter in &layout.letters {
        let reach = letter.width * 0.36;
        let top = LEG_HEIGHT + cap * 0.9;
        for side in [-1.0_f32, 1.0] {
            b.put(
                letter.x + side * reach,
                f32::midpoint(LEG_HEIGHT, top),
                -0.9,
                BEAM,
                top - LEG_HEIGHT,
                BEAM,
                0.0,
            );
        }
        b.put(
            letter.x,
            LEG_HEIGHT + cap * 0.5,
            -0.9,
            reach * 2.1,
            BEAM * 0.8,
            BEAM * 0.8,
            0.0,
        );
        let run = reach * 2.0;
        let lift = cap * 0.8;
        b.put(
            letter.x,
            LEG_HEIGHT + cap * 0.45,
            -1.1,
            run.hypot(lift),
            BEAM * 0.6,
            BEAM * 0.6,
            lift.atan2(run),
        );
        b.put(letter.x, 0.4, 0.2, letter.width * 0.95, 0.5, 2.2, 0.0);
    }
    b.0
}

#[cfg(test)]
mod tests {
    use super::super::layout::{SIGN_LAYOUT, layout_sign};
    use super::*;

    #[test]
    fn sans_lettre_pas_d_ossature() {
        assert!(build(&SignLayout::empty()).is_empty(), "pas de lettre, pas de poutre");
    }

    #[test]
    fn le_plus_long_nom_tient_dans_la_capacite() {
        let layout = layout_sign("ABCDEFGHIJKLMNOPQRST", &|_| 0.7, &SIGN_LAYOUT);
        let boxes = build(&layout);
        assert!(boxes.len() < CAPACITY, "{}", boxes.len());
        assert!(boxes.iter().all(|b| b.size.min_element() > 0.0));
    }

    #[test]
    fn l_ossature_couvre_la_largeur_des_lettres() {
        let layout = layout_sign("DJ NEON", &|_| 0.7, &SIGN_LAYOUT);
        let boxes = build(&layout);
        let left = boxes
            .iter()
            .map(|b| b.center.x - b.size.x / 2.0)
            .fold(f32::MAX, f32::min);
        let right = boxes
            .iter()
            .map(|b| b.center.x + b.size.x / 2.0)
            .fold(f32::MIN, f32::max);
        assert!(left < -layout.width / 2.0 && right > layout.width / 2.0);
    }
}
