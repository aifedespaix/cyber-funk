//! Validité des plans : chaque fiche du catalogue, jouée 30 s simulées sur la soirée de [`crate::testkit`], ne produit
//! jamais de pose invalide (NaN, dans un mur, hors bornes).
//!
//! Deux niveaux. Le plan brut d'abord : il ne doit jamais produire de NaN. Puis le réalisateur, avec la fiche seule dans
//! le catalogue : c'est la pose qui part vers la caméra, et elle doit être hors des murs et dans les bornes
//! (`MAX_BEHIND`, `MAX_AHEAD`, `MAX_HEIGHT`). Un plan brut peut sortir de ces contraintes (la spirale rentre parfois
//! dans un immeuble, la vue de dieu dépasse la fenêtre arrière) : c'est le réalisateur, avec son garde anti-collision et
//! ses bornes, qui les ramène dans le cadre, comme dans Sillage.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use crate::director::CinemaDirector;
use crate::pose::is_finite;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::shots::catalog;
use crate::testkit::{BoxCity, DT, Sim, check_pose};
use crate::world::{CinemaFrame, CinemaWorld};

/// Plans qui ne jouent que sur une soirée scénarisée : leurs tests vivent avec leur lot (`tests_flight.rs` pour le
/// lot B, J6-04 ; `tests_world.rs` pour le lot C, J6-05).
const SCENARIO_SHOTS: &[&str] = &[
    // Lot B (J6-04)
    "takeoff",
    "touchdown",
    "dodge-pass",
    "barrel",
    "stunt-cam",
    "ascension-cam",
    "bullet-time",
    "fpv-dive",
    "city-wide",
    // Lot C (J6-05) : sujets du monde, joués par `tests_world.rs`
    "ad-glance",
    "pursuit",
    "bikers",
    "landmark-reveal",
    "meteor-ride",
    "ringworld",
    "moon-pass",
    "meteor-impact-watch",
    "dj-sign",
];

/// Trente secondes simulées, à la cadence de la soirée de test.
const FRAMES: usize = (30.0 / DT as f64) as usize;

/// La soirée de test : show actif (pour `dolly-zoom`), drop préparé, et un drop qui frappe à chaque sommet.
fn scene() -> Sim {
    let mut sim = Sim::new(true);
    sim.world.show.drop_planned = true;
    sim
}

/// Un véhicule du trafic devant la voiture (pour `ride-along`), recalé à chaque image.
fn subjects_for(world: &CinemaWorld) -> [Subject; 1] {
    [Subject {
        id: 1,
        kind: SubjectKind::Craft,
        position: world.car + DVec3::new(4.0, 45.0, -90.0),
        velocity: Vec3::new(0.0, 0.0, -30.0),
        size: 6.0,
        facing: None,
    }]
}

/// Un plan seul, pour le réalisateur : le catalogue complet est reconstruit, on garde la fiche demandée.
fn single(id: &str) -> Box<dyn ShotSpec> {
    let mut found: Vec<Box<dyn ShotSpec>> = catalog().into_iter().filter(|spec| spec.id() == id).collect();
    assert_eq!(found.len(), 1, "plan introuvable : {id}");
    found.swap_remove(0)
}

#[test]
fn le_catalogue_a_des_identifiants_uniques() {
    let specs = catalog();
    assert!(specs.len() >= 17, "le lot A compte 17 plans");
    let mut ids: Vec<&str> = specs.iter().map(|s| s.id()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), specs.len(), "identifiants en double");
    assert!(!ids.contains(&"strobe"), "le strobe est hors périmètre de J6-03");
}

#[test]
fn chaque_plan_brut_reste_fini_pendant_30_s() {
    for spec in catalog().into_iter().filter(|s| !SCENARIO_SHOTS.contains(&s.id())) {
        let id = spec.id();
        let mut sim = scene();
        let mut rng = CinemaRng::new(11);
        let mut shot: Option<Box<dyn Shot>> = None;
        let mut played = 0usize;
        for _ in 0..FRAMES {
            sim.step();
            let subjects = subjects_for(&sim.world);
            let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
            if shot.is_none() && spec.available(&frame) {
                shot = Some(spec.create(&frame, &mut rng));
            }
            let Some(current) = shot.as_mut() else {
                continue;
            };
            let mut pose = CameraPose::default();
            let alive = current.update(&frame, &mut pose);
            played += 1;
            assert!(is_finite(&pose), "{id} : pose non finie à t={}", frame.time);
            if !alive {
                shot = None;
            }
        }
        assert!(
            played > FRAMES / 2,
            "{id} : joué {played} images sur {FRAMES}, trop peu pour valider"
        );
    }
}

#[test]
fn chaque_plan_par_le_realisateur_reste_dans_les_bornes_pendant_30_s() {
    for spec in catalog().into_iter().filter(|s| !SCENARIO_SHOTS.contains(&s.id())) {
        let id = spec.id();
        let mut director = CinemaDirector::new(vec![spec], false, 7);
        let mut sim = scene();
        sim.step();
        let subjects = subjects_for(&sim.world);
        director.start(
            &CameraPose::default(),
            &CinemaFrame::new(&sim.world, &BoxCity, &subjects),
        );
        let mut on_air = 0usize;
        for _ in 0..FRAMES {
            sim.step();
            let subjects = subjects_for(&sim.world);
            let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
            let pose = *director.update(&frame);
            check_pose(&pose, &frame);
            if director.shot_id() == Some(id) {
                on_air += 1;
            }
        }
        assert!(on_air > 0, "{id} : jamais à l'antenne sur 30 s");
    }
}

#[test]
fn le_dolly_zoom_ne_joue_que_si_un_drop_est_prepare() {
    let spec = single("dolly-zoom");
    let mut sim = Sim::new(true);
    let subjects = subjects_for(&sim.world);
    sim.step();
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
    assert!(!spec.available(&frame), "pas de drop préparé, pas de dolly-zoom");
    sim.world.show.drop_planned = true;
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
    assert!(spec.available(&frame));
}

#[test]
fn ride_along_sans_vehicule_n_est_pas_disponible() {
    let spec = single("ride-along");
    let mut sim = scene();
    sim.step();
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
    assert!(!spec.available(&frame));
    let subjects = subjects_for(&sim.world);
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
    assert!(spec.available(&frame));
}
