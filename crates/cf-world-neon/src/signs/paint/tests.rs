//! Tests de la cuisson : chaque genre de cellule dessine quelque chose, dans les bons canaux ; les bandeaux bouclent.
//!
//! Avec `SIGNS_DUMP=<dossier>`, les cellules sont aussi écrites en `.ppm` (aperçu : accent rose, glace blanche,
//! couleur propre menthe) pour être relues à l'œil.

use super::*;
use crate::signs::ads::{CATALOGUE, PRODUCT_ADS};
use crate::signs::icons::{IconId, icon_for};

fn painter() -> Painter {
    Painter::new().unwrap_or_else(|| panic!("canevas"))
}

fn channel(bitmap: &Bitmap, c: usize) -> Vec<u8> {
    match &bitmap.pixels {
        Pixels::Rgba(d) => d.chunks_exact(4).map(|p| p[c]).collect(),
        Pixels::R8(d) => d.clone(),
    }
}

fn ink_sum(bitmap: &Bitmap, c: usize) -> u64 {
    channel(bitmap, c).iter().map(|v| u64::from(*v)).sum()
}

fn dump(name: &str, bitmap: &Bitmap) {
    let Ok(dir) = std::env::var("SIGNS_DUMP") else { return };
    let mut out = format!("P6\n{} {}\n255\n", bitmap.w, bitmap.h).into_bytes();
    for i in 0..(bitmap.w * bitmap.h) as usize {
        let (r, g, b, k) = match &bitmap.pixels {
            Pixels::Rgba(d) => (d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]),
            Pixels::R8(d) => (d[i], 0, 0, 0),
        };
        let mix = |base: f32, a: u8, col: [f32; 3], c: usize| base + (col[c] - base) * f32::from(a) / 255.0;
        let mut px = [0.04 + 0.12 * f32::from(k) / 255.0; 3];
        for c in 0..3 {
            px[c] = mix(px[c], r, [1.0, 0.25, 0.45], c);
            px[c] = mix(px[c], g, [0.9, 0.95, 1.0], c);
            px[c] = mix(px[c], b, [0.3, 1.0, 0.8], c);
        }
        out.extend(px.iter().map(|v| (v * 255.0) as u8));
    }
    let _ = std::fs::write(format!("{dir}/{name}.ppm"), out);
}

#[test]
fn chaque_produit_dessine_dans_ses_canaux() {
    let mut p = painter();
    for (index, ad) in CATALOGUE[..PRODUCT_ADS].iter().enumerate() {
        for rank in [0, 1] {
            let slogan = ad.slogans[rank];
            let icon = icon_for(ad.kind, slogan);
            let bitmap = p.product(&Draw {
                ad,
                slogan,
                rank,
                icon,
                live: None,
            });
            assert!(ink_sum(&bitmap, 0) + ink_sum(&bitmap, 1) > 10_000, "{:?} rang {rank} : cellule vide", ad.kind);
            dump(&format!("product-{index}-{rank}"), &bitmap);
        }
    }
}

#[test]
fn les_icones_sont_toutes_dessinees() {
    let mut p = painter();
    let ad = &CATALOGUE[0];
    for icon in [
        IconId::Cage,
        IconId::Tag,
        IconId::Needle,
        IconId::Lock,
        IconId::Chip,
        IconId::Hourglass,
        IconId::Pad,
        IconId::Pause,
        IconId::Power,
        IconId::Ballot,
        IconId::Chart,
        IconId::Liar,
        IconId::House,
        IconId::Spot,
    ] {
        let bitmap = p.product(&Draw {
            ad,
            slogan: "X",
            rank: 0,
            icon: Some(icon),
            live: None,
        });
        // La moitié droite de la cellule, là où l'icône est posée.
        let right: u64 = channel(&bitmap, 0)
            .chunks(CANVAS_W as usize)
            .flat_map(|row| row[CANVAS_W as usize / 2..].iter())
            .map(|v| u64::from(*v))
            .sum();
        assert!(right > 5_000, "{icon:?} : rien de dessiné");
        dump(&format!("icon-{}", icon.name()), &bitmap);
    }
}

#[test]
fn le_neon_la_projection_et_la_camera_ont_leur_contenu() {
    let mut p = painter();
    let neon = p.neon("MOTEL ∞", 1.7);
    assert!(ink_sum(&neon, 0) > 20_000 && ink_sum(&neon, 1) > 2_000);
    dump("neon", &neon);
    let ghost = p.ghost("TOUT SE VEND");
    assert!(matches!(ghost.pixels, Pixels::R8(_)) && ink_sum(&ghost, 0) > 20_000);
    dump("ghost", &ghost);
    let cam = p.cam(None);
    assert!(ink_sum(&cam, 0) > 1_000 && ink_sum(&cam, 1) > 1_000);
    let screen = p.cam(Some(("KRMN", "DÉMESURE LIVE")));
    assert!(ink_sum(&screen, 3) > 100_000, "la plaque du DJ est un voile sombre (canal K)");
    dump("cam", &cam);
    dump("screen", &screen);
    // La rangée de chiffres et les étiquettes sont dans la bande sous l'image, vide pour les autres cellules.
    let band_start = (BAND_Y as usize) * CANVAS_W as usize;
    assert!(channel(&cam, 0)[band_start..].iter().any(|v| *v > 0));
    let product = p.product(&Draw {
        ad: &CATALOGUE[0],
        slogan: CATALOGUE[0].slogans[0],
        rank: 0,
        icon: None,
        live: None,
    });
    assert!(channel(&product, 0)[band_start..].iter().all(|v| *v == 0), "la bande est réservée aux caméras");
}

#[test]
fn un_bandeau_est_une_boucle_exacte() {
    let mut p = painter();
    let ad = &CATALOGUE[0];
    let slogans: Vec<String> = ad.slogans.iter().map(|s| (*s).to_owned()).collect();
    let strip = p.strip(&slogans).unwrap_or_else(|| panic!("bande"));
    assert!(strip.period > 600.0 && strip.period <= STRIP_MAX_W + 400.0, "{}", strip.period);
    assert_eq!(strip.bitmap.h, TICKER_H);
    let Pixels::R8(data) = &strip.bitmap.pixels else { panic!("un canal") };
    // L'intervalle final est vide : rien ne chevauche la couture de la boucle.
    let w = strip.bitmap.w as usize;
    let seam = strip.period as usize;
    for row in 0..TICKER_H as usize {
        assert!(data[row * w + seam - 3..row * w + w].iter().all(|v| *v == 0), "couture sale, ligne {row}");
    }
    dump("strip", &strip.bitmap);
}
