//! Les panneaux vus de l'onglet Debug : le catalogue publié à l'opérateur, et la pose d'un panneau à sa demande.
//!
//! [`publish_catalog`] range les formats par endroit (route, côtés, immeubles) et nomme chaque publicité ; l'interface
//! n'a ainsi aucune connaissance du monde (`cf-render::SignCatalog`). [`apply_orders`] exécute les [`SignOrder`] : le
//! panneau est posé devant la voiture par [`SignRoad::pin`](super::SignRoad::pin), qui ne le recycle pas avant qu'elle
//! l'ait dépassé.

use bevy::prelude::*;
use cf_render::{AbsolutePosition, SignAdInfo, SignCatalog, SignFormatInfo, SignGroup, SignOrder, StreamFocus};

use super::ads::{AdId, AdKind, CATALOGUE, CATALOGUE_LEN, NEON_FIRST, PRODUCT_ADS, WATCH_AD};
use super::formats::{FormatId, Look};
use super::road::RoadFrame;
use super::show_signs::SignShow;
use super::systems::SignRoadRes;

/// Les formats qu'on peut poser, dans l'ordre de la liste : (format, nom, infobulle, endroit). Les deux formats portés
/// par le vaisseau publicitaire (`HangCam`, `HangAd`) n'y sont pas : ils appartiennent à son événement.
const FORMATS: [(FormatId, &str, &str, SignGroup); 8] = [
    (
        FormatId::Gantry,
        "Portique",
        "Le caisson enjambe les voies : on passe dessous.",
        SignGroup::Road,
    ),
    (
        FormatId::Arm,
        "Potence",
        "Un seul pied sur le bas-côté, le caisson déborde au-dessus des voies.",
        SignGroup::Road,
    ),
    (
        FormatId::Cam,
        "Portique-caméra",
        "Il filme la voiture et la montre sur son écran ; le radar prend sa photo au passage.",
        SignGroup::Road,
    ),
    (
        FormatId::Holo,
        "Hologramme",
        "Un hologramme flottant au bord de la route.",
        SignGroup::Side,
    ),
    (
        FormatId::Neon,
        "Néon",
        "Une enseigne en tubes de néon, en drapeau sur un mât.",
        SignGroup::Side,
    ),
    (
        FormatId::Ghost,
        "Projection",
        "Des lettres dans le ciel, sans support ni cadre.",
        SignGroup::Side,
    ),
    (
        FormatId::Roof,
        "Panneau de toit",
        "Très haut, très loin, donc très grand.",
        SignGroup::Roof,
    ),
    (
        FormatId::Jumbo,
        "Écran géant",
        "L'écran géant d'un toit, relié à une caméra.",
        SignGroup::Roof,
    ),
];

/// Le nom d'un visuel de produit.
const fn kind_label(kind: AdKind) -> &'static str {
    match kind {
        AdKind::Food => "Viande",
        AdKind::Fashion => "Mode",
        AdKind::Mind => "Cerveau scanné",
        AdKind::Escape => "Casque VR",
        AdKind::Politics => "Propagande",
        AdKind::Estate => "Immobilier et concert",
        AdKind::Brute => "Matraquage",
        AdKind::Watch => "Écran-caméra",
        AdKind::Neon => "Néon",
    }
}

/// Le nom d'une publicité du catalogue.
fn ad_label(index: usize) -> String {
    match index {
        i if i < PRODUCT_ADS => kind_label(CATALOGUE[i].kind).to_owned(),
        i if i == usize::from(WATCH_AD.0) => "Radar (plan large)".to_owned(),
        i if i == usize::from(WATCH_AD.0) + 1 => "Caméra de face".to_owned(),
        i if i == usize::from(WATCH_AD.0) + 2 => "Caméra de profil".to_owned(),
        i => format!("Néon {}", i - NEON_FIRST + 1),
    }
}

/// Les publicités (rangs) qu'un rendu sait afficher : les produits sur les caissons et hologrammes, les caméras sur les
/// écrans-caméra, les tubes sur le néon (comme le tirage de la route).
fn ads_for(look: Look) -> Vec<u8> {
    let range = match look {
        Look::Camera => usize::from(WATCH_AD.0)..NEON_FIRST,
        Look::Neon => NEON_FIRST..CATALOGUE_LEN,
        Look::Holo | Look::Metal | Look::Ghost => 0..PRODUCT_ADS,
    };
    range.map(|i| i as u8).collect()
}

/// Le catalogue que l'opérateur voit.
#[must_use]
pub fn build_catalog() -> SignCatalog {
    SignCatalog {
        formats: FORMATS
            .iter()
            .map(|&(id, label, hint, group)| SignFormatInfo {
                key: id.name(),
                label,
                hint,
                group,
                ads: ads_for(id.format().look),
            })
            .collect(),
        ads: CATALOGUE
            .iter()
            .enumerate()
            .map(|(i, ad)| SignAdInfo {
                label: ad_label(i),
                slogans: ad.slogans.to_vec(),
            })
            .collect(),
    }
}

/// `Startup` : publie le catalogue.
pub fn publish_catalog(mut catalog: ResMut<SignCatalog>) {
    *catalog = build_catalog();
}

/// `CfSet::Simulate`, avant la route d'enseignes : pose les panneaux demandés.
pub fn apply_orders(
    time: Res<Time>,
    catalog: Res<SignCatalog>,
    sign_show: Res<SignShow>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut orders: MessageReader<SignOrder>,
    mut road: ResMut<SignRoadRes>,
    mut serial: Local<u32>,
) {
    let Ok(car) = car.single() else {
        // Pas de voiture : les ordres n'auraient pas d'endroit où se poser, on les vide.
        orders.clear();
        return;
    };
    for order in orders.read() {
        let Some(key) = catalog.formats.get(usize::from(order.format)).map(|f| f.key) else {
            continue;
        };
        let Some(id) = FormatId::from_name(key) else { continue };
        let frame = RoadFrame {
            car_z: car.0.z,
            eye_z: car.0.z,
            altitude: 0.0,
            time: time.elapsed_secs_f64(),
            show: &sign_show,
        };
        if road.0.pin(id, order.ad.map(AdId), order.slogan, *serial, &frame) {
            *serial = serial.wrapping_add(1);
        } else {
            tracing::warn!("panneau {key} : aucune unité libre (toutes prêtées au vaisseau publicitaire)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_catalogue_couvre_les_formats_et_toutes_les_publicites() {
        let catalog = build_catalog();
        assert_eq!(catalog.formats.len(), FORMATS.len());
        assert_eq!(catalog.ads.len(), CATALOGUE_LEN);
        assert!(
            catalog
                .ads
                .iter()
                .all(|ad| !ad.label.is_empty() && !ad.slogans.is_empty())
        );
        for group in SignGroup::ALL {
            assert!(catalog.formats.iter().any(|f| f.group == group), "{group:?} vide");
        }
        for format in &catalog.formats {
            assert!(FormatId::from_name(format.key).is_some(), "{} inconnu", format.key);
            assert!(!format.ads.is_empty());
            assert!(format.ads.iter().all(|&ad| usize::from(ad) < CATALOGUE_LEN));
        }
    }

    #[test]
    fn chaque_publicite_a_au_moins_un_format_qui_la_montre() {
        let catalog = build_catalog();
        for ad in 0..CATALOGUE_LEN {
            assert!(
                catalog.format_for_ad(ad as u8, None).is_some(),
                "la publicité {ad} ({}) n'a aucun format",
                catalog.ads[ad].label
            );
        }
    }

    #[test]
    fn les_formats_du_vaisseau_publicitaire_ne_sont_pas_proposes() {
        let keys: Vec<_> = build_catalog().formats.iter().map(|f| f.key).collect();
        assert!(!keys.contains(&FormatId::HangCam.name()) && !keys.contains(&FormatId::HangAd.name()));
    }

    #[test]
    fn les_noms_de_publicite_sont_distincts() {
        let catalog = build_catalog();
        let mut labels: Vec<_> = catalog.ads.iter().map(|a| a.label.clone()).collect();
        labels.sort();
        labels.dedup();
        assert_eq!(labels.len(), CATALOGUE_LEN);
    }
}
