//! Tests de la qualité des graffitis : l'ordre de retrait est celui de la spec et rien ne revient en montant le niveau.

use super::ProjectorSize::{Large, Medium, Small};
use super::quality::{GraffitiQuality, MIN_QUALITY_RANGE_M, SMALL_FAR_M, WALL_REACH_M, WALL_REACH_NEAR_M};

#[test]
fn chaque_niveau_retire_dans_l_ordre_de_la_spec() {
    let q: Vec<_> = (0..4).map(GraffitiQuality::for_level).collect();
    // Niveau 0 : tout. Puis les petits projecteurs lointains, puis le halo, puis les graffitis muraux lointains.
    assert!(q[0].holo_far && q[0].halo && q[0].wall_far && !q[0].minimal());
    assert!(!q[1].holo_far && q[1].halo && q[1].wall_far);
    assert!(!q[2].holo_far && !q[2].halo && q[2].wall_far);
    assert!(!q[3].wall_far && !q[3].halo && !q[3].holo_far && q[3].minimal());
    // Rien ne revient en dégradant : chaque niveau garde au plus ce que garde le précédent.
    for w in q.windows(2) {
        assert!(w[0].holo_far >= w[1].holo_far && w[0].halo >= w[1].halo && w[0].wall_far >= w[1].wall_far);
    }
    // Les niveaux au-delà du dernier valent le dernier.
    assert_eq!(GraffitiQuality::for_level(9), q[3]);
    assert_eq!(GraffitiQuality::default(), q[0]);
}

#[test]
fn en_qualite_minimale_seuls_les_proches_restent() {
    let min = GraffitiQuality::for_level(3);
    for size in [Small, Medium, Large] {
        assert!(min.keeps_projector(size, MIN_QUALITY_RANGE_M - 1.0));
        assert!(
            !min.keeps_projector(size, MIN_QUALITY_RANGE_M + 1.0),
            "{size:?} lointain gardé"
        );
    }
    assert!((min.wall_reach_m() - WALL_REACH_NEAR_M).abs() < f32::EPSILON);
    // Pleine qualité : tout reste, et les murs portent loin.
    let full = GraffitiQuality::for_level(0);
    assert!(full.keeps_projector(Small, 400.0));
    assert!((full.wall_reach_m() - WALL_REACH_M).abs() < f32::EPSILON);
}

#[test]
fn au_niveau_un_seuls_les_petits_lointains_partent() {
    let q = GraffitiQuality::for_level(1);
    assert!(!q.keeps_projector(Small, SMALL_FAR_M + 1.0));
    assert!(q.keeps_projector(Small, SMALL_FAR_M - 1.0));
    assert!(q.keeps_projector(Medium, 400.0) && q.keeps_projector(Large, 400.0));
}
