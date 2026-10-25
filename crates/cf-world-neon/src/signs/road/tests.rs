//! Tests de la route d'enseignes : placement déterministe, pool borné, fondus, bug des portiques, radar, flux, prêt.

#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::many_single_char_names,
    clippy::cast_possible_wrap,
    reason = "tests : un échec doit faire échouer le test, et les rangs sont petits"
)]

use super::*;
use crate::signs::formats::format_at;
use crate::signs::show_signs::{SignShow, SlotNames};

fn no_bridge(_: f64, _: f64) -> bool {
    false
}

fn frame(car_z: f64, time: f64, show: &SignShow) -> RoadFrame<'_> {
    RoadFrame {
        car_z,
        altitude: 0.0,
        time,
        show,
    }
}

fn drive(road: &mut SignRoad, from: f64, to: f64, step: f64, show: &SignShow, mut each: impl FnMut(&SignRoad, f64)) {
    let mut z = from;
    let mut time = 0.0;
    while z > to {
        road.update(&frame(z, time, show), no_bridge);
        each(road, z);
        z -= step;
        time += step / 60.0;
    }
}

#[test]
fn les_constantes_signees_suivent_le_catalogue() {
    assert_eq!(PRODUCT_I64, crate::signs::ads::PRODUCT_ADS as i64);
}

#[test]
fn le_pool_est_borne_et_aucun_emplacement_n_est_double() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    drive(&mut road, 0.0, -30_000.0, 7.0, &show, |road, _| {
        let mut slots: Vec<i64> = road.units.iter().filter_map(|u| u.slot).collect();
        let n = slots.len();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), n, "deux unités sur le même emplacement");
        assert!(n <= POOL);
    });
}

#[test]
fn un_panneau_a_le_format_de_sa_carte_et_sa_place_ne_change_pas() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    let mut seen = std::collections::HashMap::new();
    drive(&mut road, 0.0, -20_000.0, 5.0, &show, |road, _| {
        for unit in road.units.iter().filter(|u| u.state.is_some()) {
            let (Some(k), Some(id)) = (unit.slot, unit.format) else {
                continue;
            };
            assert_eq!(id, format_at(k), "emplacement {k}");
            let place = (unit.z.to_bits(), unit.layout.x.to_bits(), unit.layout.y.to_bits());
            assert_eq!(*seen.entry(k).or_insert(place), place, "emplacement {k} a bougé");
        }
    });
    assert!(seen.len() > 150, "{} emplacements vus", seen.len());
}

#[test]
fn un_panneau_apparait_en_fondu_devant_et_disparait_derriere() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    let mut max_opacity = std::collections::HashMap::new();
    let mut first_visible = std::collections::HashMap::new();
    drive(&mut road, 0.0, -3_000.0, 2.0, &show, |road, car_z| {
        for unit in road.units.iter().filter(|u| u.state.is_some() && u.visible) {
            let Some(k) = unit.slot else { continue };
            let o = max_opacity.entry(k).or_insert(0.0_f32);
            *o = o.max(unit.opacity);
            first_visible.entry(k).or_insert(car_z - unit.z);
        }
    });
    // Un panneau lointain apparaît presque transparent (fondu), et finit par être pleinement opaque.
    let (k, _) = first_visible
        .iter()
        .find(|(k, _)| **k > 3)
        .unwrap_or_else(|| panic!("aucun panneau"));
    assert!(max_opacity[k] > 0.99, "opacité maximale {}", max_opacity[k]);
    for (k, ahead) in &first_visible {
        let range = format_at(*k).format().range;
        assert!(
            *ahead <= range + 2.0,
            "emplacement {k} visible à {ahead} m, portée {range}"
        );
    }
}

#[test]
fn les_ponts_ecartent_les_panneaux_portes_mais_pas_les_toits_ni_les_projections() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    // Tout est « près d'une arche » : seuls les formats sans pied (ou sur tour) restent.
    let mut z = 0.0;
    for i in 0..400 {
        road.update(&frame(z, f64::from(i) / 10.0, &show), |_, _| true);
        z -= 9.0;
    }
    for unit in road.units.iter().filter(|u| u.state.is_some()) {
        let mount = unit.format.map(|f| f.format().mount);
        assert!(
            matches!(mount, Some(Mount::None | Mount::Tower)),
            "{mount:?} posé sur une arche"
        );
    }
}

#[test]
fn la_face_lisible_regarde_vers_la_route() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    road.update(&frame(-500.0, 0.0, &show), no_bridge);
    let mut checked = 0;
    road.collect_signs(|_, position, facing, width| {
        assert!((facing.length() - 1.0).abs() < 1e-5 && width > 0.0);
        // Un panneau de côté se tourne vers l'axe de la route : sa face a une composante qui pointe vers x = 0.
        if position.x.abs() > 5.0 {
            assert!(
                facing.x * position.x as f32 <= 0.0,
                "face tournée vers l'extérieur : {facing:?} à {position:?}"
            );
            checked += 1;
        }
    });
    assert!(checked > 0);
}

fn gantry_road(phantom_wanted: bool) -> (SignRoad, usize) {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    let mut z = 0.0;
    for _ in 0..4_000 {
        road.update(&frame(z, 0.0, &show), no_bridge);
        let found = road.units.iter().position(|u| {
            u.state.is_some_and(|s| s.phantom == phantom_wanted)
                && matches!(u.format, Some(FormatId::Gantry | FormatId::Arm))
                && (z - 300.0..z - 200.0).contains(&u.z)
        });
        if let Some(i) = found {
            return (road, i);
        }
        z -= 25.0;
    }
    panic!("aucun portique trouvé");
}

#[test]
fn un_portique_fantome_bugue_s_eteint_au_passage_puis_revient() {
    let (mut road, i) = gantry_road(true);
    let show = SignShow::default();
    let z = road.units[i].z;
    let y = f64::from(road.units[i].layout.y);
    let mut time = 100.0;
    let mut saw_bug = false;
    let mut saw_off = false;
    // La voiture vole à la hauteur du panneau et le traverse.
    for step in 0..240 {
        // `ahead` va de +80 m à -40 m.
        let mut f = frame(z + 80.0 - f64::from(step) * 0.5, time, &show);
        f.altitude = y - 2.0;
        road.update(&f, no_bridge);
        let u = &road.units[i];
        let ahead = f.car_z - u.z;
        if ahead < 6.0 && ahead > -14.0 {
            assert!(
                !u.display.on && u.display.passing,
                "à {ahead} m l'écran doit être éteint"
            );
            saw_off = true;
        } else if u.glitching {
            saw_bug = true;
        }
        time += 1.0 / 60.0;
    }
    assert!(saw_off && saw_bug);
    // Loin de la voiture, à la hauteur du panneau ou non : rien ne bugue.
    let mut f = frame(z + 300.0, time, &show);
    f.altitude = y - 2.0;
    road.update(&f, no_bridge);
    assert!(road.units[i].display.on && !road.units[i].glitching);
}

#[test]
fn un_portique_solide_ne_bugue_jamais_et_un_fantome_ne_bugue_pas_hors_de_son_altitude() {
    let (mut road, i) = gantry_road(false);
    let show = SignShow::default();
    let z = road.units[i].z;
    for step in -60..60 {
        let mut f = frame(z + f64::from(step), 50.0, &show);
        f.altitude = 0.0;
        road.update(&f, no_bridge);
        if road.units[i].slot.is_some() {
            assert!(road.units[i].display.on, "portique solide éteint à {step}");
        }
    }
    let (mut road, i) = gantry_road(true);
    let z = road.units[i].z;
    for step in -60..60 {
        let mut f = frame(z + f64::from(step), 50.0, &show);
        // Bien au-dessus du panneau (il est à 11 à 14 m, le caisson fait 7,5 m ou 10 m de haut).
        f.altitude = 60.0;
        road.update(&f, no_bridge);
        if road.units[i].slot.is_some() {
            assert!(road.units[i].display.on, "fantôme éteint hors de son altitude à {step}");
        }
    }
}

#[test]
fn les_obstructions_sont_celles_des_portiques_et_potences_visibles() {
    let (road, i) = gantry_road(false);
    let car_z = road.units[i].z + 150.0;
    let mut out = Vec::new();
    road.obstructions(car_z, &mut out);
    let unit = &road.units[i];
    let y = f64::from(unit.layout.y);
    let half = f64::from(unit.height) / 2.0;
    assert!(
        out.iter()
            .any(|o| (o.lo - (y - half - 0.5)).abs() < 1e-9 && (o.hi - (y + half + 1.6)).abs() < 1e-9)
    );
    assert!(out.iter().all(|o| o.z_near > o.z_far));
    // Un panneau fantôme ne laisse que sa poutre à esquiver.
    let (road, i) = gantry_road(true);
    let mut out = Vec::new();
    road.obstructions(road.units[i].z + 150.0, &mut out);
    let unit = &road.units[i];
    let y = f64::from(unit.layout.y);
    let half = f64::from(unit.height) / 2.0;
    assert!(out.iter().any(|o| (o.lo - (y + half + 0.3)).abs() < 1e-9));
}

#[test]
fn le_radar_part_une_fois_par_passage_et_seule_la_camera_la_plus_proche_a_un_flux() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    let mut radar = std::collections::HashMap::<i64, u32>::new();
    let mut feeds_at_once = 0;
    drive(&mut road, 0.0, -15_000.0, 1.0, &show, |road, _| {
        let mut live = 0;
        for unit in road.units.iter().filter(|u| u.state.is_some()) {
            let Some(s) = unit.state else { continue };
            if s.radar {
                *radar.entry(unit.slot.unwrap_or(-9)).or_default() += 1;
            }
            live += u32::from(s.live);
            if s.live {
                assert!(unit.format.is_some_and(|f| f.format().look == Look::Camera));
            }
        }
        feeds_at_once = feeds_at_once.max(live);
    });
    assert!(radar.len() > 5, "{} radars", radar.len());
    assert!(
        radar.values().all(|n| *n == 1),
        "un radar est parti plusieurs fois : {radar:?}"
    );
    assert_eq!(feeds_at_once, 1, "un seul flux à la fois");
}

#[test]
fn le_prete_ne_bouge_pas_avec_la_route_et_se_rend() {
    let show = SignShow::default();
    let mut road = SignRoad::default();
    let ad = SignAd::Catalogue(CAR_FRONT_AD);
    let Some(i) = road.lease(FormatId::HangCam, ad, 17.0, false, 0.0) else {
        panic!("pas d'unité libre")
    };
    assert!(road.units[i].leased);
    road.units[i].hang = DVec3::new(0.0, 30.0, -250.0);
    for step in 0..400 {
        road.update(&frame(-f64::from(step) * 4.0, f64::from(step) / 15.0, &show), no_bridge);
        assert!(
            road.units[i].leased && road.units[i].slot == Some(-1),
            "la route n'y touche pas"
        );
    }
    road.unlease(i);
    assert!(road.units[i].slot.is_none() && !road.units[i].leased);
    // Il n'y a plus que POOL - 1 unités libres tant qu'une est prêtée.
    for _ in 0..POOL {
        let _ = road.lease(FormatId::HangAd, SignAd::Catalogue(WATCH_AD), 15.0, false, 0.0);
    }
    assert!(
        road.lease(FormatId::HangAd, SignAd::Catalogue(WATCH_AD), 15.0, false, 0.0)
            .is_none()
    );
}

#[test]
fn en_spectacle_au_plus_un_panneau_sur_quatre_et_demi_annonce_le_dj() {
    let show = SignShow {
        active: true,
        dj_on_ads: true,
        slot: 0,
        slots: vec![SlotNames {
            name: "KRMN".into(),
            event: "DÉMESURE".into(),
        }],
    };
    let mut road = SignRoad::default();
    let mut seen_live = 0;
    drive(&mut road, 0.0, -60_000.0, 9.0, &show, |road, _| {
        let placed = road.units.iter().filter(|u| u.state.is_some()).count();
        let live = road
            .units
            .iter()
            .filter(|u| matches!(u.state.map(|s| s.ad), Some(SignAd::Live(_))))
            .count();
        assert!(
            live as f64 * 4.5 <= placed as f64 + 4.5,
            "{live} annonces sur {placed} panneaux"
        );
        seen_live += live;
    });
    assert!(seen_live > 0, "aucune annonce du DJ en 60 km");
    // Hors spectacle, jamais.
    let mut road = SignRoad::default();
    drive(&mut road, 0.0, -20_000.0, 9.0, &SignShow::default(), |road, _| {
        assert!(
            road.units
                .iter()
                .all(|u| !matches!(u.state.map(|s| s.ad), Some(SignAd::Live(_))))
        );
    });
}
