//! Tests des immeubles-vitrines à façade (J7-09).

#![allow(clippy::panic, clippy::expect_used)]

use super::extras::{BEAM, BLOCKS, LAMP, RADIO_TOP, WAVES, beam_mesh, pagoda_mesh};
use super::spec::{Extra, SPECS};
use super::*;
use crate::landmarks::field::{FieldState, MAX_LIGHTS};
use crate::landmarks::frame::{Colors, FlashPolicy};

fn place() -> Placement {
    Placement {
        z: -4000.0,
        side: 1,
        tint: 0,
        scale: 1.0,
        spans_road: false,
    }
}

fn permitted() -> FlashPolicy {
    FlashPolicy {
        flashes: true,
        breathing: false,
    }
}

/// Un immeuble posé, avec ses pièces prêtes à peindre.
struct Rig {
    showcase: Showcase,
    field: FieldState,
    levels: [f32; MAX_LIGHTS],
    place: Placement,
}

impl Rig {
    fn new(kind: LandmarkKind) -> Self {
        let mut showcase = Showcase::new(kind).expect("une vitrine à façade");
        let place = place();
        showcase.show(&place);
        Self {
            showcase,
            field: FieldState::default(),
            levels: [0.0; MAX_LIGHTS],
            place,
        }
    }

    fn step(&mut self, frame: &LandmarkFrame) {
        let mut ctx = StepCtx::new(&mut self.field, &self.place, &mut self.levels, frame.dt);
        self.showcase.step(frame, &mut ctx);
    }

    fn paint(&self, frame: &LandmarkFrame) -> Vec<PartPaint> {
        let mut out = vec![PartPaint::default(); self.showcase.parts().len()];
        self.showcase.paint(frame, &mut out);
        out
    }

    fn lit(&self) -> Vec<f64> {
        self.field
            .lights()
            .iter()
            .filter(|l| l.color.length() > 0.0)
            .map(|l| l.position.y)
            .collect()
    }
}

#[test]
fn les_dix_fiches_couvrent_les_dix_vitrines_une_fois() {
    assert_eq!(SPECS.len(), 10);
    let mut kinds: Vec<_> = SPECS.iter().map(|s| s.kind.as_str()).collect();
    assert_eq!(
        kinds,
        [
            "organ",
            "aquarium",
            "greenhouse",
            "lighthouse",
            "clock",
            "ice",
            "radio",
            "pagoda",
            "console",
            "hotel"
        ]
    );
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(kinds.len(), 10);
    for spec in SPECS {
        assert!(spec.lights.len() + usize::from(spec.extra == Extra::Beam) <= MAX_LIGHTS);
        assert!(spec.lights.iter().all(|f| (0.0..=1.0).contains(f)));
        assert!(Showcase::new(spec.kind).is_some());
    }
    // Les neuf immeubles de J7-08 ne sont pas des vitrines à façade.
    assert!(Showcase::new(LandmarkKind::Board).is_none());
}

#[test]
fn la_scene_construit_le_bon_type_pour_chaque_immeuble() {
    let stage = crate::landmarks::LandmarkStage::new();
    for kind in LandmarkKind::ALL {
        assert_eq!(stage.landmark(kind).kind(), kind);
    }
}

#[test]
fn chaque_vitrine_a_ses_dimensions_de_sillage() {
    let sizes: Vec<(f32, f32, u32)> = SPECS.iter().map(|s| (s.span, s.height, s.seed)).collect();
    assert_eq!(
        sizes,
        [
            (44.0, 84.0, 1212),
            (42.0, 70.0, 3030),
            (40.0, 72.0, 4545),
            (20.0, 104.0, 5151),
            (34.0, 96.0, 6262),
            (38.0, 80.0, 7373),
            (24.0, 118.0, 8484),
            (32.0, 92.0, 9595),
            (40.0, 66.0, 1313),
            (36.0, 88.0, 2424),
        ]
    );
}

#[test]
fn la_pagode_fournit_son_volume_les_autres_ont_un_corps() {
    for spec in SPECS {
        let showcase = Showcase::new(spec.kind).expect("vitrine");
        let body = showcase.body();
        if spec.kind == LandmarkKind::Pagoda {
            assert!(body.is_none());
        } else {
            let body = body.expect("corps");
            assert_eq!(
                (body.height, body.depth, body.seed),
                (spec.height, spec.depth, spec.seed)
            );
        }
        assert_eq!(showcase.body_tint(), Some(spec.tint));
        assert!(!showcase.spans_road());
    }
}

#[test]
fn les_pieces_sont_la_facade_et_au_plus_une_construction_annexe() {
    for spec in SPECS {
        let parts = Showcase::new(spec.kind).expect("vitrine").parts();
        let expected = if spec.extra == Extra::None { 1 } else { 2 };
        assert_eq!(parts.len(), expected, "{:?}", spec.kind);
        assert_eq!(parts[0].look, spec.look);
        assert_eq!(
            parts[0].mesh,
            MeshSpec::Quad {
                w: spec.span,
                h: spec.height
            }
        );
    }
}

#[test]
fn la_facade_est_posee_a_vingt_centimetres_devant_le_corps() {
    let frame = LandmarkFrame::rest();
    for spec in SPECS {
        let mut rig = Rig::new(spec.kind);
        rig.step(&frame);
        let paint = rig.paint(&frame);
        assert_eq!(paint[0].transform.translation, Vec3::new(0.0, spec.height / 2.0, 0.2));
    }
}

#[test]
fn les_lumieres_de_ville_sont_aux_hauteurs_de_la_fiche() {
    let frame = LandmarkFrame::rest();
    for spec in SPECS {
        let mut rig = Rig::new(spec.kind);
        for _ in 0..120 {
            rig.step(&frame);
        }
        let mut expected: Vec<f64> = spec.lights.iter().map(|f| f64::from(spec.height * f)).collect();
        if spec.extra == Extra::Beam {
            expected.push(f64::from(LAMP - 8.0));
        }
        let mut got = rig.lit();
        got.sort_by(f64::total_cmp);
        expected.sort_by(f64::total_cmp);
        assert_eq!(got.len(), expected.len(), "{:?}", spec.kind);
        for (g, e) in got.iter().zip(&expected) {
            assert!((g - e).abs() < 1e-3, "{:?} : {g} ≠ {e}", spec.kind);
        }
    }
}

#[test]
fn le_drop_passe_par_le_limiteur_de_flashs() {
    let mut frame = LandmarkFrame::rest();
    // Flashs interdits : le drop ne rend rien (règle 7).
    let mut rig = Rig::new(LandmarkKind::Hotel);
    frame.drive.drop = true;
    rig.step(&frame);
    frame.drive.drop = false;
    for _ in 0..30 {
        rig.step(&frame);
    }
    assert_eq!(rig.showcase.drop_level(), 0.0);
    assert_eq!(rig.paint(&frame)[0].a.w, 0.0);

    // Flashs permis : le niveau monte, reste borné à 1 et retombe.
    frame.policy = permitted();
    frame.time = 10.0;
    frame.drive.drop = true;
    let mut peak = 0.0_f32;
    for i in 0..60 {
        rig.step(&frame);
        frame.drive.drop = false;
        frame.time += f64::from(frame.dt);
        peak = peak.max(rig.showcase.drop_level());
        assert!(rig.showcase.drop_level() <= 1.0, "image {i}");
    }
    assert!(peak > 0.5, "le drop doit se voir quand les flashs sont permis ({peak})");
    for _ in 0..240 {
        rig.step(&frame);
    }
    assert!(rig.showcase.drop_level() < 0.05, "le niveau retombe");
}

#[test]
fn le_niveau_de_drop_ne_varie_jamais_plus_vite_que_le_limiteur() {
    let mut frame = LandmarkFrame::rest();
    frame.policy = permitted();
    let mut rig = Rig::new(LandmarkKind::Console);
    let mut last = 0.0_f32;
    let mut flips = 0;
    for i in 0..600 {
        // Un drop à chaque image : le limiteur doit refuser presque tout.
        frame.drive.drop = true;
        frame.time = f64::from(i) * f64::from(frame.dt);
        rig.step(&frame);
        let level = rig.showcase.drop_level();
        if (level - last).abs() > 0.5 {
            flips += 1;
        }
        last = level;
    }
    assert!(flips <= 3 * 10, "{flips} bascules en 10 s");
}

#[test]
fn les_horloges_integrees_ne_sautent_pas_quand_l_energie_change() {
    // Le rythme maximal de chaque horloge (basses et intensité à 1), par seconde.
    for (kind, max_rate) in [
        (LandmarkKind::Aquarium, 4.5),
        (LandmarkKind::Greenhouse, 3.2),
        (LandmarkKind::Clock, 0.75),
    ] {
        let mut frame = LandmarkFrame::rest();
        let mut rig = Rig::new(kind);
        let mut previous = 0.0_f32;
        let mut max_step = 0.0_f32;
        for i in 0..240 {
            // Des basses en dents de scie : avec `time × énergie` le motif sauterait de plusieurs mètres à chaque inversion.
            frame.beat.bass = if i % 2 == 0 { 1.0 } else { 0.0 };
            frame.drive.intensity = if i % 3 == 0 { 1.0 } else { 0.0 };
            rig.step(&frame);
            let clock = rig.paint(&frame)[0].b.w;
            max_step = max_step.max((clock - previous).abs());
            previous = clock;
        }
        assert!(max_step < max_rate / 60.0 + 1e-4, "{kind:?} : saut de {max_step}");
        assert!(previous > 0.0, "{kind:?} : l'horloge avance");
    }
}

#[test]
fn les_aiguilles_s_emballent_au_drop_sans_sauter() {
    let mut frame = LandmarkFrame::rest();
    frame.policy = permitted();
    let mut rig = Rig::new(LandmarkKind::Clock);
    for _ in 0..60 {
        rig.step(&frame);
    }
    let calm = rig.paint(&frame)[0].c.w;
    frame.drive.drop = true;
    let mut previous = calm;
    let mut max_step = 0.0_f32;
    for _ in 0..30 {
        rig.step(&frame);
        frame.drive.drop = false;
        let hands = rig.paint(&frame)[0].c.w;
        max_step = max_step.max(hands - previous);
        previous = hands;
    }
    assert!(
        max_step <= 7.0 / 60.0 + 1e-4,
        "pas plus de sept fois le rythme normal ({max_step})"
    );
    assert!(max_step > 1.5 / 60.0, "les aiguilles accélèrent ({max_step})");
}

#[test]
fn le_phare_balaye_et_sa_lumiere_suit_le_bout_du_faisceau() {
    let frame = LandmarkFrame::rest();
    let mut rig = Rig::new(LandmarkKind::Lighthouse);
    rig.step(&frame);
    let first = rig.showcase.beam_angle();
    for _ in 0..60 {
        rig.step(&frame);
    }
    let later = rig.showcase.beam_angle();
    assert_ne!(first, later);
    assert!((0.0..std::f32::consts::TAU).contains(&later));
    let paint = rig.paint(&frame);
    assert_eq!(paint.len(), 2);
    assert_eq!(paint[1].transform.translation, Vec3::new(0.0, LAMP + 1.5, -20.0 * 0.4));
    assert!(paint[1].a.w > 0.8, "puissance du faisceau");
    // La lumière 2 est à LAMP − 8 m, à 55 m de l'axe de la lanterne.
    let tip = rig.field.lights()[2];
    assert!((tip.position.y - f64::from(LAMP - 8.0)).abs() < 1e-3);
}

#[test]
fn le_mode_calme_ralentit_le_phare_et_le_fait_partir_de_zero() {
    let mut calm = LandmarkFrame::rest();
    calm.calm = true;
    let normal = LandmarkFrame::rest();
    let mut a = Rig::new(LandmarkKind::Lighthouse);
    let mut b = Rig::new(LandmarkKind::Lighthouse);
    for _ in 0..60 {
        a.step(&calm);
        b.step(&normal);
    }
    let travelled_calm = a.showcase.beam_angle();
    // Départ à 0, puis (0,35 + 0,4 × 0,7) rad/s : le mode calme tourne deux fois moins vite que le mode normal.
    assert!((travelled_calm - 0.63).abs() < 0.02, "{travelled_calm}");
    assert!((b.showcase.beam_angle() - travelled_calm).abs() > 0.1);
}

#[test]
fn la_tour_radio_envoie_ses_ondes_depuis_la_pointe() {
    let mut frame = LandmarkFrame::rest();
    frame.policy = permitted();
    frame.drive.intensity = 1.0;
    let mut rig = Rig::new(LandmarkKind::Radio);
    frame.drive.drop = true;
    rig.step(&frame);
    frame.drive.drop = false;
    for _ in 0..12 {
        rig.step(&frame);
    }
    let paint = rig.paint(&frame);
    assert_eq!(paint.len(), 2);
    assert_eq!(paint[1].transform.translation, Vec3::new(0.0, RADIO_TOP, 4.0));
    assert!(paint[1].a.w > 0.0 && paint[1].a.w <= 1.0, "onde de choc bornée");
    let parts = rig.showcase.parts();
    assert_eq!(parts[1].mesh, MeshSpec::Quad { w: WAVES, h: WAVES });
}

#[test]
fn le_faisceau_est_deux_plans_en_croix_de_cent_cinquante_metres() {
    let mesh = beam_mesh();
    assert_eq!(mesh.vertex_count(), 8);
    assert_eq!(mesh.indices.len(), 12);
    let (lo, hi) = mesh.bounds().expect("sommets");
    assert_eq!((lo.x, hi.x), (0.0, BEAM));
    assert!(hi.y > 0.0 && hi.z > 0.0 && lo.y < 0.0 && lo.z < 0.0);
    assert!(
        mesh.uvs
            .iter()
            .all(|uv| (0.0..=1.0).contains(&uv[0]) && (0.0..=1.0).contains(&uv[1]))
    );
}

#[test]
fn la_pagode_a_seize_blocs_dont_le_premier_etage_plonge_sous_le_sol() {
    let mesh = pagoda_mesh();
    assert_eq!(mesh.vertex_count(), BLOCKS * 24);
    assert_eq!(mesh.normals.len(), mesh.vertex_count());
    assert_eq!(mesh.indices.len(), BLOCKS * 36);
    let (lo, hi) = mesh.bounds().expect("sommets");
    assert!((lo.y - (-4.0)).abs() < 1e-4, "le pied plonge de 4 m ({})", lo.y);
    assert!(
        (hi.y - (4.0 + 5.0 * 14.0 + 12.0)).abs() < 1e-4,
        "la flèche culmine à 86 m ({})",
        hi.y
    );
    // Le plus large est le toit du premier étage : 30 + 2 × 4,5 m.
    assert!((hi.x - 19.5).abs() < 1e-4 && (lo.x + 19.5).abs() < 1e-4);
    // Tout est du côté de la route de −Z (le corps s'étend derrière la façade).
    assert!(hi.z <= 0.1 + 1e-4 || hi.z < 5.0);
    assert_eq!(pagoda_mesh(), mesh);
}

#[test]
fn la_teinte_et_les_couleurs_viennent_de_la_palette() {
    let palette = cf_render::ThemeLibrary::builtin()
        .map(|l| *l.preset(cf_show::model::PresetTheme::Velvet))
        .unwrap_or_default();
    let mut frame = LandmarkFrame::rest();
    frame.colors = Colors::of(&palette);
    let mut rig = Rig::new(LandmarkKind::Aquarium);
    rig.step(&frame);
    let paint = rig.paint(&frame);
    let spec = &SPECS[1];
    let expected = |c: SceneColor| frame.colors.get(c);
    assert_eq!(paint[0].a.truncate(), expected(spec.colors[0]));
    assert_eq!(paint[0].b.truncate(), expected(spec.colors[1]));
    assert_eq!(paint[0].c.truncate(), expected(spec.colors[2]));
    assert_eq!(paint[0].d.truncate(), expected(spec.colors[3]));
    assert_eq!(paint[0].mood, expected(spec.tint));
    assert_ne!(expected(SceneColor::Mint), expected(SceneColor::Rose));
}
