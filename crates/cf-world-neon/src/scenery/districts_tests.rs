//! Tests du plan de quartiers et de son effet sur la génération.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use cf_show::district::{DistrictId, DistrictKind};

use super::districts::{DistrictPlan, LEAD_ROWS, district_look, weighted_temper};
use super::layout::Row;
use crate::facade::{TEMPER_COUNT, temper_weights};
use crate::scenery::SceneryConfig;

fn id(kind: DistrictKind, variant: u8) -> DistrictId {
    DistrictId { kind, variant }
}

fn active_config(kind: DistrictKind) -> SceneryConfig {
    let mut config = SceneryConfig::default();
    assert!(config.push_district(0, 10, id(kind, 0), 0));
    assert!(config.set_districts_active(true));
    config
}

#[test]
fn plan_par_defaut_est_neon_partout() {
    let plan = DistrictPlan::default();
    for index in [i64::MIN, -1_000_000, -1, 0, 7, 1_000_000_000, i64::MAX] {
        assert_eq!(plan.district_at(index), id(DistrictKind::Neon, 0));
        assert_eq!(plan.blend_at(index, 3.5), id(DistrictKind::Neon, 0));
    }
    assert_eq!(plan.last_planned_index(), i64::MIN);
    assert!(!SceneryConfig::default().districts_active());
}

#[test]
fn golden_inchange_en_neon() {
    // Le golden (`tests/scenery_golden.rs`) couvre le défaut ; ici, activer les quartiers avec le plan par défaut
    // ne change pas un octet de la ville.
    let base = SceneryConfig::default();
    let mut active = SceneryConfig::default();
    active.set_districts_active(true);
    for index in -200..600 {
        let (a, b) = (Row::generate(index, &base), Row::generate(index, &active));
        assert_eq!(a.parts(), b.parts(), "rangée {index}");
        assert_eq!(a.hitboxes(), b.hitboxes(), "rangée {index}");
        assert_eq!(a.spots(), b.spots(), "rangée {index}");
    }
}

#[test]
fn segments_sont_monotones() {
    let mut plan = DistrictPlan::default();
    assert!(plan.push(0, 10, id(DistrictKind::Suburb, 1), 4));
    // La voiture a avancé : les rangées jusqu'à 20 sont générées, le segment de la banlieue a commencé.
    assert!(plan.push(20, 50, id(DistrictKind::Residential, 2), 4));
    assert_eq!(plan.district_at(9).kind, DistrictKind::Neon);
    assert_eq!(plan.district_at(10).kind, DistrictKind::Suburb);
    assert_eq!(plan.district_at(49).kind, DistrictKind::Suburb);
    assert_eq!(plan.district_at(50), id(DistrictKind::Residential, 2));
    assert_eq!(plan.last_planned_index(), 54);
    // Un segment annoncé plus tôt remplace ceux d'après, qui n'ont encore rien donné.
    assert!(plan.push(20, 30, id(DistrictKind::Neon, 3), 0));
    assert_eq!(plan.district_at(60), id(DistrictKind::Neon, 3));
    assert_eq!(plan.district_at(29).kind, DistrictKind::Suburb);
}

#[test]
fn ne_reecrit_pas_le_passe() {
    let mut config = SceneryConfig::default();
    assert!(config.push_district(100, 100 + LEAD_ROWS, id(DistrictKind::Suburb, 0), 5));
    let before = config.clone();
    assert!(!config.push_district(100, 100 + LEAD_ROWS - 1, id(DistrictKind::Residential, 0), 5));
    assert!(!config.push_district(100, 90, id(DistrictKind::Residential, 0), 5));
    assert!(!config.push_district(100, i64::MIN, id(DistrictKind::Residential, 0), 5));
    assert_eq!(config, before);
    assert!(!config.set_districts_active(false));
}

fn new_share(plan: &DistrictPlan, index: i64, samples: usize) -> f64 {
    let new = (0..samples)
        .filter(|&i| plan.blend_at(index, index as f64 * 13.37 + i as f64 * 71.1).kind == DistrictKind::Suburb)
        .count();
    new as f64 / samples as f64
}

#[test]
fn melange_deterministe() {
    let mut plan = DistrictPlan::default();
    assert!(plan.push(0, 100, id(DistrictKind::Suburb, 0), 10));
    // 10 000 emplacements : 10 rangées de 1 000.
    let shares: Vec<f64> = (100..110).map(|r| new_share(&plan, r, 1000)).collect();
    assert_eq!(
        shares,
        (100..110).map(|r| new_share(&plan, r, 1000)).collect::<Vec<_>>()
    );
    assert!(shares[0] < 0.01, "début : {}", shares[0]);
    assert!(shares[9] > 0.85, "fin : {}", shares[9]);
    for w in shares.windows(2) {
        assert!(w[1] >= w[0] - 0.03, "non monotone : {shares:?}");
    }
    assert!((new_share(&plan, 99, 1000)).abs() < f64::EPSILON);
    assert!((new_share(&plan, 110, 1000) - 1.0).abs() < f64::EPSILON);
}

fn stats(kind: DistrictKind) -> (f64, [f64; TEMPER_COUNT]) {
    let config = active_config(kind);
    let (mut height, mut bodies, mut tempers) = (0.0, 0.0, [0.0; TEMPER_COUNT]);
    for index in 10..510 {
        for part in Row::generate(index, &config).parts() {
            if part.tier == crate::scenery::Tier::Body {
                height += f64::from(part.size[1] - crate::scenery::BASE_DEPTH_M);
                bodies += 1.0;
                tempers[usize::from(part.temper)] += 1.0;
            }
        }
    }
    (height / bodies, tempers.map(|t| t / bodies))
}

#[test]
fn banlieue_et_residentiel_different_du_neon() {
    let (neon, neon_t) = stats(DistrictKind::Neon);
    let (suburb, suburb_t) = stats(DistrictKind::Suburb);
    let (resi, resi_t) = stats(DistrictKind::Residential);
    assert!(resi < suburb && suburb < neon, "{resi} {suburb} {neon}");
    assert!((suburb / neon - 0.7).abs() < 0.1, "banlieue ×{}", suburb / neon);
    assert!((resi / neon - 0.45).abs() < 0.1, "résidentiel ×{}", resi / neon);
    for other in [suburb_t, resi_t] {
        let gap = (0..TEMPER_COUNT)
            .map(|i| (other[i] - neon_t[i]).abs())
            .fold(0.0, f64::max);
        assert!(gap >= 0.10, "écart de tempérament {gap}");
    }
}

#[test]
fn reglages_coherents() {
    for kind in DistrictKind::ALL {
        let sum: f64 = district_look(kind).temper_weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9, "{kind:?} : {sum}");
    }
    let neon = district_look(DistrictKind::Neon);
    assert!((neon.height_scale - 1.0).abs() < f64::EPSILON && neon.skip_chance.abs() < f64::EPSILON);
    assert_eq!(neon.temper_weights, temper_weights());
    assert_eq!(weighted_temper(&neon.temper_weights, 0.999_999_999), 7);
}

#[test]
fn row_reste_sans_vec() {
    // `Row` ne tient que des tableaux de taille fixe : la génération n'alloue pas (voir `tests/districts_no_alloc.rs`).
    fn assert_clone<T: Clone>() {}
    assert_clone::<Row>();
    assert!(size_of::<Row>() > 1000);
}

#[test]
fn les_segments_passes_sont_retires_sans_toucher_les_rangees_vivantes() {
    let mut config = SceneryConfig::default();
    let suburb = id(DistrictKind::Suburb, 1);
    let residential = id(DistrictKind::Residential, 2);
    assert!(config.push_district(0, 10, suburb, 5));
    assert!(config.push_district(40, 50, residential, 6));
    // Le dégradé vers la banlieue (10..15) touche encore la fenêtre : rien à retirer.
    assert!(!config.districts().prunable(14));
    assert!(!config.prune_districts(14));
    let before: Vec<DistrictId> = (15..120).map(|i| config.districts().blend_at(i, 0.3)).collect();
    assert!(config.prune_districts(15));
    let after: Vec<DistrictId> = (15..120).map(|i| config.districts().blend_at(i, 0.3)).collect();
    assert_eq!(before, after, "les rangées vivantes gardent leur quartier");
    assert_eq!(config.districts().district_at(i64::MIN), suburb);
    assert_eq!(config.last_planned_index(), 56);
    // Le dernier segment n'est jamais retiré.
    assert!(config.prune_districts(1_000));
    assert!(!config.prune_districts(10_000));
    assert_eq!(config.districts().district_at(0), residential);
}

#[test]
fn un_forcage_pendant_l_approche_remplace_la_cible_en_attente() {
    let mut config = SceneryConfig::default();
    let a = id(DistrictKind::Suburb, 1);
    let b = id(DistrictKind::Residential, 2);
    // Forçage A : segment à 10 (rangées générées jusqu'à 7).
    assert!(config.push_district(7, 10, a, 4));
    let generated: Vec<DistrictId> = (-20..=9).map(|i| config.districts().blend_at(i, 0.4)).collect();
    // La voiture avance de 2 rangées (générées jusqu'à 9), forçage B à 12 : A n'a rien donné, il disparaît.
    assert!(config.push_district(9, 12, b, 4));
    for row in 10..12 {
        assert_eq!(
            config.districts().district_at(row).kind,
            DistrictKind::Neon,
            "rangée {row}"
        );
    }
    for row in 12..40 {
        assert_ne!(
            config.districts().blend_at(row, 0.4),
            a,
            "rangée {row} : l'ancienne cible reste"
        );
    }
    assert_eq!(config.districts().district_at(30), b);
    let after: Vec<DistrictId> = (-20..=9).map(|i| config.districts().blend_at(i, 0.4)).collect();
    assert_eq!(generated, after, "les rangées générées ne bougent pas");

    // Un segment qui a déjà donné des rangées reste.
    let c = id(DistrictKind::Neon, 4);
    assert!(config.push_district(20, 30, c, 4));
    assert_eq!(config.districts().district_at(20), b);
    assert_eq!(config.districts().district_at(30), c);
}

#[test]
fn quartier_vide_ne_genere_ni_immeuble_ni_projecteur() {
    let mut config = SceneryConfig::default();
    let full: usize = (-40..40).map(|i| Row::generate(i, &config).parts().len()).sum();
    assert!(full > 0, "la ville normale a des immeubles");
    assert!(
        config.set_empty(true),
        "le changement est signalé (pour re-tirer les tronçons)"
    );
    assert!(!config.set_empty(true), "rien ne change la seconde fois");
    for index in -40..40 {
        let row = Row::generate(index, &config);
        assert!(row.parts().is_empty(), "rangée {index}");
        assert!(row.spots().is_empty(), "rangée {index}");
    }
    assert!(config.set_empty(false));
    let back: usize = (-40..40).map(|i| Row::generate(i, &config).parts().len()).sum();
    assert_eq!(back, full, "rendre la ville rend exactement la même");
}
