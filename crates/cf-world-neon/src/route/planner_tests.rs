//! Tests du plan de route : scénarios à la main, puis comparaison avec les fichiers d'origine de Sillage.

use serde_json::Value;

use super::*;

fn input() -> RouteInput {
    RouteInput {
        car_x: 0.0,
        car_z: -1000.0,
        speed: 40.0,
        floor: 0,
        physical: None,
        delay: 0.0,
        can_change: false,
        lookahead: LOOKAHEAD_PLAN,
    }
}

fn bonus(x: f64, ahead: f64) -> PickupView {
    PickupView {
        kind: PickupKind::Bonus,
        x,
        z: -1000.0 - ahead,
        y: 0.0,
    }
}

fn malus(x: f64, ahead: f64) -> PickupView {
    PickupView {
        kind: PickupKind::Malus,
        ..bonus(x, ahead)
    }
}

#[test]
fn rien_a_viser_sans_objet() {
    let mut planner = RoutePlanner::default();
    let route = planner.plan(&[], &input());
    assert_eq!(route.count, 0);
    assert_eq!(route.dodge_count, 0);
    assert_eq!(route_floor(route, 0), None);
}

#[test]
fn un_bonus_a_portee_est_vise() {
    let mut planner = RoutePlanner::default();
    let route = *planner.plan(&[bonus(2.0, 60.0)], &input());
    assert_eq!(route.count, 1);
    assert!((route.aim[0] - 2.0).abs() < 1e-9);
    // 60 m à 40 m/s × 0,85 de défilement : 1,76 s.
    assert!((route.time[0] - 60.0 / 34.0).abs() < 1e-9, "{}", route.time[0]);
}

#[test]
fn un_malus_a_fuir_avant_le_bonus_est_signale() {
    let mut planner = RoutePlanner::default();
    // Un malus sur l'axe de la voiture, 30 m devant, puis un bonus à 3 m sur la droite, 80 m devant.
    let route = *planner.plan(&[malus(0.0, 30.0), bonus(3.0, 80.0)], &input());
    assert_eq!(route.count, 1);
    assert!((route.aim[0] - 3.0).abs() < 1e-9);
    assert_eq!(route.dodge_count, 1);
    assert!(route.dodge_x[0].abs() < 1e-9);
    // On en est quitte une fois sa zone de contact dépassée : 30 m à 34 m/s, plus la demi-zone.
    assert!(route.dodge_time[0] > 30.0 / 34.0, "{}", route.dodge_time[0]);
}

#[test]
fn un_malus_loin_de_la_ligne_ne_change_rien() {
    let mut planner = RoutePlanner::default();
    let alone = *planner.plan(&[bonus(2.0, 60.0)], &input());
    planner.invalidate();
    let with_malus = *planner.plan(&[bonus(2.0, 60.0), malus(-4.0, 60.0)], &input());
    assert_eq!(alone.count, with_malus.count);
    assert!((alone.aim[0] - with_malus.aim[0]).abs() < 1e-9);
    assert_eq!(with_malus.dodge_count, 0);
}

#[test]
fn un_bonus_hors_de_l_etage_est_ignore_sans_changement_possible() {
    let mut planner = RoutePlanner::default();
    let in_flight = PickupView {
        y: 4.4,
        ..bonus(0.0, 100.0)
    };
    assert_eq!(planner.plan(&[in_flight], &input()).count, 0);
    planner.invalidate();
    let free = RouteInput {
        can_change: true,
        ..input()
    };
    let route = *planner.plan(&[in_flight], &free);
    assert_eq!(route.count, 1);
    assert_eq!(route.floor[0], 1);
}

#[test]
fn le_cache_sert_la_meme_image_et_tombe_a_l_invalidation() {
    let mut planner = RoutePlanner::default();
    let first = *planner.plan(&[bonus(2.0, 60.0)], &input());
    // Mêmes entrées, objets différents : le plan en cache est resservi (comme le volant et la verticale).
    let cached = *planner.plan(&[], &input());
    assert_eq!(first, cached);
    planner.invalidate();
    assert_eq!(planner.plan(&[], &input()).count, 0);
}

#[test]
fn le_taux_de_commande_a_un_plancher() {
    let mut hint = PilotHint::default();
    assert!((hint.ctrl_rate() - FULL_RATE * 0.88).abs() < 1e-12);
    hint.rate = 0.0;
    assert!((hint.ctrl_rate() - 0.8 * 0.88).abs() < 1e-12);
}

#[test]
fn les_etages_se_lisent_et_se_chronometrent() {
    assert_eq!(floor_of(0.0), 0);
    assert_eq!(floor_of(4.4), 1);
    assert_eq!(floor_of(33.0), -1);
    assert_eq!(floor_at(0.5), 0);
    assert_eq!(floor_at(20.0), 4);
    assert!((floor_time(0, 2) - (2.2 + 0.9)).abs() < 1e-12);
    assert!((floor_time(3, 1) - (0.9 + 0.55)).abs() < 1e-12);
    assert!(floor_time(2, 2).abs() < 1e-12);
}

// --- Comparaison avec Sillage ------------------------------------------------------------------------------------

/// Tolérance sur les durées et abscisses : les mêmes formules en `f64`, à l'ulp de `exp` près.
const TOLERANCE: f64 = 1e-9;

fn golden() -> Value {
    serde_json::from_str(include_str!("../../tests/golden/route.json")).expect("route.json valide")
}

fn num(v: &Value) -> f64 {
    v.as_f64().expect("nombre")
}

fn nums(v: &Value) -> Vec<f64> {
    v.as_array().expect("tableau").iter().map(num).collect()
}

fn close(a: f64, b: f64, what: &str, case: usize) {
    assert!((a - b).abs() <= TOLERANCE, "cas {case} : {what} = {a}, Sillage {b}");
}

#[test]
fn les_etages_comme_sillage() {
    let g = golden();
    for pair in g["floors"]["floorOf"].as_array().expect("liste") {
        assert_eq!(i64::from(floor_of(num(&pair[0]))), pair[1].as_i64().expect("entier"), "floorOf({pair})");
    }
    for pair in g["floors"]["floorAt"].as_array().expect("liste") {
        assert_eq!(i64::from(floor_at(num(&pair[0]))), pair[1].as_i64().expect("entier"), "floorAt({pair})");
    }
    for triple in g["floors"]["floorTime"].as_array().expect("liste") {
        let (from, to) = (triple[0].as_i64().expect("entier"), triple[1].as_i64().expect("entier"));
        close(floor_time(from as i32, to as i32), num(&triple[2]), "floorTime", 0);
    }
}

#[test]
fn les_plans_de_route_comme_sillage() {
    let g = golden();
    let cases = g["routes"].as_array().expect("liste");
    assert!(cases.len() >= 200);
    let mut planner = RoutePlanner::default();
    let (mut with_route, mut with_dodge) = (0, 0);
    for (n, case) in cases.iter().enumerate() {
        let i = &case["input"];
        let input = RouteInput {
            car_x: num(&i["carX"]),
            car_z: num(&i["carZ"]),
            speed: num(&i["speed"]),
            floor: i["floor"].as_i64().expect("entier") as i32,
            physical: i["physical"].as_i64().map(|p| p as i32),
            delay: num(&i["delay"]),
            can_change: i["canChange"].as_bool().expect("booléen"),
            lookahead: num(&i["lookahead"]),
        };
        let pickups: Vec<PickupView> = case["pickups"]
            .as_array()
            .expect("liste")
            .iter()
            .map(|p| PickupView {
                kind: if p["kind"] == "bonus" {
                    PickupKind::Bonus
                } else {
                    PickupKind::Malus
                },
                x: num(&p["x"]),
                z: num(&p["z"]),
                y: num(&p["y"]),
            })
            .collect();
        planner.hint.rate = num(&case["rate"]);
        planner.invalidate();
        let route = *planner.plan(&pickups, &input);
        let expected = &case["route"];
        assert_eq!(route.count, expected["count"].as_u64().expect("entier") as usize, "cas {n} : count");
        assert_eq!(route.dodge_count, expected["dodgeCount"].as_u64().expect("entier") as usize, "cas {n} : dodge");
        for (k, aim) in nums(&expected["aim"]).into_iter().enumerate() {
            close(route.aim[k], aim, "aim", n);
        }
        for (k, time) in nums(&expected["time"]).into_iter().enumerate() {
            close(route.time[k], time, "time", n);
        }
        for (k, floor) in nums(&expected["floor"]).into_iter().enumerate() {
            assert_eq!(f64::from(route.floor[k]), floor, "cas {n} : étage {k}");
        }
        for (k, x) in nums(&expected["dodgeX"]).into_iter().enumerate() {
            close(route.dodge_x[k], x, "dodgeX", n);
        }
        for (k, t) in nums(&expected["dodgeTime"]).into_iter().enumerate() {
            close(route.dodge_time[k], t, "dodgeTime", n);
        }
        let want = case["routeFloor"].as_i64().map(|f| f as i32);
        assert_eq!(route_floor(&route, input.floor), want, "cas {n} : routeFloor");
        with_route += i32::from(route.count > 0);
        with_dodge += i32::from(route.dodge_count > 0);
    }
    // Le jeu de scénarios doit réellement exercer le plan, pas seulement des routes vides.
    assert!(with_route >= 60 && with_dodge >= 15, "{with_route} plans, {with_dodge} esquives");
}
