//! Tests de l'atlas : tout ce qu'une enseigne peut demander existe, la cuisson tient dans le budget, et son résultat ne
//! dépend pas du nombre de fils.

use std::time::Instant;

use super::*;
use crate::signs::ads::{CATALOGUE, NEON_FIRST, PRODUCT_ADS};
use crate::signs::show_signs::{NEON_TAGS, SignShow};

fn slots() -> Vec<crate::signs::show_signs::SlotNames> {
    SignShow::names("Démesure Live", ["KRMN", "Nova Kaito b2b Suzu", "  "])
}

#[test]
fn tout_ce_que_la_route_peut_demander_est_cuit() {
    let slots = slots();
    let atlas = BakedAtlas::build(&slots, 4);
    assert_eq!(atlas.failed, 0);
    for (index, ad) in CATALOGUE.iter().enumerate() {
        let id = AdId(index as u8);
        for slogan in 0..ad.slogans.len() as u8 {
            if index < PRODUCT_ADS {
                assert!(
                    atlas.cells.contains_key(&CellKey::Product { ad: id, slogan }),
                    "produit {index}/{slogan}"
                );
                assert!(
                    atlas.cells.contains_key(&CellKey::Ghost { ad: id, slogan }),
                    "projection {index}/{slogan}"
                );
            }
            if index >= NEON_FIRST {
                assert!(
                    atlas.cells.contains_key(&CellKey::Neon { ad: id, slogan }),
                    "néon {index}/{slogan}"
                );
            } else {
                let key = StripKey {
                    ad: StripAd::Catalogue(id),
                    start: slogan,
                };
                assert!(atlas.strips.contains_key(&key), "bandeau {index}/{slogan}");
            }
        }
    }
    assert!(atlas.cells.contains_key(&CellKey::Cam));
    // Deux créneaux nommés ont leurs panneaux ; le troisième, sans nom, n'en a pas.
    for slot in 0..2_u8 {
        assert!(atlas.cells.contains_key(&CellKey::LivePoster { slot }));
        assert!(atlas.cells.contains_key(&CellKey::LiveScreen { slot }));
        for tag in 0..NEON_TAGS.len() as u8 {
            assert!(atlas.cells.contains_key(&CellKey::LiveNeon { slot, tag }));
        }
        assert!(atlas.cells.contains_key(&CellKey::LiveGhost { slot, tag: 0 }));
        assert!(
            !atlas.cells.contains_key(&CellKey::LiveGhost { slot, tag: 1 }),
            "l'événement a un nom : un seul texte"
        );
        assert!(atlas.strips.contains_key(&StripKey {
            ad: StripAd::LivePoster { slot },
            start: 1
        }));
    }
    assert!(!atlas.cells.contains_key(&CellKey::LivePoster { slot: 2 }));
}

#[test]
fn chaque_cellule_et_chaque_bande_tient_dans_sa_page() {
    let atlas = BakedAtlas::build(&slots(), 4);
    for (key, info) in &atlas.cells {
        let page = &atlas.pages[usize::from(info.page)];
        assert!(
            u32::from(info.x) + CANVAS_W <= page.w && u32::from(info.y) + CANVAS_H <= page.h,
            "{key:?}"
        );
        assert!(page.w <= PAGE_W && page.h <= PAGE_MAX_H);
    }
    for (key, info) in &atlas.strips {
        let page = &atlas.pages[usize::from(info.page)];
        assert!(
            u32::from(info.width) <= page.w && u32::from(info.y) + TICKER_H <= page.h,
            "{key:?}"
        );
        assert!(info.period > 1.0 && info.period <= f32::from(info.width), "{key:?}");
    }
    assert!(atlas.pages.iter().all(|p| p.w <= 4096 && p.h <= 4096));
}

#[test]
fn le_resultat_ne_depend_pas_du_nombre_de_fils() {
    let slots = slots();
    let (a, b) = (BakedAtlas::build(&slots, 1), BakedAtlas::build(&slots, 3));
    assert_eq!(a.cells, b.cells);
    assert_eq!(a.pages.len(), b.pages.len());
    for (p, q) in a.pages.iter().zip(&b.pages) {
        assert!(p.data == q.data, "pages différentes");
    }
}

/// Critère de J7-01 : toutes les textures en moins de 2 s au chargement. Mesuré sur la cuisson complète, créneaux du
/// show compris. En test (profil de développement) la marge est plus large ; `cargo test --release` tient les 2 s.
#[test]
fn la_cuisson_tient_dans_le_budget() {
    let slots = SignShow::names(
        "Démesure Live",
        ["KRMN", "Nova Kaito", "DJ Trois", "Quatre", "Cinq", "Six"],
    );
    let threads = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let start = Instant::now();
    let atlas = BakedAtlas::build(&slots, threads);
    let elapsed = start.elapsed().as_secs_f32();
    eprintln!(
        "atlas : {} cellules, {} bandeaux, {} pages, {:.1} Mo, {elapsed:.2} s sur {threads} fils",
        atlas.cells.len(),
        atlas.strips.len(),
        atlas.pages.len(),
        atlas.bytes() as f64 / 1e6,
    );
    let budget = if cfg!(debug_assertions) { 10.0 } else { 2.0 };
    assert!(elapsed < budget, "{elapsed:.2} s ≥ {budget} s");
}
