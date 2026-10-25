//! Tests de `adapter`.

#![allow(clippy::expect_used)]

use super::*;
use crate::traffic::event::TrafficEvent;
use crate::traffic::events::test_rig::Rig;
use crate::traffic::grand::{LandmarkDebug, LandmarkEvent};
use bevy::math::DVec3;

fn begin(rig: &mut Rig, handle: &LandmarkHandle, kind: LandmarkKind, debug: Option<LandmarkDebug>) -> LandmarkEvent {
    rig.start(|ctx| LandmarkEvent::new(ctx, Box::new(StageLandmark::of(handle, kind)), debug, None))
}

#[test]
fn l_immeuble_est_pose_devant_la_voiture_avec_sa_zone_degagee_et_son_sujet() {
    for kind in LandmarkKind::ALL {
        let handle = LandmarkHandle::new();
        let mut rig = Rig::new(3);
        let ev = begin(&mut rig, &handle, kind, None);
        assert!(ev.structure());
        let span = f64::from(handle.lock().landmark(kind).span());
        let clearing = ev.clearing().expect("zone");
        assert!(
            clearing.z1 < 0.0 && (clearing.z1 - clearing.z0 - (span + 28.0)).abs() < 1e-6,
            "{}",
            kind.as_str()
        );
        let spans_road = handle.lock().landmark(kind).spans_road();
        assert_eq!(clearing.side == 0, spans_road, "{}", kind.as_str());
        let focus = ev.focus().expect("sujet");
        assert!((f64::from(focus.size) - span).abs() < 1e-3);
        assert!((focus.position.y - span * 0.4).abs() < 1e-3);
        assert_eq!(ev.obstruction().is_some(), spans_road, "{}", kind.as_str());
        let active = handle.lock().active().copied().expect("posé");
        assert_eq!(active.kind, kind);
        // L'ancre du sujet est la racine de l'immeuble : au bord de la route, ou au centre pour une arche.
        let expected_x = if spans_road {
            0.0
        } else {
            f64::from(active.place.side) * 17.0
        };
        assert!((focus.position.x - expected_x).abs() < 1e-6);
    }
}

#[test]
fn l_enseigne_phare_apparait_plus_loin_et_le_corps_prend_la_couleur_de_l_immeuble() {
    let handle = LandmarkHandle::new();
    let mut rig = Rig::new(3);
    let _ev = begin(&mut rig, &handle, LandmarkKind::Error, None);
    let active = handle.lock().active().copied().expect("posé");
    let span = f64::from(handle.lock().landmark(LandmarkKind::Error).span());
    assert!((active.place.z - (-620.0 - span / 2.0)).abs() < 1e-9);
    let alarm = tint_rank(cf_render::SceneColor::Alarm).map(|r| r as u8);
    assert_eq!(Some(active.place.tint), alarm);
}

#[test]
fn la_mise_au_point_fixe_le_centre_et_l_echelle_est_celle_du_passage() {
    let handle = LandmarkHandle::new();
    let mut rig = Rig::new(3);
    let ev = begin(
        &mut rig,
        &handle,
        LandmarkKind::Board,
        Some(LandmarkDebug {
            center_m: 120.0,
            side: -1,
        }),
    );
    let active = handle.lock().active().copied().expect("posé");
    assert!((active.place.z + 120.0).abs() < 1e-9 && active.place.side == -1);
    assert_eq!(ev.clearing().map(|c| c.side), Some(-1));
    // L'échelle d'un passage de spectacle atteint la scène.
    let mut stage = handle.lock();
    stage.set_scale(1.1);
    assert!((stage.active().expect("posé").place.scale - 1.1).abs() < f32::EPSILON);
}

#[test]
fn le_passage_se_termine_apres_l_immeuble_et_la_scene_est_rangee() {
    let handle = LandmarkHandle::new();
    let mut rig = Rig::new(3);
    let mut ev = begin(&mut rig, &handle, LandmarkKind::Lava, None);
    let mut frames = 0;
    while rig.step(&mut ev, 1.0 / 60.0, |_| {}) {
        frames += 1;
        assert!(frames < 60 * 60);
        // À chaque image, l'immeuble écrit la pose de ses pièces.
        assert!(handle.lock().paint(LandmarkKind::Lava)[0].transform.translation.y > 0.0);
    }
    rig.end(&mut ev);
    assert!(handle.lock().active().is_none());
    assert!(handle.lock().field.lights().iter().all(|l| l.color.length() == 0.0));
}

#[test]
fn le_tirage_a_la_fabrication_respecte_le_mode_calme_et_la_mise_au_point() {
    let handle = LandmarkHandle::new();
    let mut rig = Rig::new(7);
    for _ in 0..100 {
        let kind = rig.start(|ctx| StageLandmark::choose(&handle, ctx)).kind();
        assert!(LandmarkKind::ALL.contains(&kind));
    }
    handle.force(LandmarkKind::Mecha);
    assert_eq!(
        rig.start(|ctx| StageLandmark::choose(&handle, ctx)).kind(),
        LandmarkKind::Mecha
    );
    // L'ordre est consommé.
    let mut seen_other = false;
    for _ in 0..50 {
        seen_other |= rig.start(|ctx| StageLandmark::choose(&handle, ctx)).kind() != LandmarkKind::Mecha;
    }
    assert!(seen_other);
}

/// Une image de pire cas : un kick tous les quarts de seconde (la cadence maximale du limiteur), des drops fréquents.
fn worst_case(i: u32) -> TrafficLandmarkFrame {
    TrafficLandmarkFrame {
        time: f64::from(i) / 60.0,
        dt: 1.0 / 60.0,
        beat: TrafficBeat {
            kick: i.is_multiple_of(15),
            bass: 1.0,
            high: if i.is_multiple_of(2) { 1.0 } else { 0.0 },
            pulse: if i % 15 < 3 { 1.0 } else { 0.0 },
            count: i / 15,
            bpm: Some(240.0),
            ..TrafficBeat::SILENT
        },
        drive: crate::traffic::TrafficDrive {
            drop: i % 120 == 60,
            intensity: 1.0,
            section: cf_analysis::Section::Peak,
        },
        car: DVec3::ZERO,
        calm: false,
        phase: 0.5,
    }
}

#[test]
fn au_repos_le_pouls_que_lisent_les_shaders_reste_a_zero_pour_chaque_immeuble() {
    // Pas de spectacle, ou flashs coupés par le look : rien ne brille d'un coup (règle 7).
    for policy in [
        FlashPolicy::NONE,
        FlashPolicy {
            flashes: false,
            breathing: false,
        },
        FlashPolicy {
            flashes: true,
            breathing: true,
        },
    ] {
        for kind in LandmarkKind::ALL {
            let handle = LandmarkHandle::new();
            let mut landmark = StageLandmark::of(&handle, kind);
            landmark.show(-100.0, 1, LinearRgba::WHITE);
            for i in 0..600 {
                landmark.drive(&worst_case(i), policy, None);
                let pulse = handle.lock().uniform().pulse;
                assert!(pulse.abs() < f32::EPSILON, "{} : pouls {pulse}", kind.as_str());
            }
        }
    }
}

#[test]
fn permis_le_pouls_borne_ne_varie_jamais_plus_vite_que_min_fade_et_reste_sous_trois_departs_par_seconde() {
    let live = FlashPolicy {
        flashes: true,
        breathing: false,
    };
    for kind in LandmarkKind::ALL {
        let handle = LandmarkHandle::new();
        let mut landmark = StageLandmark::of(&handle, kind);
        landmark.show(-100.0, 1, LinearRgba::WHITE);
        let (mut last, mut starts) = (0.0_f32, Vec::new());
        for i in 0..1800 {
            landmark.drive(&worst_case(i), live, None);
            let pulse = handle.lock().uniform().pulse;
            assert!(
                (pulse - last).abs() <= (1.0 / 60.0) / cf_show::limiter::MIN_FADE + 1e-3,
                "{} : saut de {}",
                kind.as_str(),
                (pulse - last).abs()
            );
            if last < 0.5 && pulse >= 0.5 {
                starts.push(i);
            }
            last = pulse;
        }
        for &t in &starts {
            let in_second = starts.iter().filter(|&&u| u >= t && u < t + 60).count();
            assert!(in_second <= 3, "{} : {in_second} départs en 1 s", kind.as_str());
        }
        assert!(
            !starts.is_empty(),
            "{} : le pouls part quand c'est permis",
            kind.as_str()
        );
    }
}

#[test]
fn le_pouls_est_celui_que_le_trafic_accorde_a_toute_la_ville() {
    let handle = LandmarkHandle::new();
    let mut rig = Rig::new(3);
    let _ev = begin(&mut rig, &handle, LandmarkKind::Binary, None);
    let mut landmark = StageLandmark::of(&handle, LandmarkKind::Binary);
    let frame = TrafficLandmarkFrame {
        time: 1.0,
        dt: 1.0 / 60.0,
        beat: TrafficBeat {
            pulse: 1.0,
            ..TrafficBeat::SILENT
        },
        drive: crate::traffic::TrafficDrive::START,
        car: DVec3::ZERO,
        calm: false,
        phase: 0.5,
    };
    // Pouls accordé nul (flashs coupés) : rien ne brille, quel que soit le pouls brut de l'analyse.
    landmark.drive(&frame, FlashPolicy::NONE, Some(0.0));
    assert!(handle.lock().uniform().pulse.abs() < f32::EPSILON);
    landmark.drive(&frame, FlashPolicy::NONE, Some(0.6));
    assert!((handle.lock().uniform().pulse - 0.6).abs() < 1e-6);
}
