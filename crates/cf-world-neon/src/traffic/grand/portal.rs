//! Les portiques de la ville (`traffic/portals.ts`, `PortalView`) : deux montants et une barre en lumière HDR, plus un
//! rideau d'hologramme entre les montants. Sert à la course de drones et au tunnel de lumière.
//!
//! La barre est le seul obstacle (voir [`obstruction`]) : le rideau est de la lumière, la voiture le traverse. Les
//! panneaux sont reposés à chaque image par l'événement (voir `panels`).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use crate::pilot::flight::Obstruction;
use crate::traffic::panels::PanelKind;
use crate::traffic::scene::Scene;

/// Les montants sont en bordure de chaussée (|x| < 9), hors du passage de la voiture (±4,6).
pub(crate) const POST_X_M: f32 = 9.8;
const POST_M: f32 = 1.0;
/// Le haut du portique est à 12,3 m : comme les portiques néon de la route, il laisse passer les étages 1 et 2 dessous et
/// le 3 dessus.
pub(crate) const BAR_BOTTOM_M: f32 = 11.2;
pub(crate) const BAR_TOP_M: f32 = 12.3;
const CURTAIN_HEIGHT_M: f32 = BAR_BOTTOM_M - 0.2;
/// Au-dessus de 1, la couleur bave dans le bloom.
const HDR: f32 = 2.4;
/// Sous cet éclat, le rideau n'est pas dessiné.
const CURTAIN_MIN: f32 = 0.01;

/// Pose le portique en `z` : la structure à la teinte `color`, et le rideau d'éclat `curtain` (0 l'éteint).
pub(crate) fn draw(scene: &mut Scene, z: f64, color: LinearRgba, curtain: f32) {
    for side in [-1.0_f32, 1.0] {
        scene.panels.block(
            PanelKind::Post,
            PanelKind::Post,
            DVec3::new(f64::from(side * POST_X_M), 0.0, z),
            Vec3::new(POST_M, BAR_TOP_M, POST_M),
            false,
            color,
            HDR,
            0.0,
        );
    }
    scene.panels.block(
        PanelKind::Post,
        PanelKind::Post,
        DVec3::new(0.0, f64::from(BAR_BOTTOM_M), z),
        Vec3::new(POST_X_M * 2.0 + POST_M, BAR_TOP_M - BAR_BOTTOM_M, 1.2),
        false,
        color,
        HDR,
        0.0,
    );
    if curtain > CURTAIN_MIN {
        scene.panels.quad(
            PanelKind::Curtain,
            DVec3::new(0.0, f64::from(CURTAIN_HEIGHT_M * 0.5), z),
            Vec3::new((POST_X_M * 2.0 - POST_M) * 0.5, 0.0, 0.0),
            Vec3::new(0.0, CURTAIN_HEIGHT_M * 0.5, 0.0),
            color,
            curtain,
            0.0,
        );
    }
}

/// La barre du portique `z` : de quoi décider d'une esquive à la voiture volante.
pub(crate) fn obstruction(z: f64) -> Obstruction {
    Obstruction {
        z_near: z + 0.9,
        z_far: z - 0.9,
        lo: f64::from(BAR_BOTTOM_M),
        hi: f64::from(BAR_TOP_M),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_portique_pose_trois_pieces_et_un_rideau_quand_il_est_allume() {
        let mut scene = Scene::new();
        scene.begin(DVec3::ZERO);
        draw(&mut scene, -300.0, LinearRgba::WHITE, 0.0);
        // Deux montants et la barre : 5 faces chacun, dans la couche pleine ; pas de rideau éteint.
        assert_eq!(scene.panels.solid().len(), 15);
        assert_eq!(scene.panels.light().len(), 0);
        draw(&mut scene, -380.0, LinearRgba::WHITE, 1.0);
        assert_eq!(scene.panels.light().len(), 1);
    }

    #[test]
    fn la_barre_est_a_la_hauteur_des_portiques_neon_de_la_route() {
        let o = obstruction(-100.0);
        assert!(o.lo > 11.0 && o.hi < 12.5 && o.z_near > o.z_far);
    }
}
