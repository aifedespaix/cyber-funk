use super::*;

const DT: f32 = 1.0 / 60.0;
const CAR: DVec3 = DVec3::new(0.0, 0.0, -10_000.0);

fn input(now: f64, show: Option<ShowGate>) -> FieldInput {
    FieldInput {
        dt_s: DT,
        now_s: now,
        anchor: CAR,
        calm: false,
        show,
        textures: 7,
    }
}

fn gate(rate: f32) -> ShowGate {
    ShowGate {
        meteor_rate: rate,
        meteor_shower: false,
        breathing: false,
        drop_now: false,
    }
}

fn open_air(_: DVec3, _: f64) -> bool {
    false
}

/// Rejoue `seconds` secondes ; renvoie le nombre d'astres lancés.
fn run(
    field: &mut MeteorField,
    seconds: f32,
    show: Option<ShowGate>,
    collide: &mut dyn FnMut(DVec3, f64) -> bool,
) -> u64 {
    let before = field.spawned();
    for i in 0..(seconds / DT) as usize {
        field.update(&input(f64::from(DT) * i as f64, show), collide);
    }
    field.spawned() - before
}

#[test]
fn un_lancer_respecte_les_bornes_de_sillage() {
    let mut field = MeteorField::new(1);
    for _ in 0..POOL {
        let id = field.launch(CAR, false, 7).expect("place libre");
        let m = field.meteors().iter().find(|m| m.id == id).copied().expect("lancé");
        assert!(m.active && m.age_s == 0.0);
        // Départ : haut dans le ciel, derrière la voiture ; hauteur absolue.
        assert!((105.0..=185.0).contains(&m.position.y), "{}", m.position.y);
        assert!(m.position.z > CAR.z + 25.0 && m.position.z < CAR.z + 105.0);
        assert!((10.0..=100.0).contains(&(m.position.x - CAR.x).abs()));
        // Vitesse 60..130 m/s, vers l'avant (−z) et vers le bas.
        assert!((60.0..=130.1).contains(&m.velocity.length()), "{}", m.velocity.length());
        assert!(m.velocity.z < 0.0 && m.velocity.y < 0.0);
        assert!((0.7..=7.2).contains(&m.radius), "{}", m.radius);
        assert!(m.tint < HEAT_COUNT && m.texture < 7);
    }
}

#[test]
fn le_mode_calme_ralentit_les_astres() {
    let (mut a, mut b) = (MeteorField::new(1), MeteorField::new(1));
    let fast = a
        .launch(CAR, false, 7)
        .and_then(|id| a.meteors().iter().find(|m| m.id == id).copied());
    let slow = b
        .launch(CAR, true, 7)
        .and_then(|id| b.meteors().iter().find(|m| m.id == id).copied());
    let (fast, slow) = (fast.expect("astre"), slow.expect("astre"));
    assert!((slow.velocity.length() / fast.velocity.length() - 0.45).abs() < 1e-3);
}

#[test]
fn le_pool_est_borne_et_un_lancer_refuse_ne_consomme_pas_d_identifiant() {
    let mut field = MeteorField::new(1);
    for _ in 0..POOL {
        assert!(field.launch(CAR, false, 7).is_some());
    }
    let spawned = field.spawned();
    assert!(field.launch(CAR, false, 7).is_none(), "pool plein");
    assert_eq!(field.spawned(), spawned);
    assert_eq!(field.active().count(), POOL);
}

#[test]
fn sans_texture_rien_ne_tombe() {
    let mut field = MeteorField::new(1);
    assert!(field.launch(CAR, false, 0).is_none());
    assert_eq!(field.spawned(), 0);
}

#[test]
fn meteor_rate_zero_ne_lance_rien_et_la_cadence_suit_la_porte() {
    let mut field = MeteorField::new(1);
    assert_eq!(run(&mut field, 120.0, Some(gate(0.0)), &mut open_air), 0);
    // 30 par minute : un toutes les 2 s en moyenne.
    let mut field = MeteorField::new(1);
    let launched = run(&mut field, 120.0, Some(gate(30.0)), &mut open_air);
    assert!((45..=75).contains(&launched), "{launched} astres en 2 min à 30/min");
    // Plus la porte est haute, plus il en tombe.
    let mut busy = MeteorField::new(1);
    let more = run(&mut busy, 120.0, Some(gate(60.0)), &mut open_air);
    assert!(more > launched, "{more} > {launched}");
}

#[test]
fn la_respiration_ralentit_la_cadence() {
    let mut normal = MeteorField::new(1);
    let mut calm = MeteorField::new(1);
    let breathing = ShowGate {
        breathing: true,
        ..gate(30.0)
    };
    let a = run(&mut normal, 180.0, Some(gate(30.0)), &mut open_air);
    let b = run(&mut calm, 180.0, Some(breathing), &mut open_air);
    assert!(b * 2 < a + 8, "{b} en respiration contre {a}");
}

#[test]
fn hors_spectacle_la_cadence_est_libre_comme_dans_sillage() {
    let mut field = MeteorField::new(1);
    let launched = run(&mut field, 60.0, None, &mut open_air);
    // Un toutes les 2,2 à 6,2 s.
    assert!((9..=28).contains(&launched), "{launched}");
}

#[test]
fn une_pluie_ne_part_que_sur_un_drop_permis() {
    let shower = ShowGate {
        meteor_shower: true,
        drop_now: true,
        ..gate(0.0)
    };
    let started = |gate: ShowGate, seed: u64| {
        let mut field = MeteorField::new(seed);
        // 40 s de repos d'abord : la pause initiale (30 s) est écoulée.
        run(&mut field, 40.0, Some(gate_no_drop(gate)), &mut open_air);
        let mut any = false;
        for i in 0..200 {
            field.update(&input(40.0 + f64::from(DT) * f64::from(i), Some(gate)), &mut open_air);
            any |= field.showering();
        }
        any
    };
    fn gate_no_drop(g: ShowGate) -> ShowGate {
        ShowGate { drop_now: false, ..g }
    }
    // Probabilité 0,4 par drop : sur 200 images avec drop, au moins une pluie sur 40 graines.
    assert!((0..40).any(|seed| started(shower, seed)), "une pluie finit par partir");
    for forbidden in [
        ShowGate {
            meteor_shower: false,
            ..shower
        },
        ShowGate {
            breathing: true,
            ..shower
        },
        ShowGate {
            drop_now: false,
            ..shower
        },
    ] {
        assert!(!(0..40).any(|seed| started(forbidden, seed)), "{forbidden:?}");
    }
}

#[test]
fn une_pluie_lance_de_six_a_douze_astres_etales_dans_le_temps() {
    let mut field = MeteorField::new(1);
    field.start_shower(12, 5.0);
    assert!(field.showering());
    let mut launches = Vec::new();
    for i in 0..(10.0 / DT) as usize {
        let before = field.spawned();
        field.update(&input(f64::from(DT) * i as f64, Some(gate(0.0))), &mut open_air);
        if field.spawned() > before {
            launches.push(i);
        }
        if !field.showering() {
            break;
        }
    }
    assert_eq!(launches.len(), 12, "les douze astres partent");
    let span = (launches[11] - launches[0]) as f32 * DT;
    assert!((2.0..=9.0).contains(&span), "étalés sur {span} s");
    field.start_shower(500, 1.0);
    assert!(field.showering());
}

#[test]
fn un_astre_qui_touche_un_immeuble_explose_et_publie_son_dernier_impact() {
    let mut field = MeteorField::new(1);
    let id = field.launch(CAR, false, 7).expect("astre");
    // Un mur à 20 m au-dessus de la brume, dans lequel il finit par s'enfoncer.
    let mut wall = |at: DVec3, _: f64| at.y < 40.0;
    let mut now = 0.0;
    let mut blasts = 0;
    for _ in 0..600 {
        field.update(&input(now, None), &mut wall);
        now += f64::from(DT);
        blasts += field
            .events()
            .iter()
            .filter(|e| matches!(e, FieldEvent::Blast { .. }))
            .count();
        if blasts > 0 {
            break;
        }
    }
    assert_eq!(blasts, 1);
    let last = field.last_blast().expect("impact publié");
    assert_eq!(last.meteor_id, id);
    assert_eq!(last.seq, 1);
    assert!(last.time_s > 0.0 && last.radius > 0.0);
    assert!(field.active().all(|m| m.id != id), "l'astre est retiré");
    assert!(
        last.at.y > 38.0,
        "l'explosion naît sur le mur, reculée sur la trajectoire : {}",
        last.at.y
    );
}

#[test]
fn un_astre_qui_atteint_la_brume_laisse_un_anneau_qui_s_eteint() {
    let mut field = MeteorField::new(1);
    field.launch(CAR, false, 7).expect("astre");
    let mut splashed = None;
    for i in 0..1200 {
        field.update(&input(f64::from(DT) * f64::from(i), None), &mut open_air);
        for event in field.events() {
            if let FieldEvent::Splash { slot } = event {
                splashed = Some(*slot);
            }
        }
        if splashed.is_some() {
            break;
        }
    }
    let slot = splashed.expect("l'astre atteint la brume");
    assert!(field.last_blast().is_none(), "pas d'explosion dans la brume");
    let splash = field.splashes()[slot];
    assert!(!splash.is_free());
    assert!((splash.position.y - (MIST_Y + 0.3)).abs() < 1e-9);
    for i in 0..(SPLASH_LIFE / DT) as usize + 5 {
        field.update(&input(100.0 + f64::from(DT) * i as f64, Some(gate(0.0))), &mut open_air);
    }
    assert!(field.splashes()[slot].is_free());
}

#[test]
fn un_astre_rapide_ne_traverse_pas_un_mur_mince() {
    // Un mur d'un mètre d'épaisseur, sur le chemin : le pas de collision (3 m) ne le laisse pas passer.
    for k in 0..30_u64 {
        let mut field = MeteorField::new(k);
        for _ in 0..k {
            field.launch(CAR, false, 7);
            field.update(&input(0.0, None), &mut open_air);
        }
        let mut field = MeteorField::new(k);
        let id = field.launch(CAR, false, 7).expect("astre");
        let m = *field.meteors().iter().find(|m| m.id == id).expect("lancé");
        let t = 2.0_f64;
        let wall_at = m.position + m.velocity.as_dvec3() * t;
        let mut wall = |at: DVec3, r: f64| (at - wall_at).length() < r + 0.5;
        // Un gros pas de temps (accroc d'image) : 0,05 s à 130 m/s, soit 6,5 m.
        let mut hit = false;
        for i in 0..400 {
            let big = FieldInput {
                dt_s: 0.05,
                ..input(f64::from(i) * 0.05, None)
            };
            field.update(&big, &mut wall);
            hit |= field.events().iter().any(|e| matches!(e, FieldEvent::Blast { .. }));
            if hit {
                break;
            }
        }
        assert!(hit, "l'astre {k} traverse le mur");
    }
}

#[test]
fn les_evenements_et_le_pool_ne_depassent_jamais_leur_capacite() {
    let mut field = MeteorField::new(3);
    let capacity = field.events.capacity();
    field.start_shower(12, 2.0);
    let mut wall = |at: DVec3, _: f64| at.y < 20.0;
    for i in 0..3000 {
        field.update(&input(f64::from(DT) * f64::from(i), Some(gate(60.0))), &mut wall);
        assert!(field.active().count() <= POOL);
    }
    assert_eq!(
        field.events.capacity(),
        capacity,
        "le vecteur d'événements ne se réalloue pas"
    );
}

#[test]
fn le_tirage_est_deterministe_pour_une_graine() {
    let trace = |seed: u64| {
        let mut field = MeteorField::new(seed);
        field.start_shower(12, 5.0);
        run(&mut field, 20.0, Some(gate(20.0)), &mut open_air);
        field.meteors().map(|m| (m.id, m.position.x.to_bits(), m.active))
    };
    assert_eq!(trace(5), trace(5));
}
