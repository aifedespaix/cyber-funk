//! Tests de `robot`.

use super::*;
use crate::landmarks::part::PartPaint;

fn spec() -> RobotSpec {
    RobotSpec {
        side: 1,
        x: 29.0,
        z: -100.0,
        scale: 1.2,
        accent: SceneColor::Sun,
        beat_offset: 0,
        level_shift: 0,
        mirror: false,
        every: 1,
        twist_gain: 1.0,
        look_speed: 1.0,
        slot: 0,
        seed: 9,
    }
}

fn live() -> LandmarkFrame {
    let mut frame = LandmarkFrame::rest();
    frame.policy = crate::landmarks::frame::FlashPolicy {
        flashes: true,
        breathing: false,
    };
    frame
}

#[test]
fn un_robot_a_quatorze_pieces_dont_l_ecran() {
    let list = parts();
    assert_eq!(list.len(), PARTS);
    assert_eq!(list.iter().filter(|p| p.look == Look::MechaFace).count(), 1);
    assert_eq!(list.iter().filter(|p| p.look == Look::MechaPanel).count(), BLOCKS);
}

#[test]
fn la_pose_place_les_os_les_graines_se_suivent_et_les_jambes_plongent() {
    let mut robot = Robot::new(spec());
    let frame = live();
    robot.step(&frame, true, &mut || false, 0.0);
    let mut out = vec![PartPaint::default(); PARTS];
    robot.paint(&mut out);
    let seeds: Vec<f32> = [0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13]
        .iter()
        .map(|i| out[*i].a.w)
        .collect();
    assert_eq!(seeds, (9..22).map(|s| s as f32).collect::<Vec<_>>());
    // Les pieds sont à des centaines de mètres sous la route.
    let leg = &out[0];
    let bottom = leg.transform.translation.y - leg.transform.scale.y / 2.0;
    assert!(bottom < -400.0, "pieds à {bottom}");
    for part in &out {
        assert!(part.transform.translation.is_finite() && part.transform.scale.is_finite());
    }
}

#[test]
fn les_bras_miroir_echangent_les_epaules() {
    let mut a = Robot::new(spec());
    let mut b = Robot::new(RobotSpec { mirror: true, ..spec() });
    let mut frame = live();
    frame.time = 1.0;
    for robot in [&mut a, &mut b] {
        robot.kick(&frame);
        robot.step(&frame, true, &mut || false, 0.0);
        frame.time += 0.3;
        robot.step(&frame, true, &mut || false, 0.0);
    }
    let (mut pa, mut pb) = (vec![PartPaint::default(); PARTS], vec![PartPaint::default(); PARTS]);
    a.paint(&mut pa);
    b.paint(&mut pb);
    // Les mains ne sont pas aux mêmes endroits (les deux poses de la séquence ne sont pas symétriques).
    assert!(
        pa[9].transform.translation.distance(pb[9].transform.translation) > 1e-3
            || pa[13].transform.translation.distance(pb[13].transform.translation) > 1e-3
    );
}

#[test]
fn au_repos_le_visage_reste_calme_ou_souriant_et_ne_se_dechire_jamais() {
    let mut robot = Robot::new(spec());
    let mut frame = LandmarkFrame::rest();
    frame.drive.section = cf_analysis::Section::Peak;
    robot.drop_hit();
    for i in 0..600 {
        frame.time = f64::from(i) / 60.0;
        robot.step(&frame, false, &mut || true, 1.0);
        assert!(!robot.face().aggressive());
        assert!(robot.tear().abs() < f32::EPSILON);
    }
}

#[test]
fn au_sommet_permis_le_visage_devient_agressif_et_le_changement_demande_la_dechirure() {
    let mut robot = Robot::new(spec());
    let mut frame = live();
    frame.drive.section = cf_analysis::Section::Peak;
    let mut requests = 0;
    for i in 0..120 {
        frame.time = f64::from(i) / 60.0;
        robot.step(
            &frame,
            true,
            &mut || {
                requests += 1;
                true
            },
            0.5,
        );
    }
    assert!(robot.face().aggressive());
    assert_eq!(requests, 1, "un seul changement, une seule demande");
}

#[test]
fn un_visage_ne_change_pas_plus_d_une_fois_par_seconde() {
    let mut robot = Robot::new(spec());
    let mut frame = live();
    let mut changes = 0;
    let mut last = robot.face();
    for i in 0..1200 {
        frame.time = f64::from(i) / 60.0;
        // La section alterne à chaque image : le visage doit rester stable entre deux tenues d'une seconde.
        frame.drive.section = if i % 2 == 0 {
            cf_analysis::Section::Calm
        } else {
            cf_analysis::Section::Groove
        };
        robot.step(&frame, true, &mut || false, 0.0);
        if robot.face() != last {
            changes += 1;
            last = robot.face();
        }
    }
    assert!(changes <= 20, "{changes} changements en 20 s");
}

#[test]
fn la_lumiere_est_a_hauteur_de_poitrine_et_large() {
    let mut robot = Robot::new(spec());
    let frame = live();
    robot.step(&frame, true, &mut || false, 0.0);
    let mut out = vec![PartPaint::default(); PARTS];
    robot.paint(&mut out);
    let (position, radius, _, power) = robot.light(&frame);
    assert!((radius - 130.0 * 1.2).abs() < 1e-3);
    assert!(power >= 1.6);
    assert!(position.y > 30.0 && position.y < 80.0 * 1.2, "{position:?}");
}
