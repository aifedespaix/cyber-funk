//! Les astres du monde face aux plans du réalisateur : `meteor-ride` demande un astre (`LaunchMeteor`), le monde le lance
//! et le publie en sujet, le plan l'adopte, **tient** (`holding`) pendant toute la chute et rend la main après l'explosion
//! (`last_blast`). Avant J7-10 personne ne lisait la demande et le plan s'arrêtait seul après 1,2 s.
//!
//! Sans rendu : la logique des astres (`Meteors`) et les plans de `cf-cinema` se parlent par `CinemaFrame`, comme dans
//! l'application.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use bevy::math::DVec3;
use cf_cinema::ShotSpec;
use cf_cinema::rng::CinemaRng;
use cf_cinema::shots::{MeteorImpactWatch, MeteorRide};
use cf_cinema::world::{CinemaFrame, CinemaWorld, Obstacles};
use cf_render::{CameraPose, Subject};
use cf_world_neon::meteors::{FieldInput, Meteors, ShowGate};

const DT: f32 = 1.0 / 60.0;
const CAR: DVec3 = DVec3::new(0.0, 0.0, -5000.0);

/// Le mur de la ville : tout ce qui descend sous 40 m au-dessus de la brume le touche (comme le test des astres).
struct Wall;

impl Obstacles for Wall {
    fn collides(&self, at: DVec3, _radius: f64) -> bool {
        at.y < 40.0
    }
}

/// Une nuit en miniature : la voiture, des astres qui s'écrasent contre un mur à 40 m au-dessus de la brume.
struct Night {
    world: CinemaWorld,
    meteors: Meteors,
    subjects: Vec<Subject>,
    clock: f64,
}

impl Night {
    fn new() -> Self {
        let mut night = Self {
            world: CinemaWorld::default(),
            meteors: Meteors::new(5),
            subjects: Vec::new(),
            clock: 1.0,
        };
        night.world.car = CAR;
        night.world.live = true;
        night.world.time = night.clock;
        night
    }

    /// Une image : le monde fait avancer les astres, publie sujets et dernier impact (comme `build_cinema_world`).
    fn advance(&mut self) {
        self.clock += f64::from(DT);
        let input = FieldInput {
            dt_s: DT,
            now_s: self.clock,
            anchor: CAR,
            calm: false,
            // Cadence nulle : seul l'astre que le plan demande tombe.
            show: Some(ShowGate {
                meteor_rate: 0.0,
                meteor_shower: false,
                breathing: false,
                drop_now: false,
            }),
            textures: 7,
        };
        let mut wall = |at: DVec3, _: f64| at.y < 40.0;
        self.meteors.step(&input, None, &mut wall);
        self.world.time = self.clock;
        self.world.dt = DT;
        self.world.last_blast = self.meteors.cinema_blast();
        self.subjects.clear();
        let subjects = &mut self.subjects;
        self.meteors.collect_subjects(|s| subjects.push(s));
    }

    fn frame(&self) -> CinemaFrame<'_> {
        CinemaFrame::new(&self.world, &Wall, &self.subjects)
    }

    fn launch(&mut self) {
        self.meteors.launch(CAR, false, 7).expect("place libre");
    }
}

#[test]
fn la_chevauchee_tient_pendant_toute_la_chute_puis_regarde_l_explosion() {
    let mut night = Night::new();
    night.advance();
    let mut shot = MeteorRide.create(&night.frame(), &mut CinemaRng::new(1));
    assert!(shot.take_meteor_launch(), "aucun astre en vol : le plan en demande un");
    // Le monde répond au message `LaunchMeteor`.
    night.launch();
    let mut pose = CameraPose::default();
    let mut elapsed = 0.0_f32;
    let mut held_while_flying = 0;
    let mut flying_frames = 0;
    let mut blast_seen_at = None;
    let mut alive = true;
    while alive && elapsed < 30.0 {
        night.advance();
        alive = shot.update(&night.frame(), &mut pose);
        elapsed += DT;
        assert!(
            pose.rotation.is_finite() && pose.position.is_finite(),
            "pose finie à {elapsed} s"
        );
        if night.meteors.field().active().next().is_some() {
            flying_frames += 1;
            held_while_flying += i32::from(shot.holding());
        }
        if blast_seen_at.is_none() && night.world.last_blast.is_some() {
            blast_seen_at = Some(elapsed);
        }
    }
    assert!(!alive, "le plan finit seul");
    let blast_at = blast_seen_at.expect("l'astre s'est écrasé contre le mur");
    assert!(
        flying_frames > 90,
        "la chute dure plus d'une seconde ({flying_frames} images)"
    );
    // Dès que le plan a adopté l'astre (quelques images), il tient jusqu'à l'explosion.
    assert!(
        held_while_flying + 6 >= flying_frames,
        "le plan tient pendant la chute ({held_while_flying} / {flying_frames})"
    );
    assert!(
        elapsed > blast_at,
        "le plan regarde l'explosion avant de rendre la main"
    );
    assert!(elapsed > 1.5, "plus de 1,2 s : l'astre existe maintenant");
    assert!(!shot.holding(), "l'explosion est passée : la coupe est libre");
}

#[test]
fn le_plan_d_impact_adopte_l_astre_publie_et_tient_jusqu_a_l_explosion() {
    let mut night = Night::new();
    // Un gros astre dont la trajectoire finit dans le mur, assez loin devant : on lance jusqu'à en trouver un.
    let mut found = false;
    for _ in 0..400 {
        night.launch();
        night.advance();
        if MeteorImpactWatch.available(&night.frame()) {
            found = true;
            break;
        }
        // On retire les astres ratés en les laissant tomber : on avance jusqu'à ce que le pool soit vide.
        while night.meteors.field().active().next().is_some() {
            night.advance();
        }
    }
    assert!(
        found,
        "un gros astre menace un immeuble : le plan d'impact est possible"
    );
    let mut shot = MeteorImpactWatch.create(&night.frame(), &mut CinemaRng::new(2));
    let mut pose = CameraPose::default();
    let mut alive = true;
    let mut frames = 0;
    let mut held = 0;
    let mut flying = 0;
    while alive && frames < 60 * 40 {
        night.advance();
        alive = shot.update(&night.frame(), &mut pose);
        frames += 1;
        if alive && night.meteors.field().active().next().is_some() {
            flying += 1;
            held += i32::from(shot.holding());
        }
    }
    assert!(!alive, "le plan finit après l'explosion");
    assert!(
        flying > 20 && held == flying,
        "le plan tient tant que l'astre tombe ({held} / {flying})"
    );
    assert!(
        night.world.last_blast.is_some(),
        "l'explosion est publiée au réalisateur"
    );
}
