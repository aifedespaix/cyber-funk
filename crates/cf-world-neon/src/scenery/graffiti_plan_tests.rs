//! Tests des tirages de graffitis muraux et de projecteurs holographiques.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use bevy::math::DVec3;
use cf_show::district::{DistrictId, DistrictKind};

use super::graffiti_plan::{
    GraffitiLibrary, LARGE_MIN_HEIGHT_M, MEDIUM_MIN_HEIGHT_M, Projector, ProjectorList, row_projectors,
};
use super::layout::{Part, Row, SPACING_M, Tier};
use super::query::collides;
use crate::graffiti::{GraffitiStyle, GraffitiWord, Placement, ProjectorSize};
use crate::scenery::SceneryConfig;

const ROWS: i64 = 2_000;

fn word(text: &str, wall: bool, holo: bool) -> GraffitiWord {
    GraffitiWord {
        text: text.to_string(),
        font: "F".to_string(),
        style: GraffitiStyle::Fill,
        wall,
        holo,
        hues: vec![300.0],
    }
}

fn words() -> Vec<GraffitiWord> {
    vec![
        word("A", true, false),
        word("B", true, true),
        word("C", false, true),
        word("D", true, false),
        word("E", false, true),
    ]
}

fn placement(z_m: f64, side: i8, height_m: f32, size: ProjectorSize, word: &str) -> Placement {
    Placement {
        z_m,
        side,
        height_m,
        size,
        word: word.to_string(),
        yaw_deg: 0.0,
    }
}

fn library(placements: &[Placement]) -> GraffitiLibrary {
    let (lib, warnings) = GraffitiLibrary::new(&words(), placements);
    assert!(warnings.is_empty(), "{warnings:?}");
    lib
}

fn config_with(kind: Option<DistrictKind>, lib: GraffitiLibrary) -> SceneryConfig {
    let mut config = SceneryConfig::default();
    if let Some(kind) = kind {
        assert!(config.push_district(0, 10, DistrictId { kind, variant: 0 }, 0));
        assert!(config.set_districts_active(true));
    }
    config.set_graffiti(lib);
    config
}

/// Corps proches (ceux qui peuvent porter un graffiti) et corps peints, sur une plage de rangées.
fn painted(config: &SceneryConfig, rows: std::ops::Range<i64>) -> (usize, usize) {
    let (mut bodies, mut painted) = (0, 0);
    for index in rows {
        for part in Row::generate(index, config).parts() {
            if part.tier == Tier::Body {
                bodies += 1;
                painted += usize::from(part.graffiti > 0);
            } else {
                assert_eq!(part.graffiti, 0, "seul le corps porte un graffiti");
            }
        }
    }
    (bodies, painted)
}

fn projectors(config: &SceneryConfig, rows: std::ops::Range<i64>) -> Vec<(i64, Projector)> {
    let mut out = ProjectorList::default();
    let mut all = Vec::new();
    for index in rows {
        row_projectors(index, config, &mut out);
        all.extend(out.as_slice().iter().map(|p| (index, *p)));
    }
    all
}

#[test]
fn sans_bibliotheque_la_ville_est_identique() {
    // Le golden (`tests/scenery_golden.rs`) couvre les positions ; ici : aucun graffiti, aucun projecteur, et poser une
    // bibliothèque vide ne change rien.
    let base = SceneryConfig::default();
    let mut empty = SceneryConfig::default();
    assert!(!empty.set_graffiti(GraffitiLibrary::default()));
    let suburb = config_with(Some(DistrictKind::Suburb), GraffitiLibrary::default());
    let mut out = ProjectorList::default();
    for index in 0..ROWS {
        let row = Row::generate(index, &base);
        assert_eq!(row.parts(), Row::generate(index, &empty).parts());
        assert!(row.parts().iter().all(|p| p.graffiti == 0), "rangée {index}");
        assert_eq!(row.auto_projectors(), &[] as &[Projector]);
        assert!(Row::generate(index, &suburb).parts().iter().all(|p| p.graffiti == 0));
        row_projectors(index, &suburb, &mut out);
        assert!(out.is_empty(), "rangée {index}");
    }
}

#[test]
fn le_motif_mural_occupe_les_bits_27_a_31() {
    let part = Part {
        seed: 1009,
        tint: 5,
        tier: Tier::Crown,
        spot_floor: 255,
        temper: 7,
        ..Part::default()
    };
    let painted = Part { graffiti: 31, ..part };
    assert_eq!(part.tag() >> 27, 0);
    assert_eq!(painted.tag() & 0x07FF_FFFF, part.tag());
    assert_eq!(painted.tag() >> 27, 31);
}

#[test]
fn le_meme_index_donne_les_memes_graffitis() {
    let lib = || library(&[placement(400.0, 1, 4.0, ProjectorSize::Small, "C")]);
    let a = config_with(Some(DistrictKind::Suburb), lib());
    let b = config_with(Some(DistrictKind::Suburb), lib());
    let mut seen = 0;
    for index in 0..500 {
        let (ra, rb) = (Row::generate(index, &a), Row::generate(index, &b));
        assert_eq!(ra.parts(), rb.parts(), "rangée {index}");
        seen += ra.parts().iter().filter(|p| p.graffiti > 0).count();
    }
    assert!(seen > 50, "{seen} graffitis");
    assert_eq!(projectors(&a, 0..500), projectors(&b, 0..500));
}

#[test]
fn la_banlieue_porte_plus_de_graffitis_que_le_centre() {
    let share = |kind| {
        let (bodies, painted) = painted(&config_with(Some(kind), library(&[])), 10..10 + ROWS);
        painted as f64 / bodies as f64
    };
    let (neon, suburb, resi) = (
        share(DistrictKind::Neon),
        share(DistrictKind::Suburb),
        share(DistrictKind::Residential),
    );
    assert!(neon > 0.0 && neon < 0.07, "centre {neon}");
    assert!(suburb > 0.2, "banlieue {suburb}");
    assert!(neon < resi && resi < suburb, "{neon} {resi} {suburb}");
    assert!(suburb > 4.0 * neon);
}

#[test]
fn le_centre_porte_quelques_projecteurs_et_le_residentiel_presque_aucun() {
    let count = |kind| projectors(&config_with(Some(kind), library(&[])), 10..10 + ROWS).len();
    let (neon, suburb, resi) = (
        count(DistrictKind::Neon),
        count(DistrictKind::Suburb),
        count(DistrictKind::Residential),
    );
    // Environ 4 corps proches par rangée : « quelques » = au plus un sur cinq.
    assert!(neon > 200 && neon < (ROWS as usize) * 4 / 5, "centre {neon}");
    assert!(suburb < neon, "banlieue {suburb} ≥ centre {neon}");
    assert!(resi * 5 < neon, "résidentiel {resi}");
}

#[test]
fn la_classe_de_taille_suit_la_hauteur_de_l_immeuble() {
    let mut sizes = [0; 3];
    for kind in [DistrictKind::Neon, DistrictKind::Suburb] {
        for (_, p) in projectors(&config_with(Some(kind), library(&[])), 10..10 + ROWS) {
            assert!(!p.manual);
            let expected = if p.y >= LARGE_MIN_HEIGHT_M {
                ProjectorSize::Large
            } else if p.y >= MEDIUM_MIN_HEIGHT_M {
                ProjectorSize::Medium
            } else {
                ProjectorSize::Small
            };
            assert_eq!(p.size, expected, "toit à {} m", p.y);
            sizes[p.size as usize] += 1;
        }
    }
    assert!(sizes.iter().all(|&n| n > 0), "{sizes:?}");
}

#[test]
fn un_projecteur_manuel_remplace_l_automatique_qui_le_chevauche() {
    let (index, auto) = projectors(&config_with(None, library(&[])), 10..400)
        .into_iter()
        .next()
        .expect("un projecteur automatique");
    let manual = placement(-auto.z, auto.side, 6.0, ProjectorSize::Medium, "E");
    let config = config_with(None, library(&[manual]));
    let mut out = ProjectorList::default();
    row_projectors(index, &config, &mut out);
    let placed = out.as_slice().iter().find(|p| p.manual).expect("le manuel");
    assert!((placed.z - auto.z).abs() < 1e-9 && placed.side == auto.side);
    assert!((placed.y - 6.0).abs() < 1e-6 && placed.size == ProjectorSize::Medium);
    assert!(!out.as_slice().contains(&auto), "l'automatique reste");
    // Aucun automatique restant ne chevauche le manuel, y compris dans les rangées voisines.
    for (_, p) in projectors(&config, index - 2..index + 3) {
        if !p.manual && p.side == placed.side {
            let gap = (p.z - placed.z).abs();
            let reach = f64::from(p.size.bar_len_m() + placed.size.bar_len_m()) / 2.0;
            assert!(gap > reach, "chevauchement à {gap} m");
        }
    }
}

#[test]
fn un_projecteur_manuel_hors_route_est_ignore() {
    let bad = [
        placement(-100.0, 1, 4.0, ProjectorSize::Small, "C"),
        placement(f64::NAN, 1, 4.0, ProjectorSize::Small, "C"),
        placement(300.0, 1, -2.0, ProjectorSize::Small, "C"),
        placement(300.0, 0, 4.0, ProjectorSize::Small, "C"),
        placement(300.0, 1, 4.0, ProjectorSize::Small, "INCONNU"),
        placement(300.0, 1, 4.0, ProjectorSize::Small, "A"),
    ];
    let (lib, warnings) = GraffitiLibrary::new(&words(), &bad);
    assert_eq!(warnings.len(), bad.len(), "{warnings:?}");
    assert_eq!(lib.manual(), &[] as &[Projector]);

    let good = placement(300.0, -1, 4.0, ProjectorSize::Small, "C");
    let config = config_with(None, library(&[good]));
    let manual: Vec<_> = projectors(&config, -20..40)
        .into_iter()
        .filter(|(_, p)| p.manual)
        .collect();
    assert_eq!(manual.len(), 1);
    let (index, p) = manual[0];
    assert_eq!(index, (300.0 / SPACING_M).round() as i64);
    assert!((p.z + 300.0).abs() < 1e-9 && p.side == -1 && p.x < 0.0);
}

#[test]
fn un_projecteur_ne_traverse_pas_un_immeuble() {
    let mut checked = 0;
    for kind in [DistrictKind::Neon, DistrictKind::Suburb] {
        let config = config_with(Some(kind), library(&[]));
        for (_, p) in projectors(&config, 10..10 + ROWS / 2) {
            let (x, y, z) = p.extent();
            for index in Row::rows_near(p.z, f64::from(ProjectorSize::Large.bar_len_m())) {
                for hit in Row::generate(index, &config).hitboxes() {
                    let crosses =
                        x[0] < hit.x[1] && x[1] > hit.x[0] && z[0] < hit.z[1] && z[1] > hit.z[0] && y[0] < hit.top;
                    assert!(!crosses, "{p:?} traverse {hit:?}");
                }
            }
            let letters = DVec3::new(f64::from(p.x), f64::from(f32::midpoint(y[0], y[1])), p.z);
            assert!(!collides(&config, letters, 0.5), "{p:?}");
            checked += 1;
        }
    }
    assert!(checked > 200, "{checked}");
}

#[test]
fn pendant_un_degrade_le_choix_suit_la_probabilite_croissante() {
    let suburb = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 0,
    };
    let mut blend = config_with(None, library(&[]));
    assert!(blend.push_district(0, 100, suburb, 400));
    assert!(blend.set_districts_active(true));
    let neon = config_with(Some(DistrictKind::Neon), library(&[]));
    let mut after = config_with(None, library(&[]));
    assert!(after.push_district(0, 100, suburb, 0));
    assert!(after.set_districts_active(true));

    // Début du dégradé (probabilité 0) : les graffitis de l'ancien quartier, à l'identique.
    for index in 90..=100 {
        assert_eq!(
            Row::generate(index, &blend).parts(),
            Row::generate(index, &neon).parts()
        );
    }
    // Fin : ceux du nouveau.
    for index in 500..520 {
        assert_eq!(
            Row::generate(index, &blend).parts(),
            Row::generate(index, &after).parts()
        );
    }
    // Entre les deux, la part peinte monte par quarts, sans saut.
    let shares: Vec<f64> = (0..4)
        .map(|q| {
            let (bodies, painted) = painted(&blend, 100 + q * 100..200 + q * 100);
            painted as f64 / bodies as f64
        })
        .collect();
    let (b0, p0) = painted(&neon, 100..500);
    let (b1, p1) = painted(&after, 100..500);
    let (start, end) = (p0 as f64 / b0 as f64, p1 as f64 / b1 as f64);
    assert!(shares[0] > start && shares[3] < end, "{start} {shares:?} {end}");
    for w in shares.windows(2) {
        assert!(w[1] > w[0], "non croissant : {shares:?}");
        assert!(w[1] - w[0] < (end - start) / 2.0, "saut : {shares:?}");
    }
}
