//! Tests de l'étalonnage de quartier : neutre exact, bornes, écart plus petit qu'un changement de thème, luminance
//! voisine (règle 7), fondu continu, attente d'un temps fort, RON livré.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cf_show::district::{DistrictId, DistrictKind};
use cf_show::model::PresetTheme;

use super::grade::{ACCENT_RANGE, DistrictGrade, DistrictGradeState, LIGHTNESS_RANGE, MAX_HUE_DEG, SATURATION_RANGE};
use super::grade_library::GradeLibrary;
use super::scene::{PRESETS, PaletteError, SceneColor, ScenePalette, ThemeLibrary};

const DT: f32 = 1.0 / 60.0;

fn grades() -> GradeLibrary {
    GradeLibrary::builtin().unwrap()
}

/// Tous les étalonnages livrés, avec leur type.
fn all_grades() -> Vec<(DistrictKind, DistrictGrade)> {
    let library = grades();
    let mut out = Vec::new();
    for kind in DistrictKind::ALL {
        for variant in 0..library.variants(kind) {
            out.push((kind, library.grade(DistrictId { kind, variant })));
        }
    }
    out
}

fn themes() -> Vec<ScenePalette> {
    let library = ThemeLibrary::builtin().unwrap();
    PRESETS.iter().map(|p| *library.preset(*p)).collect()
}

/// Luminance relative moyenne de la palette (Y de Rec. 709 sur la lumière linéaire, moyenne des onze rôles).
fn mean_luminance(palette: &ScenePalette) -> f32 {
    let sum: f32 = SceneColor::ALL
        .iter()
        .map(|role| {
            let c = palette.get(*role);
            0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
        })
        .sum();
    sum / 11.0
}

fn relative_gap(a: f32, b: f32) -> f32 {
    (a - b).abs() / a.max(b).max(1e-6)
}

/// Écart moyen entre deux palettes : par rôle, distance de teinte (en tours, pesée par la saturation la plus faible
/// des deux, une teinte de gris ne veut rien dire) plus distance de luminosité HSL.
fn palette_gap(a: &ScenePalette, b: &ScenePalette) -> f32 {
    let sum: f32 = SceneColor::ALL
        .iter()
        .map(|role| {
            let (x, y) = (a.get(*role).to_hsl(), b.get(*role).to_hsl());
            let dh = (x.h - y.h).abs();
            let dh = dh.min(1.0 - dh);
            dh * x.s.min(y.s) + (x.l - y.l).abs()
        })
        .sum();
    sum / 11.0
}

#[test]
fn neutre_ne_change_rien() {
    for palette in themes() {
        assert_eq!(palette.graded(&DistrictGrade::neutral()), palette);
        // Neon variante 0 est le neutre.
        let neon0 = grades().grade(DistrictId {
            kind: DistrictKind::Neon,
            variant: 0,
        });
        assert!(neon0.is_neutral());
        assert_eq!(palette.graded(&neon0), palette);
    }
}

#[test]
fn bornes_respectees() {
    for (kind, grade) in all_grades() {
        assert!(grade.hue_deg.abs() <= MAX_HUE_DEG, "{kind:?} teinte");
        assert!(
            (SATURATION_RANGE.0..=SATURATION_RANGE.1).contains(&grade.saturation),
            "{kind:?} saturation"
        );
        assert!(
            (LIGHTNESS_RANGE.0..=LIGHTNESS_RANGE.1).contains(&grade.lightness),
            "{kind:?} luminosité"
        );
        for (role, weight) in grade.accents {
            assert!(
                (ACCENT_RANGE.0..=ACCENT_RANGE.1).contains(&weight),
                "{kind:?} accent {role:?}"
            );
        }
        assert!(grade.within_bounds());
    }
}

#[test]
fn plus_petit_qu_un_changement_de_theme() {
    let palettes = themes();
    let mut min_theme_gap = f32::MAX;
    for (i, a) in palettes.iter().enumerate() {
        for b in &palettes[i + 1..] {
            min_theme_gap = min_theme_gap.min(palette_gap(a, b));
        }
    }
    let mut worst = 0.0_f32;
    for palette in &palettes {
        for (kind, grade) in all_grades() {
            let gap = palette_gap(palette, &palette.graded(&grade));
            worst = worst.max(gap);
            assert!(
                gap < min_theme_gap,
                "{kind:?} : écart {gap} >= écart mini entre thèmes {min_theme_gap}"
            );
        }
    }
    assert!(worst > 0.0, "les étalonnages doivent se voir");
}

#[test]
fn les_etalonnages_se_voient_et_les_variantes_different() {
    let base = themes()[0];
    for kind in DistrictKind::ALL {
        let library = grades();
        let rendered: Vec<ScenePalette> = (0..library.variants(kind))
            .map(|variant| base.graded(&library.grade(DistrictId { kind, variant })))
            .collect();
        for (i, a) in rendered.iter().enumerate() {
            for b in &rendered[i + 1..] {
                assert_ne!(a, b, "{kind:?} : deux variantes identiques");
            }
        }
    }
    // Chaque type s'éloigne du neutre (sauf Neon 0).
    let suburb = grades().grade(DistrictId {
        kind: DistrictKind::Suburb,
        variant: 0,
    });
    assert_ne!(base.graded(&suburb), base);
}

/// Règle 7 : la luminance moyenne d'un même type varie de moins de 10 %, et un fondu entre deux étalonnages
/// quelconques (même de types différents) change la luminance moyenne de moins de 10 % : un fondu d'étalonnage ne peut
/// pas devenir un flash. Cette garantie vient des valeurs livrées de `district_grades.ron`, vérifiées ici : les bornes
/// de teinte, luminosité et accents ne la donnent pas à elles seules.
#[test]
fn luminance_voisine() {
    const TOLERATED: f32 = 0.10;
    for palette in themes() {
        let all = all_grades();
        let lum: Vec<(DistrictKind, f32)> = all
            .iter()
            .map(|(k, g)| (*k, mean_luminance(&palette.graded(g))))
            .collect();
        for (i, (ka, a)) in lum.iter().enumerate() {
            for (kb, b) in &lum[i + 1..] {
                let gap = relative_gap(*a, *b);
                assert!(gap < TOLERATED, "{ka:?} / {kb:?} : saut de luminance {gap}");
            }
        }
    }
}

#[test]
fn fondu_continu() {
    let library = grades();
    let from_id = DistrictId {
        kind: DistrictKind::Neon,
        variant: 3,
    };
    let to_id = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 2,
    };
    for fade_s in [0.5_f32, 4.0, 10.0] {
        let mut state = DistrictGradeState::new(library.clone());
        // Se pose d'abord sur le premier quartier (kick de temps fort), puis repart vers l'autre.
        state.set_target(from_id, 0.0);
        state.tick_pending(DT, true, 4);
        let start = state.update(DT);
        assert_eq!(start, library.grade(from_id));
        state.set_target(to_id, fade_s);
        state.tick_pending(DT, true, 8);
        let end = library.grade(to_id);
        let mut previous = start;
        let mut frames = 0;
        while state.is_fading() {
            let now = state.update(DT);
            let steps = [
                (previous.hue_deg, now.hue_deg, start.hue_deg, end.hue_deg),
                (previous.saturation, now.saturation, start.saturation, end.saturation),
                (previous.lightness, now.lightness, start.lightness, end.lightness),
            ];
            for (p, n, s, e) in steps {
                assert!(
                    (n - p).abs() <= (e - s).abs() * DT / fade_s * 1.01 + 1e-6,
                    "pas trop grand : {p} -> {n}"
                );
            }
            for ((_, p), ((_, n), ((_, s), (_, e)))) in previous
                .accents
                .iter()
                .zip(now.accents.iter().zip(start.accents.iter().zip(end.accents.iter())))
            {
                assert!((n - p).abs() <= (e - s).abs() * DT / fade_s * 1.01 + 1e-6);
            }
            previous = now;
            frames += 1;
            assert!(frames < 2000);
        }
        assert_eq!(previous, end, "arrive exactement sur la cible");
        assert!(fade_s.mul_add(60.0, 2.0) >= frames as f32);
    }
}

#[test]
fn changer_de_cible_en_plein_fondu_repart_de_l_etat_courant() {
    let mut state = DistrictGradeState::default();
    let first = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 0,
    };
    state.set_target(first, 4.0);
    state.tick_pending(DT, true, 4);
    let mut now = DistrictGrade::neutral();
    for _ in 0..120 {
        now = state.update(DT);
    }
    state.set_target(
        DistrictId {
            kind: DistrictKind::Residential,
            variant: 1,
        },
        4.0,
    );
    state.tick_pending(DT, true, 8);
    let next = state.update(0.0);
    assert!(next.within_bounds());
    assert!(
        (next.hue_deg - now.hue_deg).abs() < 1e-4,
        "pas de saut au changement de cible"
    );
}

#[test]
fn la_cible_attend_un_temps_fort_puis_au_plus_quatre_secondes() {
    let id = DistrictId {
        kind: DistrictKind::Residential,
        variant: 0,
    };
    let mut state = DistrictGradeState::default();
    state.set_target(id, 8.0);
    assert!(state.is_pending());
    // Kicks 1, 2, 3 : pas un temps fort ; rien ne bouge.
    for count in 1..=3 {
        state.tick_pending(DT, true, count);
        assert!(state.is_pending());
        assert!(state.update(DT).is_neutral());
    }
    state.tick_pending(DT, true, 4);
    assert!(!state.is_pending());
    assert!(state.is_fading());

    // Sans aucun kick, le départ se fait quand même après MAX_WAIT.
    let mut state = DistrictGradeState::default();
    state.set_target(id, 8.0);
    for _ in 0..(4 * 60 + 2) {
        state.tick_pending(DT, false, 0);
    }
    assert!(!state.is_pending());
}

#[test]
fn le_fondu_est_borne_a_dix_secondes() {
    let mut state = DistrictGradeState::default();
    state.set_target(
        DistrictId {
            kind: DistrictKind::Suburb,
            variant: 0,
        },
        120.0,
    );
    state.tick_pending(DT, true, 4);
    for _ in 0..(10 * 60 + 2) {
        state.update(DT);
    }
    assert!(!state.is_fading());
}

#[test]
fn ron_livre_valide() {
    let library = grades();
    for kind in DistrictKind::ALL {
        assert!((6..=8).contains(&library.variants(kind)), "{kind:?}");
    }
    // Une variante hors liste ne panique pas.
    let _ = library.grade(DistrictId {
        kind: DistrictKind::Neon,
        variant: 200,
    });
}

#[test]
fn ron_hors_bornes_refuse() {
    let row = "(hue_deg: 0.0, saturation: 1.0, lightness: 1.0, mint: 1.0, rose: 1.0, amber: 1.0, ice: 1.0)";
    let bad_row = "(hue_deg: 45.0, saturation: 1.0, lightness: 1.0, mint: 1.0, rose: 1.0, amber: 1.0, ice: 1.0)";
    let list = |first: &str| format!("[{first}, {row}, {row}, {row}, {row}, {row}]");
    let text = |first: &str| format!("(neon: {0}, suburb: {1}, residential: {1})", list(first), list(row));
    assert!(GradeLibrary::from_ron(&text(row)).is_ok());
    assert!(matches!(
        GradeLibrary::from_ron(&text(bad_row)),
        Err(PaletteError::BadGrade { .. })
    ));
    assert!(matches!(GradeLibrary::from_ron("("), Err(PaletteError::GradesRon(_))));
    let short = format!("(neon: [{row}], suburb: {0}, residential: {0})", list(row));
    assert!(matches!(
        GradeLibrary::from_ron(&short),
        Err(PaletteError::BadGrade { .. })
    ));
    // Neon 0 doit être neutre.
    let first = "(hue_deg: 5.0, saturation: 1.0, lightness: 1.0, mint: 1.0, rose: 1.0, amber: 1.0, ice: 1.0)";
    assert!(matches!(
        GradeLibrary::from_ron(&text(first)),
        Err(PaletteError::BadGrade { .. })
    ));
}

#[test]
fn le_rouge_d_alarme_n_est_pas_touche() {
    let base = *ThemeLibrary::builtin().unwrap().preset(PresetTheme::Aurora);
    for (_, grade) in all_grades() {
        assert_eq!(base.graded(&grade).get(SceneColor::Alarm), base.get(SceneColor::Alarm));
    }
}

#[test]
fn set_target_repete_ne_repousse_pas_le_repli() {
    let id = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 1,
    };
    let mut state = DistrictGradeState::default();
    let mut started_at = None;
    // 6 s sans aucun kick, la demande ré-émise toutes les 0,5 s.
    for frame in 0..(6 * 60) {
        if frame % 30 == 0 {
            state.set_target(id, 4.0);
        }
        state.tick_pending(DT, false, 0);
        if started_at.is_none() && !state.is_pending() {
            started_at = Some(frame);
        }
    }
    let frame = started_at.expect("le repli doit partir");
    assert!(frame <= 4 * 60 + 2, "départ à l'image {frame}");
}

#[test]
fn viser_l_etalonnage_courant_ne_lance_aucun_fondu() {
    let id = DistrictId {
        kind: DistrictKind::Residential,
        variant: 2,
    };
    let mut state = DistrictGradeState::default();
    state.set_target(id, 0.0);
    state.tick_pending(DT, true, 4);
    state.update(DT);
    assert!(!state.is_fading());
    state.set_target(id, 8.0);
    assert!(!state.is_pending());
    assert!(!state.is_fading());
}
