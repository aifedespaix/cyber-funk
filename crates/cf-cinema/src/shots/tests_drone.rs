//! Tests du plan `drone-pov` : la caméra reprend les yeux du drone que le monde publie, se penche dans les virages, ne
//! s'arrête qu'avec l'événement, et le réalisateur y bascule dès que la visite existe.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use super::DronePov;
use crate::director::CinemaDirector;
use crate::rng::CinemaRng;
use crate::shot::ShotSpec;
use crate::shots::Chase;
use crate::testkit::{BoxCity, DT, Sim};
use crate::world::{CinemaFrame, CinemaWorld};

/// Les yeux d'un drone qui tourne autour de la voiture à `t` secondes : position, regard vers la voiture, champ de vision.
fn eyes(world: &CinemaWorld, t: f64) -> Subject {
    let a = t * 0.9;
    let position = world.car + DVec3::new(a.sin() * 4.5, 2.2, a.cos() * 4.5 + 0.2);
    let forward = (world.car + DVec3::new(0.0, 0.9, 0.2) - position).as_vec3().normalize();
    Subject {
        id: 7,
        kind: SubjectKind::Drone,
        position,
        velocity: Vec3::new(a.cos() as f32 * 4.0, 0.0, -(a.sin() as f32) * 4.0),
        size: 84.0,
        facing: Some(forward),
    }
}

#[test]
fn le_plan_n_est_disponible_qu_avec_un_drone_et_s_y_precipite() {
    let spec = DronePov;
    let mut sim = Sim::new(false);
    sim.step();
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
    assert!(!spec.available(&frame));
    assert_eq!(spec.opportunity(&frame), 0.0);
    let subjects = [eyes(&sim.world, 0.0)];
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
    assert!(spec.available(&frame));
    assert!(spec.opportunity(&frame) > 0.9, "au-dessus du seuil des occasions (0,7)");
}

#[test]
fn la_camera_prend_la_position_le_regard_et_le_champ_du_drone() {
    let spec = DronePov;
    let mut sim = Sim::new(false);
    sim.step();
    let subjects = [eyes(&sim.world, 0.0)];
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &subjects);
    let mut shot = spec.create(&frame, &mut CinemaRng::new(1));
    assert!(shot.holding(), "couper en pleine visite la gâcherait");
    let mut pose = CameraPose::default();
    assert!(shot.update(&frame, &mut pose));
    assert_eq!(pose.position, subjects[0].position);
    assert!((pose.fov_deg - 84.0).abs() < 1e-4);
    let looked = pose.rotation * Vec3::NEG_Z;
    let wanted = subjects[0].facing.expect("regard");
    assert!(looked.dot(wanted) > 0.999, "regard {looked:?} au lieu de {wanted:?}");
}

#[test]
fn le_drone_se_penche_dans_un_virage_et_se_redresse_en_ligne_droite() {
    let spec = DronePov;
    let mut sim = Sim::new(false);
    sim.step();
    let subjects = [eyes(&sim.world, 0.0)];
    let mut shot = spec.create(
        &CinemaFrame::new(&sim.world, &BoxCity, &subjects),
        &mut CinemaRng::new(1),
    );
    let mut pose = CameraPose::default();
    let mut max_tilt = 0.0_f32;
    for _ in 0..120 {
        sim.step();
        let subjects = [eyes(&sim.world, sim.world.time)];
        assert!(shot.update(&CinemaFrame::new(&sim.world, &BoxCity, &subjects), &mut pose));
        // Le côté droit de la caméra quitte l'horizontale quand le drone se penche dans le virage.
        max_tilt = max_tilt.max((pose.rotation * Vec3::X).y.abs());
    }
    assert!(max_tilt > 0.05, "aucun penchant dans un virage : {max_tilt}");
    // Regard fixe, on se redresse.
    let fixed = [eyes(&sim.world, 3.0)];
    for _ in 0..240 {
        sim.step();
        assert!(shot.update(&CinemaFrame::new(&sim.world, &BoxCity, &fixed), &mut pose));
    }
    assert!((pose.rotation * Vec3::X).y.abs() < 1e-3, "le drone ne se redresse pas");
}

#[test]
fn le_plan_s_arrete_avec_l_evenement() {
    let spec = DronePov;
    let mut sim = Sim::new(false);
    sim.step();
    let subjects = [eyes(&sim.world, 0.0)];
    let mut shot = spec.create(
        &CinemaFrame::new(&sim.world, &BoxCity, &subjects),
        &mut CinemaRng::new(1),
    );
    let mut pose = CameraPose::default();
    assert!(shot.update(&CinemaFrame::new(&sim.world, &BoxCity, &subjects), &mut pose));
    assert!(!shot.update(&CinemaFrame::new(&sim.world, &BoxCity, &[]), &mut pose));
}

/// Le réalisateur laisse la poursuite pour la visite dès qu'elle existe, la garde tant qu'elle dure et revient ensuite.
#[test]
fn le_realisateur_bascule_sur_le_drone_et_en_revient() {
    let mut director = CinemaDirector::new(vec![Box::new(Chase), Box::new(DronePov)], false, 5);
    let mut sim = Sim::new(false);
    sim.step();
    director.start(&CameraPose::default(), &CinemaFrame::new(&sim.world, &BoxCity, &[]));
    let (mut on_drone, mut after_end) = (0usize, None);
    let seconds = 50.0;
    for i in 0..((seconds / f64::from(DT)) as usize) {
        sim.step();
        let t = sim.world.time;
        // La visite dure de 5 s à 30 s.
        let subjects: Vec<Subject> = if (5.0..30.0).contains(&t) {
            vec![eyes(&sim.world, t)]
        } else {
            vec![]
        };
        director.update(&CinemaFrame::new(&sim.world, &BoxCity, &subjects));
        if director.shot_id() == Some("drone-pov") {
            on_drone += 1;
            assert!(
                (5.0..30.5).contains(&t),
                "le plan drone dure après l'événement (t = {t})"
            );
        } else if t > 31.0 && after_end.is_none() {
            after_end = Some((i, director.shot_id()));
        }
    }
    assert!(
        on_drone > 60 * 15,
        "le réalisateur n'a tenu le drone que {on_drone} images"
    );
    assert_eq!(
        after_end.map(|(_, id)| id),
        Some(Some("chase")),
        "il revient à la poursuite"
    );
}
