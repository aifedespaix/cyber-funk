//! Tests des projecteurs holographiques : le niveau d'instabilité ne dépasse jamais ce que le limiteur de flashs accorde (règle
//! 7), l'hologramme reste stable et allumé quand les flashs sont coupés, l'apparition est brève, la teinte suit la palette
//! étalonnée. Pas de banc GPU : on vérifie le texte du shader et, sur un MIROIR Rust de ses maths d'instabilité (marqué
//! ci-dessous, à tenir à l'identique du WGSL), la stabilité à niveau 0 et la cadence des changements.

use bevy::math::Vec4;
use cf_render::{DistrictGrade, ScenePalette, ThemeLibrary};
use cf_show::limiter::{Channel, MAX_FLASHES_PER_SECOND, MIN_FADE};
use cf_show::model::PresetTheme;

use super::holo_material::HOLO_SOURCE;
use super::holo_systems::{APPEAR_S, advance_appear, level_from, tint_for};
use crate::signs::flash::SignFlash;

const DT: f32 = 1.0 / 60.0;

/// Lit une constante WGSL `const NAME: f32 = valeur;` du shader.
fn wesl_const(name: &str) -> f32 {
    HOLO_SOURCE
        .lines()
        .find(|l| l.starts_with(&format!("const {name}:")))
        .and_then(|l| l.split('=').nth(1)?.trim().trim_end_matches(';').parse().ok())
        .unwrap_or(f32::NAN)
}

/// Un limiteur nourri par un kick à 180 bpm, comme le test des enseignes.
fn run(policy: Option<(bool, bool)>, seconds: u32) -> Vec<f32> {
    let mut flash = SignFlash::default();
    (0..60 * seconds)
        .map(|i| {
            let now = f64::from(i) / 60.0;
            flash.update(DT, now, policy, if i % 20 < 3 { 1.0 } else { 0.0 }, false);
            level_from(Some(&flash))
        })
        .collect()
}

// ---- MIROIR Rust des maths d'instabilité de `graffiti_holo.wesl` (à garder identique) ----

fn h11(x: f32) -> f32 {
    (x * 127.1).sin().mul_add(43_758.547, 0.0).rem_euclid(1.0)
}

/// Décalage horizontal (part de la largeur) de la tranche `slice` à l'instant `t`, pour un `level` et une graine.
fn jump(level: f32, slice: f32, t: f32, seed: f32) -> f32 {
    let tick = (t * wesl_const("TICK_HZ")).floor();
    let hit = f32::from(h11(slice * 7.3 + tick + seed * 31.0) >= 1.0 - wesl_const("HIT_SHARE") * level);
    (h11(slice + tick * 1.7 + seed * 17.0) - 0.5) * wesl_const("JUMP_MAX") * level * hit
}

#[test]
fn les_constantes_du_shader_sont_trouvees() {
    for name in [
        "TICK_HZ",
        "SLICES",
        "HIT_SHARE",
        "JUMP_MAX",
        "SHIFT_BASE",
        "SHIFT_LEVEL",
        "LEVEL_GAIN",
    ] {
        assert!(wesl_const(name).is_finite(), "constante {name} introuvable");
    }
}

#[test]
fn a_niveau_zero_le_texte_est_stable() {
    for slice in 0..16 {
        let reference = jump(0.0, slice as f32, 0.0, 0.3);
        for i in 0..600 {
            assert!(
                jump(0.0, slice as f32, i as f32 / 60.0, 0.3).abs() < f32::EPSILON && reference.abs() < f32::EPSILON,
                "tranche {slice} à {i}/60 s"
            );
        }
    }
    // À plein niveau, au contraire, des tranches bougent.
    assert!((0..16).any(|s| (0..120).any(|i| jump(1.0, s as f32, i as f32 / 60.0, 0.3).abs() > 1e-4)));
}

#[test]
fn le_texte_ne_change_pas_plus_de_trois_fois_par_seconde() {
    assert!(wesl_const("TICK_HZ") <= MAX_FLASHES_PER_SECOND as f32);
    // Toute quantification du temps dans le shader passe par `TICK_HZ`.
    for line in HOLO_SOURCE.lines().filter(|l| l.contains("floor(t")) {
        assert!(
            line.contains("TICK_HZ"),
            "quantification du temps hors TICK_HZ : {line}"
        );
    }
    // Entre deux instants d'une même tranche de temps, rien ne change.
    let tick_s = 1.0 / wesl_const("TICK_HZ");
    for slice in 0..16 {
        assert_eq!(
            jump(1.0, slice as f32, 0.1 * tick_s, 0.7),
            jump(1.0, slice as f32, 0.9 * tick_s, 0.7)
        );
    }
}

#[test]
fn le_niveau_ne_depasse_jamais_le_plafond_du_limiteur() {
    let levels = run(Some((true, false)), 20);
    assert!(levels.iter().all(|l| (0.0..=1.0).contains(l)));
    // 1 est le plafond de la voie « flash » : le niveau, remis à l'échelle de la voie, ne dépasse pas `Channel::Flash.cap()`.
    assert!(
        levels
            .iter()
            .map(|l| l * Channel::Flash.cap())
            .all(|v| v <= Channel::Flash.cap() + 1e-6)
    );
    assert!(
        levels.iter().any(|l| *l > 0.5),
        "le limiteur a laissé passer au moins un pouls"
    );
}

#[test]
fn jamais_plus_de_trois_changements_par_seconde() {
    let levels = run(Some((true, false)), 10);
    let slew = DT / MIN_FADE + 1e-4;
    assert!(
        levels.windows(2).all(|w| (w[1] - w[0]).abs() <= slew),
        "variation plus rapide que MIN_FADE"
    );
    let starts: Vec<usize> = (1..levels.len())
        .filter(|&i| levels[i] > 0.02 && levels[i - 1] <= 0.02)
        .collect();
    for w in starts.windows(MAX_FLASHES_PER_SECOND + 1) {
        assert!(
            w[MAX_FLASHES_PER_SECOND] - w[0] >= 60,
            "plus de 3 départs en 1 s : {w:?}"
        );
    }
    assert!(starts.len() > 5, "peu de départs : {}", starts.len());
}

#[test]
fn flashs_coupes_donne_un_hologramme_stable_pas_eteint() {
    for policy in [Some((false, false)), Some((true, true)), Some((false, true)), None] {
        assert!(run(policy, 10).iter().skip(10).all(|l| *l == 0.0), "{policy:?}");
    }
    // Pas de limiteur du tout (panne, enseignes absentes) : niveau 0, donc stable.
    assert_eq!(level_from(None), 0.0);
    // Et « stable » n'est pas « éteint » : à niveau 0 la lumière du texte et de la barre reste entière (le niveau ne fait
    // qu'y ajouter `LEVEL_GAIN`, il ne la retire jamais).
    assert!(wesl_const("LEVEL_GAIN").is_sign_positive());
    assert!(HOLO_SOURCE.contains("(1.0 + LEVEL_GAIN * level)"));
}

#[test]
fn l_apparition_dure_au_plus_1_5_s() {
    const { assert!(APPEAR_S <= 1.5) };
    let mut appear = 0.0;
    let mut frames = 0;
    while appear < 1.0 {
        appear = advance_appear(appear, DT);
        frames += 1;
        assert!(frames < 200, "l'apparition ne se termine pas");
    }
    assert!(frames as f32 * DT <= 1.5 + DT);
    assert_eq!(advance_appear(1.0, DT), 1.0);
}

#[test]
fn la_teinte_suit_la_palette_et_l_etalonnage() {
    let themes = ThemeLibrary::builtin().expect("thèmes livrés");
    let velvet = *themes.preset(PresetTheme::Velvet);
    let gold = *themes.preset(PresetTheme::Gold);
    for word in 0..8 {
        assert_ne!(
            tint_for(&velvet, word),
            tint_for(&gold, word),
            "mot {word} : la palette ne change rien"
        );
        assert!(tint_for(&velvet, word).truncate().max_element() > 0.0, "teinte noire");
    }
    // L'étalonnage de quartier nuance la teinte ; neutre, il la laisse telle quelle.
    let turned = DistrictGrade {
        hue_deg: 25.0,
        ..DistrictGrade::neutral()
    };
    assert_eq!(
        tint_for(&velvet.graded(&DistrictGrade::neutral()), 0),
        tint_for(&velvet, 0)
    );
    assert_ne!(tint_for(&velvet.graded(&turned), 0), tint_for(&velvet, 0));
    assert_eq!(tint_for(&ScenePalette::default(), 0).w, 0.0);
    let _ = Vec4::ZERO;
}
