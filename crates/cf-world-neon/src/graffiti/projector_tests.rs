//! Tests purs du placement des projecteurs : distribution sur les emplacements et pose des pavés.

use super::ProjectorSize;
use super::atlas::{CellRect, HoloCell};
use super::projector::{MAX_PROJECTORS, assign, keep_nearest, layout};
use crate::scenery::graffiti_plan::{LETTER_LIFT, Projector};

fn at(z: f64) -> Projector {
    Projector { z, ..Projector::EMPTY }
}

fn cell(w: u16, h: u16, letters: f32) -> HoloCell {
    let rect = CellRect {
        layer: 0,
        x: 0,
        y: 0,
        w,
        h,
        letters,
    };
    HoloCell {
        sharp: rect,
        halo: rect,
    }
}

#[test]
fn on_garde_les_douze_plus_proches() {
    let mut all: Vec<(f64, Projector)> = (0..30).map(|i| (f64::from(29 - i), at(f64::from(i)))).collect();
    keep_nearest(&mut all);
    assert_eq!(all.len(), MAX_PROJECTORS);
    assert!(all.windows(2).all(|w| w[0].0 <= w[1].0));
    assert!(
        all.iter().all(|(d, _)| *d < MAX_PROJECTORS as f64),
        "les plus lointains sont écartés"
    );
}

#[test]
fn un_projecteur_garde_son_emplacement_quand_les_autres_changent() {
    let mut active = [None; MAX_PROJECTORS];
    let first: Vec<_> = (0..5).map(|i| (0.0, at(f64::from(i)))).collect();
    assign(&mut active, &first);
    let keeper = active[3];
    assert!(keeper.is_some());
    // Les voisins changent, `3` reste choisi : même emplacement, et l'ordre d'arrivée n'y change rien.
    let second = [(0.0, at(9.0)), (0.0, at(3.0)), (0.0, at(8.0))];
    let changed = assign(&mut active, &second);
    assert_eq!(active[3], keeper);
    assert!(!changed[3], "un emplacement inchangé n'est pas réécrit");
    assert_eq!(active.iter().flatten().count(), 3);
}

#[test]
fn un_emplacement_libere_est_signale_puis_reutilise() {
    let mut active = [None; MAX_PROJECTORS];
    assign(&mut active, &[(0.0, at(1.0)), (0.0, at(2.0))]);
    let changed = assign(&mut active, &[(0.0, at(2.0))]);
    assert!(changed[0] && active[0].is_none(), "l'emplacement 0 est libéré");
    let changed = assign(&mut active, &[(0.0, at(2.0)), (0.0, at(5.0))]);
    assert!(
        changed[0] && active[0] == Some(at(5.0)),
        "il est réutilisé avant tout autre"
    );
}

#[test]
fn la_pose_suit_la_classe_de_taille() {
    for size in [ProjectorSize::Small, ProjectorSize::Medium, ProjectorSize::Large] {
        let p = Projector {
            size,
            ..Projector::EMPTY
        };
        let lay = layout(&p, &cell(1000, 250, 0.5));
        assert!((lay.bar_scale.z - size.bar_len_m()).abs() < 1e-6);
        let letters_h = lay.text_scale.y * 0.5;
        assert!(
            letters_h <= size.letter_h_m() + 1e-4 && letters_h > size.letter_h_m() * 0.4,
            "{size:?} : lettres de {letters_h} m"
        );
        assert!(
            lay.text_scale.x <= size.bar_len_m() * 2.0 + 1e-4,
            "le texte déborde de deux barres"
        );
        // Le texte flotte au-dessus de la barre, séparé d'elle par le faisceau.
        let bottom = lay.text_pos.y - letters_h / 2.0;
        assert!((bottom - (0.25 + letters_h * LETTER_LIFT)).abs() < 1e-3);
    }
}

#[test]
fn le_panneau_regarde_la_route_des_deux_cotes() {
    for side in [-1_i8, 1] {
        let p = Projector {
            side,
            ..Projector::EMPTY
        };
        let lay = layout(&p, &cell(1000, 250, 0.5));
        let normal = lay.text_rot * bevy::math::Vec3::Z;
        assert!(
            (normal.x + f32::from(side)).abs() < 1e-5,
            "côté {side} : normale {normal}"
        );
        // Le texte se lit de gauche à droite vu de la voiture (qui roule vers −z) : +X du panneau = côté × z.
        let right = lay.text_rot * bevy::math::Vec3::X;
        assert!((right.z - f32::from(side)).abs() < 1e-5, "côté {side} : droite {right}");
    }
}
