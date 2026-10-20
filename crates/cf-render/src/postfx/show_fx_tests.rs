use cf_show::limiter::{MIN_FADE, is_saturated_red};

use super::*;

const DT: f64 = 1.0 / 60.0;

fn show() -> ShowFxInput {
    ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.0,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop: None,
    }
}

/// Le drop qui frappe dans l'image, avec ses effets.
#[allow(clippy::unnecessary_wraps, reason = "se range directement dans `ShowFxInput::drop`")]
fn drop_fx(flash: f32, rgb_split: f32) -> Option<DropFxPackage> {
    Some(DropFxPackage {
        flash,
        rgb_split,
        city_pulse: 0.0,
        shockwaves: 1,
        burst: 20,
    })
}

/// Fait tourner `fx` pendant `frames` images à partir de `*t`, avec la même entrée.
fn run(fx: &mut ShowFx, input: Option<&ShowFxInput>, frames: usize, t: &mut f64) {
    for _ in 0..frames {
        fx.update(input, 0.0, false, *t, 0);
        *t += DT;
    }
}

#[test]
fn sans_spectacle_aucun_flash_meme_sur_un_kick_au_sommet() {
    let mut fx = ShowFx::default();
    let mut t = 0.0;
    for i in 0..600 {
        // Un kick franc toutes les 30 images, au sommet du morceau.
        let pulse = if i % 30 == 0 { 1.0 } else { 0.0 };
        fx.update(None, pulse, true, t, 0);
        t += DT;
        assert!(fx.flash().abs() < f32::EPSILON);
        assert!(fx.sweep().abs() < f32::EPSILON);
        assert!(fx.split().abs() < f32::EPSILON);
    }
    assert!(!fx.limiter().permitted());
}

#[test]
fn le_flash_du_drop_passe_par_le_limiteur_plafond_et_montee() {
    let mut fx = ShowFx::default();
    let mut t = 0.0;
    run(&mut fx, Some(&show()), 2, &mut t);
    // Le drop demande un flash plein (1,0) : le limiteur le ramène à `MAX_FLASH_PEAK`.
    let hit = ShowFxInput {
        drop: drop_fx(1.0, 0.5),
        ..show()
    };
    fx.update(Some(&hit), 0.0, false, t, 0);
    t += DT;
    // Montée bornée : pas plus de `cap × dt / MIN_FADE` en une image.
    let first = fx.flash();
    assert!(first > 0.0, "le flash du drop est accordé");
    assert!(
        first <= MAX_FLASH_PEAK * (DT as f32) / MIN_FADE + 1e-6,
        "montée trop rapide : {first}"
    );
    let mut top = first;
    for _ in 0..120 {
        fx.update(Some(&show()), 0.0, false, t, 0);
        t += DT;
        top = top.max(fx.flash());
    }
    assert!(top <= MAX_FLASH_PEAK + 1e-6, "plafond dépassé : {top}");
    assert!(top > MAX_FLASH_PEAK * 0.9, "le flash atteint son plafond : {top}");
    assert!(fx.flash().abs() < f32::EPSILON, "le flash est retombé");
}

#[test]
fn flashs_coupes_ou_respiration_aucun_flash_de_drop() {
    for input in [
        ShowFxInput {
            flashes: false,
            drop: drop_fx(1.0, 1.0),
            ..show()
        },
        ShowFxInput {
            breathing: true,
            drop: drop_fx(1.0, 1.0),
            ..show()
        },
    ] {
        let mut fx = ShowFx::default();
        let mut t = 0.0;
        for _ in 0..300 {
            fx.update(Some(&input), 1.0, true, t, 0);
            t += DT;
            assert!(fx.flash().abs() < f32::EPSILON, "{input:?}");
            assert!(fx.sweep().abs() < f32::EPSILON, "{input:?}");
        }
    }
}

#[test]
fn respiration_la_separation_rvb_tombe_a_presque_rien() {
    let loud = ShowFxInput {
        fx_rgb_split: 1.0,
        ..show()
    };
    let calm = ShowFxInput {
        breathing: true,
        ..loud
    };
    let mut a = ShowFx::default();
    let mut b = ShowFx::default();
    a.update(Some(&loud), 1.0, false, 0.0, 0);
    b.update(Some(&calm), 1.0, false, 0.0, 0);
    assert!((a.split() - 0.009).abs() < 1e-6);
    assert!((b.split() - 0.009 * 0.05).abs() < 1e-6);
}

#[test]
fn flash_de_kick_sur_front_montant_au_sommet_au_plus_deux_par_seconde() {
    let input = ShowFxInput {
        fx_flash: 1.0,
        ..show()
    };
    let mut fx = ShowFx::default();
    let mut t = 0.0;
    let mut starts = Vec::new();
    let mut before = 0.0_f32;
    // Un kick toutes les 6 images (10 par seconde) pendant 10 s : bien plus que permis.
    for i in 0..600 {
        let pulse = if i % 6 == 0 { 1.0 } else { 0.2 };
        fx.update(Some(&input), pulse, true, t, 0);
        if fx.flash() > 0.0 && before <= 0.0 {
            starts.push(t);
        }
        before = fx.flash();
        t += DT;
    }
    assert_ne!(starts.len(), 0);
    for pair in starts.windows(2) {
        assert!(pair[1] - pair[0] > KICK_FLASH_GAP - 1e-9, "{pair:?}");
    }
    // Pic de kick : `gates.fx.flash × 0,18`.
    let mut fx = ShowFx::default();
    let mut top = 0.0_f32;
    for i in 0..60 {
        fx.update(Some(&input), if i == 1 { 1.0 } else { 0.0 }, true, f64::from(i) * DT, 0);
        top = top.max(fx.flash());
    }
    assert!((top - 0.18).abs() < 1e-4, "{top}");
}

#[test]
fn hors_sommet_pas_de_flash_de_kick() {
    let input = ShowFxInput {
        fx_flash: 1.0,
        ..show()
    };
    let mut fx = ShowFx::default();
    for i in 0..300 {
        let pulse = if i % 30 == 0 { 1.0 } else { 0.0 };
        fx.update(Some(&input), pulse, false, f64::from(i) * DT, 0);
        assert!(fx.flash().abs() < f32::EPSILON);
    }
}

#[test]
fn un_drop_a_chaque_image_reste_sous_trois_flashs_par_seconde() {
    let input = ShowFxInput {
        drop: drop_fx(1.0, 1.0),
        cue_seq: 0,
        ..show()
    };
    let mut fx = ShowFx::default();
    let mut starts: Vec<f64> = Vec::new();
    let mut before = 0.0_f32;
    for i in 0..1200 {
        let t = f64::from(i) * DT;
        // Le cue change aussi à chaque image : flash et vague se disputent le même budget.
        let frame = ShowFxInput { cue_seq: i, ..input };
        fx.update(Some(&frame), 1.0, true, t, 0);
        let level = fx.flash() + fx.sweep();
        if level > before + 1e-6 && before < 1e-6 {
            starts.push(t);
        }
        before = level;
    }
    for (i, &start) in starts.iter().enumerate() {
        let in_window = starts[i..].iter().take_while(|&&s| s - start < 1.0).count();
        assert!(in_window <= 3, "{in_window} départs dans la seconde qui suit {start}");
    }
}

#[test]
fn un_changement_de_cue_lance_la_vague_qui_traverse_l_ecran() {
    let mut fx = ShowFx::default();
    let mut t = 0.0;
    run(&mut fx, Some(&show()), 3, &mut t);
    assert!(fx.sweep().abs() < f32::EPSILON, "le premier cue vu ne lance rien");
    assert!(fx.sweep_pos() > 1.2, "vague au repos hors écran");
    let next = ShowFxInput { cue_seq: 1, ..show() };
    run(&mut fx, Some(&next), 1, &mut t);
    assert!(fx.sweep() > 0.0);
    assert!(fx.sweep_pos() < 0.0, "la vague part de la gauche");
    let mut top = 0.0_f32;
    for _ in 0..240 {
        run(&mut fx, Some(&next), 1, &mut t);
        top = top.max(fx.sweep());
    }
    assert!(top <= MAX_SWEEP_PEAK + 1e-6);
    assert!(fx.sweep_pos() > 1.2, "la vague est sortie à droite");
    // Couleur de la vague : celle du cue (rang 1 → `mint`).
    assert_eq!(fx.sweep_color(), FlashPalette::default().sweeps[1].rgb());
}

#[test]
fn l_etalonnage_monte_avec_la_chaleur_et_ne_descend_jamais_sous_un() {
    let hot = ShowFxInput { heat: 1.0, ..show() };
    let mut fx = ShowFx::default();
    let mut t = 0.0;
    run(&mut fx, Some(&hot), 60 * 10, &mut t);
    assert!((fx.contrast() - 1.07).abs() < 1e-3, "{}", fx.contrast());
    assert!((fx.saturation() - 1.12).abs() < 1e-3, "{}", fx.saturation());
    run(&mut fx, None, 60 * 10, &mut t);
    assert!(fx.contrast() >= 1.0 && fx.contrast() < 1.001);
    assert!(fx.saturation() >= 1.0 && fx.saturation() < 1.001);
}

#[test]
fn bullet_time_le_flou_radial_se_coupe_aux_basses_qualites() {
    let slow = ShowFxInput {
        time_scale: 0.2,
        ..show()
    };
    let mut fx = ShowFx::default();
    let mut t = 0.0;
    run(&mut fx, Some(&slow), 120, &mut t);
    assert!(fx.slow() > 0.99);
    assert!(fx.blur() > 0.99);
    fx.update(Some(&slow), 0.0, false, t, 2);
    assert!(fx.blur().abs() < f32::EPSILON);
}

#[test]
fn une_palette_rouge_ne_donne_jamais_un_flash_rouge_sature() {
    let red = SafeColor::new(Rgb::new(1.0, 0.0, 0.0));
    let mut fx = ShowFx::new(FlashPalette {
        flash: red,
        sweeps: [red; 4],
    });
    let mut t = 0.0;
    run(&mut fx, Some(&show()), 1, &mut t);
    let hit = ShowFxInput {
        drop: drop_fx(1.0, 0.0),
        cue_seq: 1,
        ..show()
    };
    run(&mut fx, Some(&hit), 1, &mut t);
    assert!(!is_saturated_red(fx.flash_color()));
    assert!(!is_saturated_red(fx.sweep_color()));
}

#[test]
fn les_couleurs_par_defaut_sont_celles_du_theme_velours_en_lineaire() {
    let palette = FlashPalette::default();
    // `ice` = #e6f7ff : bleu à 1, rouge ≈ 0,79 en linéaire.
    let ice = palette.flash.rgb();
    assert!((ice.b - 1.0).abs() < 1e-6);
    assert!((ice.r - 0.791).abs() < 1e-3, "{ice:?}");
}
