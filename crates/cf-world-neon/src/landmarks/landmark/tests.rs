//! Tests de `landmark`.

use super::*;

fn place(side: i8, spans_road: bool) -> Placement {
    Placement {
        z: -5000.0,
        side,
        tint: 0,
        scale: 1.0,
        spans_road,
    }
}

#[test]
fn le_registre_garde_l_ordre_et_les_noms_de_sillage() {
    let names: Vec<_> = LandmarkKind::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        names,
        [
            "error", "matrix", "flux", "binary", "mecha", "arch", "lava", "arcade", "board"
        ]
    );
    for (i, kind) in LandmarkKind::ALL.iter().enumerate() {
        assert_eq!(kind.index(), i);
        assert_eq!(LandmarkKind::from_name(kind.as_str()), Some(*kind));
    }
    assert_eq!(LandmarkKind::from_name("aquarium"), None);
}

#[test]
fn seuls_error_mecha_et_arcade_sont_exclus_du_mode_calme() {
    let loud: Vec<_> = LandmarkKind::ALL.iter().filter(|k| !k.calm_safe()).collect();
    assert_eq!(
        loud,
        [&LandmarkKind::Error, &LandmarkKind::Mecha, &LandmarkKind::Arcade]
    );
    // Le mur de code compte double.
    assert!((LandmarkKind::Flux.weight() - 2.0).abs() < f64::EPSILON);
    assert!((LandmarkKind::Lava.weight() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn la_racine_de_la_facade_est_au_bord_de_la_route_et_regarde_l_axe() {
    for side in [-1_i8, 1] {
        let p = place(side, false);
        let root = p.root_position();
        assert!((root.x - f64::from(side) * 17.0).abs() < 1e-9);
        // +Z local (vers la route) pointe vers l'axe : le monde −side × X.
        let toward_road = p.root_rotation() * Vec3::Z;
        assert!((toward_road.x + f32::from(side)).abs() < 1e-5, "{toward_road:?}");
        // +X local (à droite du conducteur) suit la route vers −z... du côté droit, +X local est −Z... voir `world`.
        let right = p.root_rotation() * Vec3::X;
        assert!((right.z - f32::from(side)).abs() < 1e-5, "{right:?}");
    }
}

#[test]
fn world_et_la_rotation_de_la_racine_donnent_le_meme_point() {
    for side in [-1_i8, 1] {
        let p = place(side, false);
        let local = Vec3::new(3.0, 12.0, 5.0);
        let by_matrix = p.root_position() + (p.root_rotation() * local).as_dvec3();
        let by_world = p.world(local);
        assert!((by_matrix - by_world).length() < 1e-4, "{by_matrix:?} / {by_world:?}");
    }
}

#[test]
fn local_est_l_inverse_de_world() {
    for (side, spans) in [(-1, false), (1, false), (1, true)] {
        let mut p = place(side, spans);
        p.scale = 1.1;
        let local = Vec3::new(-7.5, 20.0, 9.0);
        let back = p.local(p.world(local));
        assert!((back - local).length() < 1e-4, "{back:?}");
    }
}

#[test]
fn une_structure_qui_enjambe_ne_tourne_pas() {
    let p = place(1, true);
    assert_eq!(p.root_rotation(), Quat::IDENTITY);
    assert!(p.root_position().x.abs() < f64::EPSILON);
}

#[test]
fn la_pente_d_une_lumiere_est_bornee_par_le_contexte() {
    let mut field = FieldState::default();
    let place = place(1, false);
    let mut levels = [0.0; MAX_LIGHTS];
    let dt = 1.0 / 60.0;
    let mut previous = 0.0;
    for _ in 0..20 {
        let mut ctx = StepCtx::new(&mut field, &place, &mut levels, dt);
        ctx.light(0, Vec3::ZERO, 50.0, Vec3::ONE, 5.0);
        let level = field.lights()[0].color.x;
        assert!(level - previous <= LIGHT_FULL_SCALE * dt / cf_show::limiter::MIN_FADE + 1e-4);
        previous = level;
    }
    assert!(previous > 1.0);
}
