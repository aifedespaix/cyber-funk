//! Tests de la section « Graffitis » du panneau Debug : les interrupteurs écrivent le réglage, le mot imposé est un mot
//! chargé, les densités sont bornées, et la section n'existe qu'en Préparation.

use bevy_egui::egui;
use cf_render::{GraffitiTuning, GraffitiWords, MAX_GRAFFITI_DENSITY};

use super::graffiti::{committed, draw, sanitize};
use super::{Section, section_key_is_unique};
use crate::OperatorMode;
use crate::shell::Tab;

fn words() -> GraffitiWords {
    GraffitiWords(vec!["NEON".into(), "GRID".into()])
}

#[test]
fn les_interrupteurs_ecrivent_le_reglage() {
    let current = GraffitiTuning::default();
    let words = words();
    let walls_off = GraffitiTuning {
        wall_enabled: false,
        ..current.clone()
    };
    let next = committed(&current, walls_off, Some(&words)).expect("le geste change le réglage");
    assert!(!next.wall_enabled && next.holo_enabled);
    assert!(
        next.wall_factor().abs() < f32::EPSILON,
        "murs coupés : densité nulle côté monde"
    );
    assert!((next.holo_factor() - 1.0).abs() < f32::EPSILON, "projecteurs inchangés");
    let holo_off = GraffitiTuning {
        holo_enabled: false,
        ..next.clone()
    };
    let both = committed(&next, holo_off, Some(&words)).expect("deuxième interrupteur");
    assert!(both.holo_factor().abs() < f32::EPSILON && !both.wall_enabled);
    // Un geste qui ne change rien ne réveille pas la ressource.
    assert_eq!(committed(&both, both.clone(), Some(&words)), None);
}

#[test]
fn le_forcage_d_un_mot_est_dans_la_liste_des_mots_charges() {
    let words = words();
    let forced = |w: &str| GraffitiTuning {
        forced_word: Some(w.into()),
        ..GraffitiTuning::default()
    };
    assert_eq!(
        sanitize(forced("NEON"), Some(&words)).forced_word.as_deref(),
        Some("NEON")
    );
    assert_eq!(sanitize(forced("INCONNU"), Some(&words)).forced_word, None);
    // Sans mots chargés (cuisson pas finie, monde absent), rien ne peut être imposé.
    assert_eq!(sanitize(forced("NEON"), None).forced_word, None);
    assert_eq!(
        sanitize(forced("NEON"), Some(&GraffitiWords::default())).forced_word,
        None
    );
}

#[test]
fn la_densite_est_bornee() {
    let words = words();
    let with = |wall: f32, holo: f32| {
        sanitize(
            GraffitiTuning {
                wall_density: wall,
                holo_density: holo,
                ..GraffitiTuning::default()
            },
            Some(&words),
        )
    };
    let high = with(99.0, 1e9);
    assert!((high.wall_density - MAX_GRAFFITI_DENSITY).abs() < f32::EPSILON);
    assert!((high.holo_density - MAX_GRAFFITI_DENSITY).abs() < f32::EPSILON);
    let low = with(-3.0, -0.5);
    assert!(low.wall_density.abs() < f32::EPSILON && low.holo_density.abs() < f32::EPSILON);
    let broken = with(f32::NAN, f32::INFINITY);
    assert!((broken.wall_density - 1.0).abs() < f32::EPSILON && (broken.holo_density - 1.0).abs() < f32::EPSILON);
}

#[test]
fn la_section_est_listee_en_preparation_seulement() {
    assert!(Section::ALL.contains(&Section::Graffiti));
    assert!(section_key_is_unique());
    // L'onglet Debug, qui porte la section, n'est affiché qu'en Préparation.
    assert!(Tab::Debug.visible_in(OperatorMode::Preparation));
    assert!(!Tab::Debug.visible_in(OperatorMode::Live));
}

#[test]
fn la_section_se_dessine_sans_fenetre_et_sans_clic_ne_fait_rien() {
    let words = words();
    let tuning = GraffitiTuning::default();
    let ctx = egui::Context::default();
    let mut result = None;
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 400.0))),
            ..egui::RawInput::default()
        };
        ctx.run_ui(raw, |ui| result = Some(draw(ui, &tuning, Some(&words))))
            .drop_without_applying_deltas();
    }
    assert_eq!(result, Some(None), "aucun clic, aucun geste");
}
