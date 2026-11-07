#![allow(
    clippy::unwrap_used,
    reason = "tests : un échec d'aide doit faire échouer le test bruyamment"
)]

use cf_render::{CarOption, CarSelection, DistrictOrder, DistrictState, GradeLibrary};
use cf_show::district::{DistrictId, DistrictKind, StayReason, TransitionKind};
use cf_show::persist::{AppSettings, CarChoice};

use super::districts::{
    DistrictGestures, DistrictPick, TRANSITIONS, district_label, district_orders, fallback_note, fresh_fade,
    reason_label, variant_count,
};
use super::dock::choose_car;
use super::persist::{family_is_open, migrate_legacy, section_is_open, set_family_open, set_section_open};
use super::{DebugState, Section};
use crate::OperatorMode;

// --- Voiture ---

fn selection() -> CarSelection {
    CarSelection {
        options: vec![
            CarOption {
                key: "dirty",
                label: "Dirty car",
                ready: true,
            },
            CarOption {
                key: "cyber",
                label: "CyberCar",
                ready: true,
            },
        ],
        active: "dirty",
        requested: "dirty",
    }
}

#[test]
fn choisir_la_voiture_en_place_n_ecrit_rien() {
    let mut selection = selection();
    let mut settings = AppSettings::default();
    assert!(!choose_car(&mut selection, &mut settings, "dirty"));
    assert_eq!(settings, AppSettings::default());
}

#[test]
fn choisir_une_autre_voiture_la_demande_et_la_retient() {
    let mut selection = selection();
    let mut settings = AppSettings::default();
    assert!(choose_car(&mut selection, &mut settings, "cyber"));
    assert_eq!(selection.requested, "cyber");
    assert_eq!(settings.car, CarChoice::Cyber);
}

// --- Préférences ---

#[test]
fn les_familles_ouvertes_se_retiennent_sans_doublon() {
    let mut settings = AppSettings::default();
    set_family_open(&mut settings, "Trafic", true);
    set_family_open(&mut settings, "Trafic", true);
    assert!(family_is_open(&settings, "Trafic"));
    // Retenues dans la structure typée, plus dans `collapsed_panels`.
    assert_eq!(settings.debug_panel.open_families.len(), 1);
    assert_eq!(settings.collapsed_panels, Vec::<String>::new());
    set_family_open(&mut settings, "Trafic", false);
    assert!(!family_is_open(&settings, "Trafic"));
}

#[test]
fn les_sections_sont_ouvertes_par_defaut_et_se_retiennent() {
    let mut settings = AppSettings::default();
    for section in Section::ALL {
        assert!(section_is_open(&settings, section));
    }
    set_section_open(&mut settings, Section::Car, false);
    assert!(!section_is_open(&settings, Section::Car));
    assert!(section_is_open(&settings, Section::Facades));
    set_section_open(&mut settings, Section::Car, true);
    assert_eq!(settings, AppSettings::default());
}

#[test]
fn migration_des_cles() {
    let mut settings = AppSettings {
        collapsed_panels: vec!["debug-open:Trafic".into(), "tuning:car-follow:50".into()],
        ..AppSettings::default()
    };
    let tuning_before = cf_render::tuning_from_entries(&settings.collapsed_panels);
    assert!((tuning_before.car_follow - 0.5).abs() < 1e-6);
    // Lecture : l'ancienne clé suffit.
    assert!(family_is_open(&settings, "Trafic"));
    assert!(!family_is_open(&settings, "Immeubles"));
    // Migration : la famille passe dans la structure typée, le curseur reste où cf-render le lit.
    assert!(migrate_legacy(&mut settings));
    assert!(!migrate_legacy(&mut settings), "rien à migrer deux fois");
    assert!(family_is_open(&settings, "Trafic"));
    assert_eq!(settings.collapsed_panels, ["tuning:car-follow:50"]);
    assert_eq!(
        cf_render::tuning_from_entries(&settings.collapsed_panels),
        tuning_before
    );
    // Fermer une famille venue de l'ancienne clé tient : l'ancienne entrée ne la rouvre pas.
    let mut legacy = AppSettings {
        collapsed_panels: vec!["debug-open:Trafic".into()],
        ..AppSettings::default()
    };
    set_family_open(&mut legacy, "Trafic", false);
    assert!(!family_is_open(&legacy, "Trafic"));
    assert_eq!(legacy.collapsed_panels, Vec::<String>::new());
}

#[test]
fn un_ancien_fichier_de_reglages_se_relit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    std::fs::write(
        &path,
        "(version: 1, car: \"cyber\", collapsed_panels: [\"debug-open:Trafic\", \"tuning:window-life:25\"])",
    )
    .unwrap();
    let settings = AppSettings::load(&path);
    assert_eq!(settings.car, CarChoice::Cyber);
    assert!(family_is_open(&settings, "Trafic"));
    assert!((cf_render::tuning_from_entries(&settings.collapsed_panels).window_life - 0.25).abs() < 1e-6);
    assert!(section_is_open(&settings, Section::Car));
}

#[test]
fn gel_et_forcage_non_sauvegardes() {
    // Une session : un quartier choisi dans le panneau (il vit dans `DebugState`, hors des réglages).
    let session = DebugState {
        district: DistrictPick {
            kind: DistrictKind::Residential,
            variant: 3,
            transition: TransitionKind::Fog,
        },
        ..DebugState::default()
    };
    assert_ne!(session.district, DistrictPick::default());
    let mut settings = AppSettings::default();
    set_section_open(&mut settings, Section::Facades, false);
    set_family_open(&mut settings, "Trafic", true);

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.ron");
    settings.save(&path).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    for word in ["frozen", "Frozen", "force", "Force", "target", "Residential", "Fog"] {
        assert!(!text.contains(word), "« {word} » ne doit pas être sauvegardé : {text}");
    }
    let reloaded = AppSettings::load(&path);
    assert_eq!(reloaded, settings);
    // La sélection du panneau repart de zéro à chaque session.
    assert_eq!(DebugState::default().district, DistrictPick::default());
}

// --- Quartiers ---

#[test]
fn section_quartiers_envoie_les_bons_ordres() {
    let pick = DistrictPick {
        kind: DistrictKind::Suburb,
        variant: 2,
        transition: TransitionKind::Bridge,
    };
    let force = DistrictGestures {
        force: true,
        ..DistrictGestures::default()
    };
    let orders: Vec<_> = district_orders(OperatorMode::Preparation, &pick, &force).collect();
    assert_eq!(
        orders,
        [DistrictOrder::Force {
            to: DistrictId {
                kind: DistrictKind::Suburb,
                variant: 2,
            },
            transition: TransitionKind::Bridge,
        }]
    );
    for frozen in [true, false] {
        let gesture = DistrictGestures {
            frozen: Some(frozen),
            ..DistrictGestures::default()
        };
        let orders: Vec<_> = district_orders(OperatorMode::Preparation, &pick, &gesture).collect();
        assert_eq!(orders, [DistrictOrder::SetFrozen(frozen)]);
    }
    let fade = DistrictGestures {
        fade_s: Some(4.0),
        ..DistrictGestures::default()
    };
    let orders: Vec<_> = district_orders(OperatorMode::Preparation, &pick, &fade).collect();
    assert_eq!(orders, [DistrictOrder::SetFadeSeconds(4.0)]);
    // Sans geste, rien.
    assert_eq!(
        district_orders(OperatorMode::Preparation, &pick, &DistrictGestures::default()).count(),
        0
    );
}

#[test]
fn le_curseur_de_duree_n_envoie_que_sur_un_changement() {
    assert_eq!(fresh_fade(10.0, 10.0), None);
    assert_eq!(fresh_fade(10.0, 4.0), Some(4.0));
    assert_eq!(fresh_fade(5.0, 40.0), Some(10.0), "borné à 1..=10 s");
    assert_eq!(fresh_fade(10.0, 40.0), None, "déjà à la borne");
    assert_eq!(fresh_fade(3.0, 0.0), Some(1.0));
}

#[test]
fn rien_hors_preparation() {
    let pick = DistrictPick::default();
    let all = DistrictGestures {
        force: true,
        frozen: Some(true),
        fade_s: Some(3.0),
    };
    assert_eq!(district_orders(OperatorMode::Live, &pick, &all).count(), 0);
    assert_eq!(district_orders(OperatorMode::Preparation, &pick, &all).count(), 3);
}

#[test]
fn repli_affiche() {
    assert_eq!(TRANSITIONS.len(), 7);
    for (i, a) in TRANSITIONS.iter().enumerate() {
        assert!(TRANSITIONS[i + 1..].iter().all(|b| b != a), "transitions distinctes");
    }
    assert_eq!(fallback_note(TransitionKind::Gradient), None);
    for transition in TRANSITIONS {
        if transition != TransitionKind::Gradient {
            assert_eq!(fallback_note(transition), Some("(repli : dégradé)"), "{transition:?}");
        }
    }
}

#[test]
fn etat_affiche_raison() {
    let state = DistrictState {
        last_reason: StayReason::DropImminent,
        ..DistrictState::default()
    };
    assert_eq!(reason_label(&state), "bloqué : drop imminent");
    for reason in [
        StayReason::NoShow,
        StayReason::TooSoon,
        StayReason::PeakSection,
        StayReason::DropImminent,
        StayReason::AfterDrop,
        StayReason::NoWindow,
        StayReason::Transitioning,
        StayReason::Frozen,
    ] {
        let state = DistrictState {
            last_reason: reason,
            ..DistrictState::default()
        };
        assert!(!reason_label(&state).is_empty(), "{reason:?}");
    }
}

#[test]
fn le_quartier_s_affiche_avec_sa_variante_comptee_a_partir_de_un() {
    let id = DistrictId {
        kind: DistrictKind::Suburb,
        variant: 0,
    };
    assert_eq!(district_label(id), "Banlieue, variante 1");
}

#[test]
fn le_nombre_de_variantes_vient_de_la_bibliotheque_avec_un_repli_sur() {
    for kind in DistrictKind::ALL {
        assert_eq!(variant_count(None, kind), 1);
    }
    let library = GradeLibrary::builtin().unwrap();
    for kind in DistrictKind::ALL {
        assert_eq!(variant_count(Some(&library), kind), library.variants(kind).max(1));
    }
    let empty = GradeLibrary::default();
    assert_eq!(variant_count(Some(&empty), DistrictKind::Neon), 1);
}

#[test]
fn changer_de_type_ramene_la_variante_dans_les_bornes() {
    let mut pick = DistrictPick {
        kind: DistrictKind::Neon,
        variant: 7,
        transition: TransitionKind::Gradient,
    };
    pick.clamp_variant(3);
    assert_eq!(pick.variant, 2);
    pick.clamp_variant(0);
    assert_eq!(pick.variant, 0);
}
