//! Les données de référence de Sillage (`tests/golden/signs.json`, produites par
//! `tools/sillage-golden/signs/index.ts`) et les tests qui y comparent le portage : jeu de cartes, slogans, annonces
//! du DJ, icônes, catalogue.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_wrap,
    reason = "tests : un fichier de référence illisible doit faire échouer le test, et les rangs sont petits"
)]

use std::sync::OnceLock;

use serde_json::Value;

use super::ads::{AdId, CATALOGUE, CATALOGUE_LEN};
use super::formats::{FormatId, format_at};
use super::icons::icon_for;
use super::show_signs::{LiveRole, SignShow, SlotNames, is_short, live_ad, live_allowed, slogan_index, tidy};

fn golden() -> &'static Value {
    static GOLDEN: OnceLock<Value> = OnceLock::new();
    GOLDEN.get_or_init(|| {
        serde_json::from_str(include_str!("../../tests/golden/signs.json"))
            .unwrap_or_else(|e| panic!("signs.json : {e}"))
    })
}

fn rows(key: &str) -> &'static Vec<Value> {
    golden()[key].as_array().unwrap_or_else(|| panic!("{key} absent"))
}

fn number(v: &Value) -> i64 {
    v.as_i64().unwrap_or_else(|| panic!("entier attendu : {v}"))
}

fn format_of(name: &str) -> FormatId {
    FormatId::from_name(name).unwrap_or_else(|| panic!("format inconnu {name}"))
}

/// Les formats de la table, dans l'ordre où le script les a énumérés.
fn formats_in_order() -> [FormatId; 10] {
    use FormatId::{Arm, Cam, Gantry, Ghost, HangAd, HangCam, Holo, Jumbo, Neon, Roof};
    [Holo, Gantry, Arm, Roof, Jumbo, Ghost, Neon, Cam, HangCam, HangAd]
}

#[test]
fn le_jeu_de_cartes_est_celui_de_sillage() {
    let formats = rows("formats");
    assert_eq!(formats.len(), 1216);
    for (i, expected) in formats.iter().enumerate() {
        let k = i as i64 - 16;
        let name = expected.as_str().unwrap_or_default();
        assert_eq!(format_at(k), format_of(name), "emplacement {k}");
    }
}

#[test]
fn le_catalogue_correspond_a_celui_de_sillage() {
    let ads = rows("ads");
    assert_eq!(ads.len(), CATALOGUE_LEN);
    for (i, row) in ads.iter().enumerate() {
        let ad = AdId(i as u8).ad();
        assert_eq!(row["accent"].as_str(), Some(ad.accent.name()), "pub {i}");
        assert_eq!(row["slogans"].as_u64(), Some(ad.slogans.len() as u64), "pub {i}");
        assert_eq!(row["kind"].as_str(), Some(ad.kind.name()), "pub {i}");
    }
}

#[test]
fn les_slogans_de_depart_sont_ceux_de_sillage() {
    let formats = formats_in_order();
    for row in rows("slogans") {
        let r = row.as_array().unwrap_or_else(|| panic!("ligne"));
        let (ad, format, k) = (number(&r[0]) as usize, formats[number(&r[1]) as usize], number(&r[2]));
        let ad = &CATALOGUE[ad];
        assert_eq!(
            slogan_index(ad, k, format, false) as i64,
            number(&r[3]),
            "sans spectacle {r:?}"
        );
        assert_eq!(
            slogan_index(ad, k, format, true) as i64,
            number(&r[4]),
            "en spectacle {r:?}"
        );
    }
}

fn show_for(index: i64) -> SignShow {
    let (dj, event, dj_on_ads, active) = match index {
        0 => ("KRMN", "Démesure Live", true, true),
        1 => ("  ", "", true, true),
        2 => ("KRMN", "X", false, true),
        _ => ("KRMN", "X", true, false),
    };
    SignShow {
        active,
        dj_on_ads,
        slot: 0,
        slots: SignShow::names(event, [dj]),
    }
}

#[test]
fn les_annonces_du_dj_sont_celles_de_sillage() {
    let formats = formats_in_order();
    for row in rows("allowed") {
        let r = row.as_array().unwrap_or_else(|| panic!("ligne"));
        let show = show_for(number(&r[0]));
        let got = live_allowed(
            number(&r[2]),
            formats[number(&r[1]) as usize],
            &show,
            number(&r[3]) as usize,
            number(&r[4]) as usize,
        );
        assert_eq!(i64::from(got), number(&r[5]), "{r:?}");
    }
}

#[test]
fn le_panneau_du_dj_est_celui_de_sillage() {
    let show = SignShow {
        active: true,
        dj_on_ads: true,
        slot: 0,
        slots: SignShow::names("démesure live", ["  krmn   le  très long nom de dj qui dépasse"]),
    };
    let names: &SlotNames = &show.slots[0];
    for row in rows("lives") {
        let format = format_of(row["format"].as_str().unwrap_or_default());
        let k = number(&row["k"]);
        let Some(ad) = live_ad(k, format, &show) else {
            // Sillage donne alors une affiche par défaut ; ces formats ne portent jamais le nom du DJ ici.
            assert!(
                matches!(format, FormatId::Cam | FormatId::HangCam | FormatId::HangAd),
                "{row}"
            );
            continue;
        };
        assert_eq!(row["kind"].as_str(), Some(ad.kind.name()), "{row}");
        assert_eq!(row["accent"].as_str(), Some(ad.accent.name()), "{row}");
        let role = match ad.role {
            LiveRole::Poster => "poster",
            LiveRole::Screen => "screen",
            LiveRole::Text => "text",
        };
        assert_eq!(row["role"].as_str(), Some(role), "{row}");
        assert_eq!(row["name"].as_str(), Some(names.name.as_str()));
        assert_eq!(row["event"].as_str(), Some(names.event.as_str()));
        let expected: Vec<&str> = row["slogans"]
            .as_array()
            .map_or_else(Vec::new, |a| a.iter().filter_map(Value::as_str).collect());
        assert_eq!(expected.len(), ad.slogan_count(), "{row}");
        for (i, text) in expected.iter().enumerate() {
            assert_eq!(&ad.slogan(i, names), text, "{row}");
        }
    }
}

#[test]
fn les_icones_et_les_slogans_courts_sont_ceux_de_sillage() {
    for row in rows("icons") {
        let ad = &CATALOGUE[number(&row["ad"]) as usize];
        let text = ad.slogans[number(&row["slogan"]) as usize];
        assert_eq!(
            icon_for(ad.kind, text).map(super::icons::IconId::name),
            row["icon"].as_str(),
            "{row}"
        );
        assert_eq!(is_short(text), row["short"].as_bool().unwrap_or_default(), "{text}");
    }
}

#[test]
fn les_noms_sont_ranges_comme_sillage() {
    assert_eq!(tidy("  krmn   dj "), "KRMN DJ");
    let long = tidy("un nom de dj vraiment beaucoup trop long");
    assert_eq!(long.chars().count(), 24);
    assert!(long.ends_with('…'));
}
