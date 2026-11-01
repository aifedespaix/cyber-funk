//! Tests des réglages de l'opérateur et du repli en qualité basse. Pas de banc GPU : on vérifie les uniformes écrits et le
//! texte du shader.

use bevy::math::Vec4;
use cf_render::{FacadeTuning, ScenePalette};

use super::material::FACADE_SOURCE;
use super::systems::{FrameInputs, LOW_QUALITY_FADE_S, LowQualityMix, build_params, trail_for_quality};
use super::{CarTrail, FacadeLook, TRAIL_POINTS};

fn params_of(look: &FacadeLook, frame: &FrameInputs) -> super::FacadeParams {
    build_params(&ScenePalette::default(), look, 0.0, 0.0, frame)
}

#[test]
fn look_defaults_are_one() {
    let look = FacadeLook::default();
    assert_eq!(look.car_follow, 1.0);
    assert_eq!(look.window_life, 1.0);
    let tuning = FacadeTuning::default();
    assert_eq!((tuning.car_follow, tuning.window_life), (1.0, 1.0));
    // Réglages par défaut : le shader reçoit la part de vie des fenêtres à 1 et ne coupe aucun écran.
    let p = params_of(&look, &FrameInputs::default());
    assert_eq!(p.motion.z, 1.0);
    assert_eq!(p.motion.w, 0.0);
}

#[test]
fn window_life_is_clamped_and_passed_through() {
    let look = FacadeLook {
        window_life: 0.25,
        ..FacadeLook::default()
    };
    assert_eq!(params_of(&look, &FrameInputs::default()).motion.z, 0.25);
    for (given, wanted) in [(2.0, 1.0), (-1.0, 0.0), (f32::NAN, 0.0)] {
        let look = FacadeLook {
            window_life: given,
            ..FacadeLook::default()
        };
        assert_eq!(params_of(&look, &FrameInputs::default()).motion.z, wanted, "{given}");
    }
}

#[test]
fn low_quality_disables_costly_effects() {
    let busy = FrameInputs {
        motion_sweep: 0.7,
        motion_flicker: 1.0,
        ..FrameInputs::default()
    };
    let high = params_of(&FacadeLook::default(), &busy);
    assert_eq!(high.motion, Vec4::new(0.7, 1.0, 1.0, 0.0), "qualité haute inchangée");

    let low = params_of(&FacadeLook::default(), &FrameInputs { low_mix: 1.0, ..busy });
    assert_eq!(low.motion.y, 0.0, "plus de scintillement rapide");
    assert_eq!(low.motion.z, 0.0, "plus de vie des fenêtres");
    assert_eq!(low.motion.w, 1.0, "écrans qui défilent coupés");
    assert_eq!(low.motion.x, 0.7, "la voie limitée (sweep) est transmise telle quelle");
    assert_eq!(low.trail, [Vec4::ZERO; TRAIL_POINTS]);
    assert_eq!(low.car_tint.w, 0.0);

    // La traîne est remplie après `build_params` : le repli la remet à zéro même avec une voiture qui roule.
    let mut trail = CarTrail::default();
    trail.record(bevy::math::DVec3::new(0.0, 0.0, -10.0), 0.0);
    trail.record(bevy::math::DVec3::new(0.0, 0.0, -30.0), 0.5);
    trail.set_tint(bevy::math::Vec3::ONE, 1.0);
    let (points, tint) = trail_for_quality(&trail, None, 0.6, 1.0);
    assert_eq!(points, [Vec4::ZERO; TRAIL_POINTS]);
    assert_eq!(tint, Vec4::ZERO);
}

#[test]
fn shader_honours_the_new_uniform_slots() {
    assert!(
        FACADE_SOURCE.contains("temper_b.z * facade.motion.z"),
        "vie des fenêtres"
    );
    assert!(
        FACADE_SOURCE.contains("mix(screen, m, facade.motion.w)"),
        "écrans fondus en qualité basse"
    );
    assert!(
        FACADE_SOURCE.contains("(glow + 0.55 * (motion - 1.0))"),
        "gain de mouvement sur la base stable seulement"
    );
}

#[test]
fn legacy_looks_still_parse() {
    // Les looks ne passent pas par `FacadeLook` (ressource d'exécution) : les nouveaux champs n'entrent pas dans le RON.
    // Ce test garde la promesse de la spec : tous les fichiers existants se chargent avec le chargeur réel.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/looks");
    let library = cf_show::load::LookLibrary::load_dir(&dir).expect("looks lisibles");
    let levels = std::fs::read_dir(&dir)
        .expect("dossier des looks")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("niveau-"))
        .count();
    assert_eq!(levels, 6);
    assert!(library.len() >= levels);
}

const DT: f32 = 1.0 / 60.0;

#[test]
fn low_quality_ramp_is_monotonic_and_bounded() {
    let mut mix = LowQualityMix::default();
    let mut last = 0.0_f32;
    let mut frames = 0;
    while last < 1.0 {
        let now = mix.update(DT, true);
        assert!(now >= last, "montée monotone");
        assert!(
            now - last <= DT / LOW_QUALITY_FADE_S + 1e-6,
            "pas par image borné à 1/90 : {}",
            now - last
        );
        last = now;
        frames += 1;
        assert!(frames < 200);
    }
    assert!(frames <= 91, "entièrement bas après 1,5 s ({frames} images)");
    // Retour en qualité haute : redescente monotone, mêmes bornes, retour complet.
    while last > 0.0 {
        let now = mix.update(DT, false);
        assert!(now <= last && last - now <= DT / LOW_QUALITY_FADE_S + 1e-6);
        last = now;
    }
    assert_eq!(mix.update(DT, false), 0.0);
    // Une image très longue ne fait pas sauter le repli : au plus 0,1 s d'avance.
    assert!(mix.update(5.0, true) <= 0.1 / LOW_QUALITY_FADE_S + 1e-6);
}

#[test]
fn low_mix_scales_effects_continuously() {
    let busy = FrameInputs {
        motion_flicker: 1.0,
        ..FrameInputs::default()
    };
    let half = params_of(&FacadeLook::default(), &FrameInputs { low_mix: 0.5, ..busy });
    assert_eq!(half.motion, Vec4::new(0.0, 0.5, 0.5, 0.5));
    assert_eq!(
        params_of(&FacadeLook::default(), &FrameInputs { low_mix: 0.0, ..busy }),
        params_of(&FacadeLook::default(), &busy),
        "qualité haute inchangée"
    );
}
