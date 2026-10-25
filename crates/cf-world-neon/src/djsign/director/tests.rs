use super::*;

fn director() -> DjSignDirector {
    DjSignDirector::new(Box::new(|_| 0.7), false, 11)
}

fn view(dj: &str, dj_seq: u32, show_seq: u32) -> ShowView<'_> {
    ShowView {
        active: true,
        dj_sign: true,
        dj,
        dj_seq,
        dj_show_seq: show_seq,
        level: 2,
    }
}

fn ctx(show: Option<ShowView<'_>>, car_z: f64, time: f64) -> SignContext<'_> {
    SignContext {
        show,
        dt: 1.0 / 60.0,
        time,
        car_z,
        kick: false,
        pulse: 0.0,
        bpm: Some(128.0),
        mood: [0.1, 0.4, 0.8],
        intro: None,
        flashes: true,
    }
}

#[test]
fn hors_spectacle_rien_n_est_pose() {
    let mut d = director();
    d.update(&ctx(None, 0.0, 0.0), &ScenePalette::default());
    assert!(d.site().is_none() && !d.visible());
    assert!(d.focus().is_none());
    assert!(d.distance_ahead(0.0).is_infinite());
}

#[test]
fn le_premier_dj_est_deja_vu_et_n_appelle_pas_l_enseigne() {
    let mut d = director();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &ScenePalette::default());
    // Ni changement de DJ, ni ordre, ni retour périodique : l'enseigne attend.
    assert!(d.site().is_none());
}

#[test]
fn un_changement_de_dj_pose_l_enseigne_loin_devant() {
    let mut d = director();
    let p = ScenePalette::default();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("KIRA", 1, 0)), -100.0, 1.0), &p);
    let site = d.site().expect("enseigne posée");
    assert!(d.visible());
    // 520 m devant la voiture, 17 m de demi-profondeur d'immeuble.
    assert!((site.body.base.z - (-100.0 - AHEAD_M - 17.0)).abs() < 1e-3);
    assert!(site.body.base.x.abs() > f64::from(SITE_NEAR_M));
    assert_eq!(d.sign().name(), "KIRA");
    let clearing = d.clearing().expect("zone dégagée");
    assert!(clearing.z0 < site.sign_z && site.sign_z < clearing.z1);
    assert!(d.distance_ahead(-100.0) > 400.0);
    let (center, facing, width) = d.focus().expect("sujet");
    assert_eq!(facing, Vec3::Z);
    assert!(width > 0.0 && center.y > f64::from(site.roof_y));
}

#[test]
fn la_pre_intro_pose_plus_pres_et_libere_les_lettres() {
    let mut d = director();
    let p = ScenePalette::default();
    let mut c = ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0);
    c.intro = Some(0.0);
    d.update(&c, &p);
    let site = d.site().expect("posée");
    assert!((site.body.base.z - (-AHEAD_INTRO_M - 17.0)).abs() < 1e-3);
    assert!(d.allowed().abs() < f32::EPSILON);
    assert!(d.sign().lit().abs() < f32::EPSILON);
}

#[test]
fn l_ordre_afficher_le_dj_la_fait_venir() {
    let mut d = director();
    let p = ScenePalette::default();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0), &p);
    assert!(d.site().is_some());
}

#[test]
fn le_retour_periodique_demande_un_niveau_non_nul() {
    let p = ScenePalette::default();
    let mut d = director();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 1000.0), &p);
    assert!(d.site().is_some(), "après 16 minutes elle revient");

    let mut d = director();
    let mut v = view("AIFE", 0, 0);
    v.level = 0;
    d.update(&ctx(Some(v), 0.0, 0.0), &p);
    d.update(&ctx(Some(v), 0.0, 1000.0), &p);
    assert!(d.site().is_none(), "niveau 0 : pas de retour");
}

#[test]
fn elle_est_rangee_quand_la_voiture_l_a_depassee() {
    let mut d = director();
    let p = ScenePalette::default();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0), &p);
    let z = d.site().expect("posée").body.base.z;
    d.update(&ctx(Some(view("AIFE", 0, 1)), z - BEHIND_M - 1.0, 2.0), &p);
    assert!(d.site().is_none());
    assert!(d.clearing().is_none());
}

#[test]
fn un_changement_de_dj_devant_garde_l_enseigne_et_fait_tomber_les_lettres() {
    let mut d = director();
    let p = ScenePalette::default();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0), &p);
    let mut c = ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0);
    c.intro = None;
    c.kick = true;
    d.update(&c, &p);
    let seq = d.site_seq();
    d.update(&ctx(Some(view("KIRA", 1, 1)), 0.0, 2.0), &p);
    assert_eq!(d.site_seq(), seq, "même immeuble : l'enseigne est encore loin devant");
    assert!(d.sign().falling());
}

#[test]
fn un_changement_de_dj_juste_avant_le_depassement_en_fait_venir_une_neuve() {
    let mut d = director();
    let p = ScenePalette::default();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0), &p);
    let sign_z = d.site().expect("posée").body.base.z;
    let car = sign_z + STILL_AHEAD_M - 10.0;
    d.update(&ctx(Some(view("KIRA", 1, 1)), car, 2.0), &p);
    let site = d.site().expect("neuve");
    assert!(site.body.base.z < car - AHEAD_M + 1.0, "reposée devant la voiture");
}

#[test]
fn retirer_le_reglage_range_tout() {
    let mut d = director();
    let p = ScenePalette::default();
    d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
    d.update(&ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0), &p);
    let mut v = view("AIFE", 0, 1);
    v.dj_sign = false;
    d.update(&ctx(Some(v), 0.0, 2.0), &p);
    assert!(d.site().is_none() && !d.sign().present());
}

#[test]
fn la_teinte_est_la_plus_eloignee_de_l_ambiance() {
    let mut p = ScenePalette::default();
    for (color, rgb) in [
        (SceneColor::Sun, [1.0, 0.9, 0.1]),
        (SceneColor::Amber, [1.0, 0.5, 0.0]),
        (SceneColor::Rose, [1.0, 0.2, 0.4]),
        (SceneColor::Lime, [0.3, 1.0, 0.2]),
        (SceneColor::Violet, [0.5, 0.2, 1.0]),
    ] {
        p.set(
            color,
            cf_render::palette::LinearRgb {
                r: rgb[0],
                g: rgb[1],
                b: rgb[2],
            },
        );
    }
    let one = |m: [f32; 3]| pick_hue(&p, m);
    // Deux ambiances opposées ne choisissent pas la même teinte (la palette par défaut a 4 candidates distinctes).
    let hues: Vec<_> = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 1.0, 0.0]]
        .into_iter()
        .map(one)
        .collect();
    assert!(hues.iter().any(|h| *h != hues[0]), "{hues:?}");
}

#[test]
fn le_tirage_est_deterministe() {
    let p = ScenePalette::default();
    let run = || {
        let mut d = director();
        d.update(&ctx(Some(view("AIFE", 0, 0)), 0.0, 0.0), &p);
        d.update(&ctx(Some(view("AIFE", 0, 1)), 0.0, 1.0), &p);
        *d.site().expect("posée")
    };
    assert_eq!(run(), run());
}
