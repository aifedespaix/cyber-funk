//! Quelle cellule et quel bandeau de l'atlas montre un panneau posé : une fonction pure de son format, de sa publicité et de
//! son slogan. Rien n'est cuit ici (tout l'a été à l'ouverture) ; si une clé manque, le panneau reste vide.

use super::ads::AdKind;
use super::atlas::{CellKey, StripAd, StripKey};
use super::formats::Look;
use super::road::SignAd;
use super::show_signs::{LiveRole, SignShow};

/// La cellule d'un panneau de rendu `look`, publicité `ad`, slogan `slogan`.
#[must_use]
pub fn cell_key(look: Look, ad: &SignAd, slogan: u8, show: &SignShow) -> Option<CellKey> {
    let slot = |live: &super::show_signs::LiveAd| u8::try_from(live.slot).ok();
    match (look, ad) {
        (Look::Holo | Look::Metal, SignAd::Catalogue(id)) if is_product(id.ad().kind) => {
            Some(CellKey::Product { ad: *id, slogan })
        }
        (Look::Holo | Look::Metal, SignAd::Live(live)) if live.role == LiveRole::Poster => {
            Some(CellKey::LivePoster { slot: slot(live)? })
        }
        (Look::Ghost, SignAd::Catalogue(id)) if is_product(id.ad().kind) => Some(CellKey::Ghost { ad: *id, slogan }),
        (Look::Ghost, SignAd::Live(live)) if live.role == LiveRole::Text && live.ghost => {
            // Avec un nom d'événement, le texte ne dépend pas du mot d'accompagnement : une seule cellule.
            let names = show.slots.get(live.slot)?;
            let tag = if names.event.is_empty() { live.tag as u8 } else { 0 };
            Some(CellKey::LiveGhost { slot: slot(live)?, tag })
        }
        (Look::Neon, SignAd::Catalogue(id)) if id.ad().kind == AdKind::Neon => Some(CellKey::Neon { ad: *id, slogan }),
        (Look::Neon, SignAd::Live(live)) if live.role == LiveRole::Text && !live.ghost => Some(CellKey::LiveNeon {
            slot: slot(live)?,
            tag: live.tag as u8,
        }),
        (Look::Camera, SignAd::Catalogue(id)) if id.ad().kind == AdKind::Watch => Some(CellKey::Cam),
        (Look::Camera, SignAd::Live(live)) if live.role == LiveRole::Screen => {
            Some(CellKey::LiveScreen { slot: slot(live)? })
        }
        _ => None,
    }
}

/// Le bandeau d'un panneau (hologramme, caisson, caméra) ; les autres rendus n'en ont pas.
#[must_use]
pub fn strip_key(look: Look, ad: &SignAd, slogan: u8) -> Option<StripKey> {
    if !matches!(look, Look::Holo | Look::Metal | Look::Camera) {
        return None;
    }
    let ad = match ad {
        SignAd::Catalogue(id) => StripAd::Catalogue(*id),
        SignAd::Live(live) => {
            let slot = u8::try_from(live.slot).ok()?;
            match live.role {
                LiveRole::Poster => StripAd::LivePoster { slot },
                LiveRole::Screen => StripAd::LiveScreen { slot },
                LiveRole::Text => return None,
            }
        }
    };
    let start = match ad {
        StripAd::LivePoster { .. } => 1,
        _ => slogan,
    };
    Some(StripKey { ad, start })
}

const fn is_product(kind: AdKind) -> bool {
    !matches!(kind, AdKind::Watch | AdKind::Neon)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signs::atlas::BakedAtlas;
    use crate::signs::road::{RoadFrame, SignRoad};
    use crate::signs::show_signs::{SignShow, SlotNames};

    /// Tout ce que la route peut poser a sa cellule (et son bandeau) dans l'atlas cuit à l'ouverture : rien à cuire
    /// pendant le show (règle 8). 60 km de route, spectacle actif, trois créneaux.
    #[test]
    fn tout_ce_que_la_route_pose_est_deja_cuit() {
        let slots = vec![
            SlotNames {
                name: "KRMN".into(),
                event: "DÉMESURE".into(),
            },
            SlotNames {
                name: "NOVA KAITO B2B SUZU".into(),
                event: String::new(),
            },
            SlotNames {
                name: "TROIS".into(),
                event: "DÉMESURE".into(),
            },
        ];
        let atlas = BakedAtlas::build(
            &slots,
            std::thread::available_parallelism().map_or(2, std::num::NonZero::get),
        );
        let mut show = SignShow {
            active: true,
            dj_on_ads: true,
            slot: 0,
            slots,
        };
        let mut road = SignRoad::default();
        let mut z = 0.0_f64;
        let (mut placed, mut live) = (0, 0);
        let mut seen = std::collections::HashSet::new();
        while z > -60_000.0 {
            show.slot = ((-z / 9_000.0) as usize) % 3;
            road.update(
                &RoadFrame {
                    car_z: z,
                    altitude: 0.0,
                    time: -z / 60.0,
                    show: &show,
                },
                |_, _| false,
            );
            for unit in road.units.iter().filter(|u| u.state.is_some() && u.fresh) {
                let (Some(state), Some(format)) = (unit.state, unit.format) else {
                    continue;
                };
                if !seen.insert((unit.slot, state.slogan)) {
                    continue;
                }
                let look = format.format().look;
                let cell = cell_key(look, &state.ad, state.slogan, &show);
                assert!(
                    cell.is_some_and(|k| atlas.cells.contains_key(&k)),
                    "{format:?} {:?} : cellule {cell:?}",
                    state.ad
                );
                if matches!(look, Look::Holo | Look::Metal | Look::Camera) {
                    let strip = strip_key(look, &state.ad, state.slogan);
                    assert!(
                        strip.is_some_and(|k| atlas.strips.contains_key(&k)),
                        "{format:?} : bandeau {strip:?}"
                    );
                }
                placed += 1;
                live += usize::from(matches!(state.ad, SignAd::Live(_)));
            }
            for unit in &mut road.units {
                unit.fresh = false;
            }
            z -= 6.0;
        }
        assert!(
            placed > 400 && live > 10,
            "{placed} panneaux, dont {live} annonces du DJ"
        );
    }
}
