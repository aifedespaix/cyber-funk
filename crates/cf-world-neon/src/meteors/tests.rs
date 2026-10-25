//! Les astres dans une application Bevy sans rendu : ordres du réalisateur, sujets, impact publié, cadence des éclairs.

use bevy::math::DVec3;
use cf_cinema::world::LaunchMeteor;
use cf_render::{AbsolutePosition, StreamFocus};

use super::*;

fn app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .add_plugins(MeteorsPlugin)
        .insert_resource(PlanetTextures(vec![Handle::default(); 3]));
    app.world_mut()
        .spawn((AbsolutePosition(DVec3::new(0.0, 0.0, -5000.0)), StreamFocus));
    app
}

fn tick(app: &mut App, frames: usize) {
    for _ in 0..frames {
        std::thread::sleep(std::time::Duration::from_millis(1));
        app.update();
    }
}

#[test]
fn le_message_launch_meteor_lance_un_astre_publie_comme_sujet() {
    let mut app = app();
    app.world_mut().write_message(LaunchMeteor);
    tick(&mut app, 2);
    let meteors = app.world().resource::<Meteors>();
    assert_eq!(meteors.field().active().count(), 1, "le message lance un astre");
    let mut subjects = Vec::new();
    meteors.collect_subjects(|s| subjects.push(s));
    assert_eq!(subjects.len(), 1);
    let subject = subjects[0];
    assert_eq!(subject.kind, cf_render::world::SubjectKind::Meteor);
    let meteor = meteors.field().active().next().copied().expect("astre");
    assert_eq!(subject.id, meteor.id);
    assert_eq!(subject.position, meteor.position);
    assert!((subject.size - meteor.radius).abs() < f32::EPSILON);
    assert!(subject.velocity.length() > 50.0);
}

#[test]
fn sans_texture_le_message_ne_lance_rien() {
    let mut app = app();
    app.insert_resource(PlanetTextures(Vec::new()));
    app.world_mut().write_message(LaunchMeteor);
    tick(&mut app, 2);
    assert_eq!(app.world().resource::<Meteors>().field().active().count(), 0);
}

#[test]
fn sans_spectacle_une_explosion_ne_flashe_pas_mais_publie_son_impact() {
    let mut meteors = Meteors::new(1);
    let anchor = DVec3::new(0.0, 0.0, -5000.0);
    meteors.launch(anchor, false, 3).expect("astre");
    let mut wall = |at: DVec3, _: f64| at.y < 40.0;
    let mut now = 0.0;
    for _ in 0..600 {
        let input = FieldInput {
            dt_s: 1.0 / 60.0,
            now_s: now,
            anchor,
            calm: false,
            show: None,
            textures: 3,
        };
        meteors.step(&input, None, &mut wall);
        now += 1.0 / 60.0;
        if meteors.last_blast().is_some() {
            break;
        }
    }
    assert!(
        meteors.last_blast().is_some(),
        "l'impact est publié pour le réalisateur"
    );
    assert!(
        meteors.blasts().slots().iter().any(|b| !b.shock_done()),
        "l'explosion est rendue"
    );
    assert!(
        meteors.blasts().slots().iter().all(|b| !b.flash_granted),
        "aucun éclair sans spectacle"
    );
    assert_eq!(meteors.flash().level(), 0.0);
}

/// La pluie la plus dense (douze astres sur 4 s) contre un mur : jamais plus de trois éclairs par seconde, jamais de
/// variation plus rapide que `MIN_FADE`, jamais plus que le plafond, quelle que soit la demande.
#[test]
fn une_pluie_dense_ne_depasse_pas_trois_eclairs_par_seconde() {
    use cf_show::limiter::{MAX_FLASH_PEAK, MAX_FLASHES_PER_SECOND, MIN_FADE};

    const DT: f32 = 1.0 / 60.0;
    let mut meteors = Meteors::new(9);
    let anchor = DVec3::new(0.0, 0.0, -5000.0);
    let mut wall = |at: DVec3, _: f64| at.y < 60.0;
    let mut grants = Vec::new();
    let mut levels = Vec::new();
    let mut granted_before = 0;
    let mut showers = 0;
    for i in 0..(40.0 / DT) as usize {
        let now = f64::from(DT) * i as f64;
        if i % 240 == 0 && showers < 6 {
            // Une pluie de plus tous les 4 s.
            meteors.field_mut().start_shower(12, 4.0);
            showers += 1;
        }
        let input = FieldInput {
            dt_s: DT,
            now_s: now,
            anchor,
            calm: false,
            show: Some(ShowGate {
                meteor_rate: 0.0,
                meteor_shower: true,
                breathing: false,
                drop_now: false,
            }),
            textures: 3,
        };
        meteors.step(&input, Some((true, false)), &mut wall);
        let granted = meteors.blasts().slots().iter().filter(|b| b.flash_granted).count()
            + meteors.field().splashes().iter().filter(|s| s.flash_granted).count();
        assert!(granted <= 1, "un seul éclair brille à la fois");
        // Un nouveau titulaire = un départ accordé : on compte les changements d'identité par `last_blast` / niveau.
        let level = meteors.flash().level();
        if level > 0.0 && levels.last().is_none_or(|&l| l == 0.0) {
            grants.push(i);
        }
        granted_before = granted_before.max(granted);
        levels.push(level);
    }
    assert!(granted_before == 1, "au moins un éclair est accordé");
    assert!(meteors.field().spawned() >= 12, "les astres tombent");
    assert!(levels.iter().all(|&l| l <= MAX_FLASH_PEAK + 1e-6), "plafond de la voie");
    let max_step = MAX_FLASH_PEAK * DT / MIN_FADE + 1e-3;
    for pair in levels.windows(2) {
        assert!((pair[0] - pair[1]).abs() <= max_step, "variation trop rapide");
    }
    let window = (1.0 / DT) as usize;
    for &s in &grants {
        let in_second = grants.iter().filter(|&&t| t >= s && t < s + window).count();
        assert!(in_second <= MAX_FLASHES_PER_SECOND, "{in_second} éclairs en 1 s");
    }
}

#[test]
fn flashs_coupes_ou_respiration_aucun_eclair_meme_pendant_une_pluie() {
    const DT: f32 = 1.0 / 60.0;
    for policy in [Some((false, false)), Some((true, true)), None] {
        let mut meteors = Meteors::new(9);
        let anchor = DVec3::new(0.0, 0.0, -5000.0);
        let mut wall = |at: DVec3, _: f64| at.y < 60.0;
        meteors.field_mut().start_shower(12, 4.0);
        for i in 0..(10.0 / DT) as usize {
            let input = FieldInput {
                dt_s: DT,
                now_s: f64::from(DT) * i as f64,
                anchor,
                calm: false,
                show: None,
                textures: 3,
            };
            meteors.step(&input, policy, &mut wall);
            assert_eq!(meteors.flash().level(), 0.0, "{policy:?}");
            assert!(meteors.blasts().slots().iter().all(|b| !b.flash_granted), "{policy:?}");
        }
        assert!(
            meteors.last_blast().is_some(),
            "les explosions ont lieu, sans éclair : {policy:?}"
        );
    }
}
