//! Tests des étoiles filantes : départs, enveloppe, cadence, base du quad, golden de Sillage.

use super::*;

const DT: f32 = 1.0 / 60.0;

#[test]
fn la_premiere_etoile_part_apres_une_seconde_et_demie() {
    let mut stars = ShootingStars::new();
    let mut elapsed = 0.0;
    while stars.streaks().iter().all(Streak::is_free) {
        stars.update(DT, false);
        elapsed += DT;
        assert!(elapsed < 3.0, "aucun départ");
    }
    assert!((elapsed - 1.5).abs() < 0.05, "{elapsed}");
}

#[test]
fn un_depart_reste_tangent_a_la_voute_et_descendant() {
    let mut stars = ShootingStars::new();
    for _ in 0..8 {
        let slot = stars.launch().expect("pool libre");
        let streak = stars.streaks()[slot];
        assert!((streak.position.length() - RADIUS_M).abs() < 1e-2);
        assert!(
            streak.direction.dot(streak.position.normalize()).abs() < 1e-4,
            "tangente"
        );
        assert!((streak.direction.length() - 1.0).abs() < 1e-5);
        assert!(streak.direction.y < 0.0, "descendant : {:?}", streak.direction);
        assert!((0.7..=1.3).contains(&streak.life_s));
        assert!((420.0..=760.0).contains(&streak.speed_mps));
        // Libère le pool et laisse passer la cadence : 8 départs d'affilée.
        stars.clock_s += 2.0;
        stars.streaks = [Streak::FREE; POOL];
    }
}

#[test]
fn le_depart_est_deterministe() {
    let a = {
        let mut s = ShootingStars::new();
        s.launch().map(|i| s.streaks()[i])
    };
    let b = {
        let mut s = ShootingStars::new();
        s.launch().map(|i| s.streaks()[i])
    };
    assert_eq!(a, b);
}

#[test]
fn l_opacite_monte_et_descend_en_cloche_sans_jamais_etre_nan() {
    let mut streak = Streak::FREE;
    streak.life_s = 1.0;
    streak.age_s = 0.0;
    let mut previous = 0.0;
    let mut peak = 0.0f32;
    for step in 0..=100 {
        streak.age_s = step as f32 / 100.0 * 1.2;
        let alpha = streak.alpha();
        assert!(alpha.is_finite() && (0.0..=1.0).contains(&alpha), "{alpha}");
        peak = peak.max(alpha);
        previous = alpha;
    }
    assert!(peak > 0.99, "{peak}");
    assert!(previous.abs() < 1e-6, "libre à la fin");
    // Le sinus de π en `f32` est négatif : exactement l'âge qui donnait NaN sans plancher.
    streak.age_s = streak.life_s - f32::EPSILON;
    assert!(streak.alpha().is_finite());
}

#[test]
fn la_montee_est_plus_lente_que_la_limite_photosensible() {
    // Variation maximale d'opacité d'une image à l'autre, à 60 i/s, sur toute la vie : jamais un saut brutal.
    let streak = Streak {
        life_s: LIFE_S.0,
        ..Streak::FREE
    };
    let mut worst = 0.0f32;
    let mut last = 0.0;
    for step in 1..(LIFE_S.0 / DT) as u32 {
        let alpha = Streak {
            age_s: step as f32 * DT,
            ..streak
        }
        .alpha();
        worst = worst.max((alpha - last).abs());
        last = alpha;
    }
    // Les 0,08 s de MIN_FADE font ~5 images : une montée de 0 à 1 en moins de cinq images serait un flash.
    // L'enveloppe en sinus^0,7 a sa pente maximale au tout début (0,7 de puissance), mais sur un trait de moins de 3 m
    // de large : l'écart d'une image reste sous 0,3.
    assert!(worst < 0.3, "{worst}");
}

#[test]
fn la_cadence_respecte_trois_par_seconde_et_un_quart_de_seconde() {
    let mut stars = ShootingStars::new();
    let mut times = Vec::new();
    for frame in 0..600 {
        // Demande à chaque image pendant dix secondes : le limiteur tranche.
        if stars.launch().is_some() {
            times.push(f64::from(frame) * f64::from(DT));
        }
        stars.update(DT, false);
    }
    assert!(times.len() >= 10, "{times:?}");
    for pair in times.windows(2) {
        assert!(pair[1] - pair[0] >= MIN_GAP_S - 1e-6, "écart {pair:?}");
    }
    for window in times.windows(MAX_PER_SECOND + 1) {
        assert!(
            window[MAX_PER_SECOND] - window[0] >= 1.0 - 1e-6,
            "4 départs en moins d'une seconde : {window:?}"
        );
    }
}

#[test]
fn le_pool_plein_refuse_le_depart() {
    let mut stars = ShootingStars::new();
    let mut launched = 0;
    // Cinq départs possibles à la cadence maximale, puis le pool (vie ≥ 0,7 s) est encore occupé.
    for _ in 0..(3.0 / f64::from(DT)) as u32 {
        if stars.launch().is_some() {
            launched += 1;
        }
        stars.update(DT, false);
        let flying = stars.streaks().iter().filter(|s| !s.is_free()).count();
        assert!(flying <= POOL);
    }
    assert!(launched >= 2);
}

#[test]
fn la_tete_est_a_la_position_et_la_base_est_orthonormee() {
    let mut stars = ShootingStars::new();
    let slot = stars.launch().expect("libre");
    for _ in 0..30 {
        stars.update(DT, true);
    }
    let streak = stars.streaks()[slot];
    let (center, rotation, scale) = streak.pose();
    // L'axe X local est la direction du mouvement ; la tête est à +longueur/2 du centre.
    let x = rotation * Vec3::X;
    assert!(x.dot(streak.direction) > 0.9999);
    let head = center + x * (scale.x * 0.5);
    assert!(head.distance(streak.position) < 1e-2);
    // La face du quad regarde vers le centre du ciel (la direction du vol, elle, ne tourne pas avec la normale).
    let face = rotation * Vec3::Z;
    assert!(face.dot(-streak.position.normalize()) > 0.7);
    assert!(face.dot(x).abs() < 1e-5, "base orthogonale");
    assert!((rotation.length() - 1.0).abs() < 1e-5);
}

/// Les départs contre ceux de `shooting-stars.ts` (voir `tools/sillage-golden/sky/`, `tests/golden/sky.json`).
#[test]
fn les_departs_sont_ceux_de_sillage() {
    let golden: serde_json::Value = serde_json::from_str(include_str!("../../tests/golden/sky.json")).expect("json");
    let launches = golden["launches"].as_array().expect("départs");
    assert!(launches.len() >= 4);
    let near = |v: &serde_json::Value, expected: f32, tol: f32, what: &str| {
        let v = v.as_f64().expect("nombre") as f32;
        assert!((v - expected).abs() <= tol, "{what} : {v} contre {expected}");
    };
    let mut stars = ShootingStars::new();
    for launch in launches {
        // Un départ de plus, après la pause qu'il faut à la cadence maximale, sur un pool libéré : le trait de Sillage
        // repart de zéro de la même façon.
        stars.clock_s += 2.0;
        stars.streaks = [Streak::FREE; POOL];
        let slot = stars.launch().expect("départ");
        let streak = stars.streaks()[slot];
        let k = launch["k"].as_u64().expect("k");
        for (axis, value) in streak.position.to_array().iter().enumerate() {
            near(&launch["position"][axis], *value, 0.02, &format!("départ {k} position"));
        }
        for (axis, value) in streak.direction.to_array().iter().enumerate() {
            near(
                &launch["direction"][axis],
                *value,
                1e-3,
                &format!("départ {k} direction"),
            );
        }
        near(&launch["speed"], streak.speed_mps, 0.02, "vitesse");
        near(&launch["life"], streak.life_s, 1e-3, "vie");
        near(&launch["scale"][0], streak.length_m, 0.02, "longueur");
        near(&launch["scale"][1], streak.width_m, 1e-3, "largeur");
        // Teinte : numéro du rôle de couleur de la scène (glace 7, menthe 2, azur 3, corail 5).
        let roles = [7, 2, 3, 5];
        assert_eq!(
            launch["tint"].as_u64(),
            Some(roles[streak.tint]),
            "teinte du départ {k}"
        );
    }
}

#[test]
fn calme_espace_les_departs() {
    let count = |calm: bool| {
        let mut stars = ShootingStars::new();
        let mut launches = 0;
        let mut was_free = true;
        for _ in 0..(120.0 / f64::from(DT)) as u32 {
            stars.update(DT, calm);
            let any = stars.streaks().iter().any(|s| s.age_s < DT * 1.5);
            if any && was_free {
                launches += 1;
            }
            was_free = !any;
        }
        launches
    };
    assert!(count(true) < count(false));
}
