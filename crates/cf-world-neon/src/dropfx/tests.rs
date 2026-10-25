//! Le paquet du drop : logique pure (même image, cascade, règle 7) et application Bevy sans rendu (même image jusqu'aux
//! entités).

use bevy::math::DVec3;
use cf_render::{AbsolutePosition, CfSet, CurrentShowFx, ShowFxInput, StreamFocus};
use cf_show::drop::DropFxPackage;
use cf_show::limiter::{MAX_FLASH_PEAK, MAX_FLASHES_PER_SECOND, MIN_FADE};

use super::ring::{BURST_Y_M, RING_Y_M};
use super::*;

const DT: f32 = 1.0 / 60.0;
const CAR: DVec3 = DVec3::new(2.0, 0.0, -8000.0);

fn package(shockwaves: u8, burst: u16) -> DropFxPackage {
    DropFxPackage {
        flash: 0.5,
        rgb_split: 0.3,
        city_pulse: 0.5,
        shockwaves,
        burst,
    }
}

fn input(frame: usize, drop: Option<DropFxPackage>, policy: Option<(bool, bool)>) -> DropInput {
    DropInput {
        dt_s: DT,
        now_s: f64::from(DT) * frame as f64,
        drop,
        at: Some(CAR),
        policy,
        seed: 7,
    }
}

const ON: Option<(bool, bool)> = Some((true, false));

/// L'étincelle `i` brille-t-elle ?
fn lit(fx: &DropFx, i: usize) -> bool {
    fx.particle_color(i).into_iter().any(|c| c > 0.0)
}

#[test]
fn les_ondes_et_la_gerbe_partent_sur_l_image_du_drop_now() {
    let mut fx = DropFx::new(1);
    for frame in 0..30 {
        fx.step(&input(frame, None, ON));
    }
    assert_eq!((fx.rings_alive(), fx.particles_alive(), fx.packages()), (0, 0, 0));
    // L'image du `drop_now` : le paquet est présent, et rien d'autre à cette image.
    fx.step(&input(30, Some(package(3, 40)), ON));
    assert_eq!(fx.packages(), 1);
    assert_eq!(
        fx.last_package_frame(),
        Some(fx.frame()),
        "le paquet part dans l'appel de l'image du drop"
    );
    assert_eq!(fx.particles_alive(), 40, "la gerbe jaillit tout de suite");
    assert_eq!(fx.rings_alive(), 1, "la première onde part tout de suite");
    assert_eq!(fx.pending(), 2, "les deux autres attendent leur tour");
    let ring = fx.rings().iter().find(|r| !r.is_free()).expect("onde");
    assert_eq!(ring.at, DVec3::new(CAR.x, RING_Y_M, CAR.z), "au sol, sous la voiture");
    assert!((fx.origin() - (CAR + DVec3::new(0.0, BURST_Y_M, 0.0))).length() < 1e-9);
    assert!(
        fx.ring_level(ring) > 0.0 || fx.master() > 0.0,
        "visible dès cette image"
    );
}

#[test]
fn le_nombre_d_ondes_suit_le_paquet_et_la_cascade_garde_son_ecart() {
    for shockwaves in 1..=3_u8 {
        let mut fx = DropFx::new(1);
        let mut starts = Vec::new();
        let mut alive_before = 0;
        for frame in 0..120 {
            let drop = (frame == 0).then(|| package(shockwaves, 20));
            fx.step(&input(frame, drop, ON));
            let alive = fx
                .rings()
                .iter()
                .filter(|r| r.age_s <= DT * 1.5 && !r.is_free())
                .count();
            if alive > alive_before.min(alive) || (alive > 0 && fx.rings().iter().any(|r| r.age_s <= DT * 1.01)) {
                starts.push(frame);
            }
            alive_before = alive;
        }
        starts.dedup();
        assert_eq!(
            starts.len(),
            usize::from(shockwaves),
            "{shockwaves} ondes : départs aux images {starts:?}"
        );
        for (i, &frame) in starts.iter().enumerate() {
            let expected = (i as f32 * DROP_RING_GAP_S / DT).ceil() as usize;
            assert!(
                frame.abs_diff(expected) <= 1,
                "onde {i} à l'image {frame}, attendue vers {expected}"
            );
        }
    }
}

#[test]
fn la_cascade_est_accordee_entiere_et_ne_depasse_pas_trois_departs_par_seconde() {
    let mut fx = DropFx::new(1);
    for frame in 0..60 {
        let drop = (frame == 0).then(|| package(3, 60));
        fx.step(&input(frame, drop, ON));
    }
    let rings = fx.rings();
    assert_eq!(
        rings.iter().filter(|r| !r.is_free() && r.granted).count(),
        3,
        "trois ondes accordées"
    );
    const { assert!(DROP_RINGS_MAX <= MAX_FLASHES_PER_SECOND) };
}

#[test]
fn un_drop_trop_rapproche_du_precedent_part_mais_reste_sombre() {
    let mut fx = DropFx::new(1);
    fx.step(&input(0, Some(package(1, 20)), ON));
    // 0,1 s plus tard (voie occupée, moins de 0,25 s) : le paquet part quand même, mais le limiteur refuse l'éclat.
    for frame in 1..6 {
        fx.step(&input(frame, None, ON));
    }
    fx.step(&input(6, Some(package(1, 20)), ON));
    assert_eq!(fx.packages(), 2);
    let youngest = fx
        .rings()
        .iter()
        .filter(|r| !r.is_free())
        .min_by(|a, b| a.age_s.total_cmp(&b.age_s));
    let youngest = youngest.expect("onde");
    assert!(!youngest.granted, "départ refusé : anneau sombre");
    assert!(fx.ring_level(youngest) <= UNGRANTED_GAIN + 1e-6);
}

#[test]
fn flashs_coupes_respiration_ou_sans_spectacle_rien_ne_s_allume_mais_l_etat_part_a_l_image() {
    for policy in [None, Some((false, false)), Some((true, true))] {
        let mut fx = DropFx::new(1);
        for frame in 0..10 {
            fx.step(&input(frame, None, policy));
        }
        fx.step(&input(10, Some(package(3, 40)), policy));
        assert_eq!(fx.last_package_frame(), Some(fx.frame()), "{policy:?}");
        assert_eq!(fx.rings_alive(), 1);
        for frame in 11..120 {
            fx.step(&input(frame, None, policy));
            for ring in fx.rings() {
                assert_eq!(fx.ring_level(ring), 0.0, "{policy:?} : onde visible");
            }
            for i in 0..PARTICLES {
                assert_eq!(fx.particle_color(i), [0.0; 3], "{policy:?} : étincelle visible");
            }
        }
        assert_eq!(fx.master(), 0.0);
    }
}

#[test]
fn couper_les_flashs_en_plein_drop_redescend_en_min_fade() {
    let mut fx = DropFx::new(1);
    fx.step(&input(0, Some(package(1, 60)), ON));
    for frame in 1..12 {
        fx.step(&input(frame, None, ON));
    }
    assert!((fx.master() - 1.0).abs() < 1e-6);
    let mut frames = 0;
    while fx.master() > 0.0 {
        fx.step(&input(12 + frames, None, Some((false, false))));
        frames += 1;
        assert!(frames < 20, "la permission doit retomber");
    }
    assert!(frames as f32 * DT <= MIN_FADE + 2.0 * DT, "{frames} images");
}

#[test]
fn aucune_variation_de_luminance_plus_rapide_que_min_fade() {
    let mut fx = DropFx::new(3);
    let mut previous: Option<(Vec<f32>, Vec<f32>)> = None;
    // Un drop à l'image 0, un autre à 0,5 s, un troisième à 1 s : le pire cas des demandes.
    for frame in 0..300 {
        let drop = matches!(frame, 0 | 30 | 60).then(|| package(3, 60));
        fx.step(&input(frame, drop, ON));
        let rings: Vec<f32> = fx.rings().iter().map(|r| fx.ring_level(r)).collect();
        let sparks: Vec<f32> = (0..PARTICLES)
            .map(|i| fx.particle_color(i).into_iter().fold(0.0, f32::max))
            .collect();
        assert!(rings.iter().all(|&l| l <= MAX_FLASH_PEAK + 1e-6), "plafond de la voie");
        if let Some((rings_before, sparks_before)) = &previous {
            // Une onde reprise dans un autre emplacement (recyclage) change de niveau : on ne compare que les ondes encore
            // en vie à l'image précédente.
            for (i, (&a, &b)) in rings_before.iter().zip(&rings).enumerate() {
                if fx.rings()[i].age_s > DT * 1.5 {
                    assert!(
                        (a - b).abs() <= MAX_FLASH_PEAK * DT / MIN_FADE + 1e-3,
                        "onde {i} : {a} -> {b}"
                    );
                }
            }
            // Les étincelles montent en 0,1 s (jamais d'allumage sec) et s'éteignent en douceur.
            for (&a, &b) in sparks_before.iter().zip(&sparks) {
                assert!((a - b).abs() <= DT / MIN_FADE + 1e-3, "étincelle : {a} -> {b}");
            }
        }
        previous = Some((rings, sparks));
    }
}

#[test]
fn le_mode_calme_ne_joue_pas_le_paquet() {
    let mut fx = DropFx::new(1);
    let mut calm = input(0, Some(package(3, 40)), ON);
    calm.at = None;
    fx.step(&calm);
    assert_eq!((fx.packages(), fx.rings_alive(), fx.particles_alive()), (0, 0, 0));
}

#[test]
fn les_etincelles_retombent_et_leur_boite_les_contient() {
    let mut fx = DropFx::new(5);
    fx.step(&input(0, Some(package(1, 60)), ON));
    for frame in 1..40 {
        fx.step(&input(frame, None, ON));
        let (min, max) = fx.bounds(0.0).expect("des étincelles vivent");
        for i in 0..PARTICLES {
            if lit(&fx, i) {
                let p = fx.particle_offset(i);
                assert!(
                    p.cmpge(min).all() && p.cmple(max).all(),
                    "étincelle {i} hors de sa boîte"
                );
            }
        }
    }
    for frame in 40..200 {
        fx.step(&input(frame, None, ON));
    }
    assert_eq!(fx.particles_alive(), 0);
    assert!(fx.bounds(0.0).is_none());
    assert_eq!(fx.rings_alive(), 0);
}

#[test]
fn deux_gerbes_successives_gardent_leurs_positions_malgre_le_changement_d_origine() {
    let mut fx = DropFx::new(5);
    fx.step(&input(0, Some(package(1, 20)), ON));
    for frame in 1..30 {
        fx.step(&input(frame, None, ON));
    }
    let world_of = |fx: &DropFx, i: usize| fx.origin() + fx.particle_offset(i).as_dvec3();
    let alive: Vec<usize> = (0..PARTICLES).filter(|&i| lit(&fx, i)).collect();
    let before: Vec<DVec3> = alive.iter().map(|&i| world_of(&fx, i)).collect();
    let mut moved = input(30, Some(package(1, 20)), ON);
    moved.at = Some(CAR + DVec3::new(0.0, 0.0, -150.0));
    // Un `dt` nul : seules les positions rebasées comptent.
    moved.dt_s = 0.0;
    fx.step(&moved);
    for (&i, &was) in alive.iter().zip(&before) {
        assert!(
            (world_of(&fx, i) - was).length() < 1e-3,
            "étincelle {i} a sauté au changement d'origine"
        );
    }
}

#[test]
fn meme_graine_meme_gerbe() {
    let run = |seed| {
        let mut fx = DropFx::new(seed);
        fx.step(&input(0, Some(package(1, 30)), ON));
        (0..PARTICLES)
            .map(|i| fx.particle_offset(i).to_array())
            .collect::<Vec<_>>()
    };
    assert_eq!(run(9), run(9));
    assert_ne!(run(9), run(10));
}

// --- Application Bevy sans rendu : même image jusqu'aux entités ---

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Mesh>()
        .configure_sets(
            Update,
            (
                CfSet::Input,
                CfSet::Show,
                CfSet::Simulate,
                CfSet::Direct,
                CfSet::Present,
            )
                .chain(),
        )
        .insert_resource(show(None))
        .add_plugins(DropFxPlugin);
    app.world_mut().spawn((AbsolutePosition(CAR), StreamFocus));
    app
}

fn show(drop: Option<DropFxPackage>) -> CurrentShowFx {
    CurrentShowFx(ShowFxInput {
        flashes: true,
        breathing: false,
        heat: 0.5,
        time_scale: 1.0,
        cue_seq: 0,
        fx_rgb_split: 0.0,
        fx_flash: 0.0,
        drop,
    })
}

fn tick(app: &mut App) {
    std::thread::sleep(std::time::Duration::from_millis(2));
    app.update();
}

fn visible(app: &mut App) -> (usize, bool) {
    let mut q = app.world_mut().query::<(&DropPiece, &Visibility)>();
    let (mut rings, mut sparks) = (0, false);
    for (piece, visibility) in q.iter(app.world()) {
        if *visibility == Visibility::Visible {
            match piece {
                DropPiece::Ring(_) => rings += 1,
                DropPiece::Sparks => sparks = true,
            }
        }
    }
    (rings, sparks)
}

#[test]
fn dans_l_application_le_paquet_est_a_l_ecran_sur_l_image_ou_drop_now_est_vrai() {
    let mut app = app();
    for _ in 0..4 {
        tick(&mut app);
    }
    assert_eq!(visible(&mut app), (0, false), "rien à l'écran avant le drop");
    assert_eq!(app.world().resource::<DropFx>().packages(), 0);

    // Une seule image porte le paquet : `ShowFxInput::drop` est `Some` quand `drop_now` est vrai, pas avant ni après.
    app.insert_resource(show(Some(package(3, 40))));
    tick(&mut app);
    {
        let fx = app.world().resource::<DropFx>();
        assert_eq!(fx.packages(), 1);
        assert_eq!(
            fx.last_package_frame(),
            Some(fx.frame()),
            "parti dans l'image même du drop_now"
        );
        assert_eq!((fx.rings_alive(), fx.particles_alive()), (1, 40));
    }
    let (rings, sparks) = visible(&mut app);
    assert_eq!(
        (rings, sparks),
        (1, true),
        "onde et gerbe sont visibles sur cette même image"
    );

    // L'image suivante n'a plus de paquet : rien de nouveau ne part, ce qui est parti continue.
    app.insert_resource(show(None));
    tick(&mut app);
    let fx = app.world().resource::<DropFx>();
    assert_eq!(fx.packages(), 1);
    assert!(fx.rings_alive() >= 1 && fx.particles_alive() > 0);
}

#[test]
fn dans_l_application_flashs_coupes_rien_n_est_visible_mais_le_paquet_part_a_l_image() {
    let mut app = app();
    app.insert_resource(CurrentShowFx(ShowFxInput {
        flashes: false,
        ..show(None).0
    }));
    for _ in 0..4 {
        tick(&mut app);
    }
    app.insert_resource(CurrentShowFx(ShowFxInput {
        flashes: false,
        ..show(Some(package(3, 40))).0
    }));
    tick(&mut app);
    let fx = app.world().resource::<DropFx>();
    assert_eq!(fx.last_package_frame(), Some(fx.frame()));
    assert_eq!(visible(&mut app), (0, false), "flash nul quand les flashs sont coupés");
}

#[test]
fn le_nombre_d_entites_ne_change_pas_pendant_un_drop() {
    let mut app = app();
    tick(&mut app);
    let count = |app: &mut App| app.world_mut().query::<Entity>().iter(app.world()).count();
    let before = count(&mut app);
    assert!(before > RINGS, "le pool existe dès le démarrage");
    app.insert_resource(show(Some(package(3, 60))));
    tick(&mut app);
    app.insert_resource(show(None));
    for _ in 0..30 {
        tick(&mut app);
    }
    assert_eq!(count(&mut app), before);
}
