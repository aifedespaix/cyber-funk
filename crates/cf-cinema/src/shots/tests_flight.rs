//! Tests du lot B (J6-04) : plans calés sur les animations de la voiture, figure, cascades et plans de vitesse.
//!
//! Ces plans ne jouent que si le monde le permet (voiture qui décolle, figure, cascade, drop de haut niveau) : la soirée
//! ordinaire de [`crate::testkit`] ne les déclenche pas. Chacun est donc rejoué sur une soirée « scénarisée » qui lui
//! offre ses occasions, 30 s simulées, avec les mêmes contrôles de pose que le lot A.

use bevy::math::{DVec3, Quat, Vec3};
use cf_render::CameraPose;

use super::{AscensionCam, Barrel, BulletTime, CityWide, DodgePass, FpvDive, StuntCam, Takeoff, Touchdown, catalog};
use crate::choreo::{MAX_CUTS_PER_SECOND, NoShow, ShowSide, WINDOW_SECONDS, is_jump};
use crate::director::CinemaDirector;
use crate::editor::is_strobing;
use crate::pose::{is_finite, smootherstep, smoothstep};
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::shots::Chase;
use crate::testkit::{BoxCity, DT, Sim, check_pose};
use crate::types::{FlightPhase, RingPlanet, Stunt, Trick, TrickKind};
use crate::world::{CinemaFrame, CinemaWorld, NoObstacles};

const FRAMES: usize = (30.0 / DT as f64) as usize;

/// Les neuf plans du lot B.
const LOT_B: [&str; 9] = [
    "takeoff",
    "touchdown",
    "dodge-pass",
    "barrel",
    "stunt-cam",
    "ascension-cam",
    "bullet-time",
    "fpv-dive",
    "city-wide",
];

/// Genres de cascade joués par la soirée des cascades, 3 s chacun.
const STUNTS: [&str; 10] = [
    "wall-ride",
    "off-road",
    "head-on",
    "rooftop",
    "sign-hop",
    "big-jump",
    "loop",
    "near-miss",
    "slalom",
    "inverted",
];

/// La soirée offerte au plan.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Scene {
    /// Un cycle de 16 s : décollage, figure, vol, atterrissage, drop, esquive (le plan à l'antenne a le temps de dépasser sa garde avant chaque animation).
    Flight,
    /// Une cascade de chaque genre, 3 s chacune.
    Stunts,
    /// Au sol, une figure (tonneau puis salto) toutes les 7 s à partir de 8 s : le plan à l'antenne a dépassé sa garde.
    Tricks,
    /// L'Ascension sur 30 s, avec ou sans la planète à anneau.
    Ascension { planet: bool },
}

fn scene_for(id: &str) -> Scene {
    match id {
        "stunt-cam" => Scene::Stunts,
        "barrel" => Scene::Tricks,
        "ascension-cam" => Scene::Ascension { planet: true },
        _ => Scene::Flight,
    }
}

fn spec_of(id: &str) -> Box<dyn ShotSpec> {
    match id {
        "takeoff" => Box::new(Takeoff),
        "touchdown" => Box::new(Touchdown),
        "dodge-pass" => Box::new(DodgePass),
        "barrel" => Box::new(Barrel),
        "stunt-cam" => Box::new(StuntCam),
        "ascension-cam" => Box::new(AscensionCam),
        "bullet-time" => Box::new(BulletTime),
        "fpv-dive" => Box::new(FpvDive),
        _ => Box::new(CityWide),
    }
}

/// Pose l'état de la voiture de l'image selon la soirée (après `Sim::step`).
fn drive(world: &mut CinemaWorld, scene: Scene) {
    let t = world.time as f32;
    world.drop = false;
    world.show.drop_now = false;
    match scene {
        Scene::Flight => flight(world, t),
        Scene::Stunts => stunts(world, t),
        Scene::Tricks => tricks(world, t),
        Scene::Ascension { planet } => ascension(world, t, planet),
    }
}

/// Fixe l'altitude, la vitesse verticale et la phase de vol de l'image.
fn set_altitude(world: &mut CinemaWorld, altitude: f32) {
    let previous = world.altitude;
    world.altitude = altitude;
    world.car.y = f64::from(altitude);
    world.animation.climb = (altitude - previous) / DT;
}

fn flight(world: &mut CinemaWorld, t: f32) {
    let c = t % 16.0;
    let cycle = (t / 16.0) as u32;
    if (8.0..8.0 + DT * 1.5).contains(&c) {
        world.drop = true;
        world.show.drop_now = true;
    }
    let altitude = match c {
        c if c < 2.5 => 0.0,
        c if c < 4.5 => 30.0 * smoothstep((c - 2.5) / 2.0),
        c if c < 6.0 => 30.0,
        c if c < 7.5 => 30.0 * (1.0 - smoothstep((c - 6.0) / 1.5)),
        _ => 0.0,
    };
    set_altitude(world, altitude);
    let (phase, hover) = match c {
        c if c < 1.0 => (FlightPhase::Ground, 0.0),
        c if c < 2.5 => (FlightPhase::Deploying, (c - 1.0) / 1.5),
        c if c < 6.8 => (FlightPhase::Flying, 1.0),
        c if c < 7.5 => (FlightPhase::Retracting, 1.0 - (c - 6.8) / 0.7),
        _ => (FlightPhase::Ground, 0.0),
    };
    world.animation.phase = phase;
    world.animation.hover = hover;
    world.animation.dodge = if (11.0..12.5).contains(&c) {
        if cycle.is_multiple_of(2) { 1 } else { -1 }
    } else {
        0
    };
    world.animation.roll = if world.animation.dodge == 0 { 0.0 } else { 0.4 };
    // Une figure en plein vol : tonneau les cycles pairs, salto les autres.
    if (4.0..5.2).contains(&c) {
        let progress = (c - 4.0) / 1.2;
        let kind = if cycle.is_multiple_of(2) {
            TrickKind::Roll
        } else {
            TrickKind::Flip
        };
        world.trick = Some(Trick { kind, progress });
        world.car_rotation = match kind {
            TrickKind::Roll => Quat::from_rotation_z(progress * std::f32::consts::TAU),
            TrickKind::Flip => Quat::from_rotation_x(progress * std::f32::consts::TAU),
        };
    } else {
        world.trick = None;
        world.car_rotation = Quat::IDENTITY;
    }
}

fn tricks(world: &mut CinemaWorld, t: f32) {
    let since = (t - 8.0) % 7.0;
    if t >= 8.0 && since < 1.2 {
        let progress = since / 1.2;
        let kind = if (((t - 8.0) / 7.0) as u32).is_multiple_of(2) {
            TrickKind::Roll
        } else {
            TrickKind::Flip
        };
        world.trick = Some(Trick { kind, progress });
        world.car_rotation = Quat::from_rotation_z(progress * std::f32::consts::TAU);
    } else {
        world.trick = None;
        world.car_rotation = Quat::IDENTITY;
    }
}

fn stunts(world: &mut CinemaWorld, t: f32) {
    let index = (t / 3.0) as usize % STUNTS.len();
    let progress = (t % 3.0) / 3.0;
    let kind = STUNTS[index];
    let lofty = matches!(kind, "big-jump" | "loop");
    set_altitude(
        world,
        if lofty {
            20.0 * (progress * std::f32::consts::PI).sin()
        } else {
            0.0
        },
    );
    world.animation.phase = if lofty {
        FlightPhase::Flying
    } else {
        FlightPhase::Ground
    };
    world.car_rotation = if kind == "inverted" {
        Quat::from_rotation_z(progress * std::f32::consts::TAU)
    } else {
        Quat::IDENTITY
    };
    world.stunt = Some(Stunt {
        kind,
        progress,
        focus: world.car + DVec3::new(0.0, 0.0, -60.0),
    });
}

fn ascension(world: &mut CinemaWorld, t: f32, planet: bool) {
    let p = (t / 30.0).min(1.0);
    let altitude = if p < 0.4 {
        110.0 * smootherstep(p / 0.4)
    } else if p < 0.5 {
        110.0
    } else {
        110.0 * (1.0 - smootherstep((p - 0.5) / 0.4))
    };
    set_altitude(world, altitude);
    world.animation.phase = if altitude > 1.0 {
        FlightPhase::Flying
    } else {
        FlightPhase::Ground
    };
    world.stunt = Some(Stunt {
        kind: "ascension",
        progress: p,
        focus: world.car,
    });
    world.ring_planet = planet.then_some(RingPlanet {
        center: Vec3::new(2000.0, 800.0, -3000.0),
        radius: 900.0,
        ring_inner: 1200.0,
        ring_outer: 1900.0,
        ring_normal: Vec3::new(0.3, 1.0, 0.2).normalize(),
    });
}

/// Une soirée de test dans laquelle `drive` pose l'état de la voiture à chaque image.
fn night(scene: Scene) -> Sim {
    let mut sim = Sim::new(true);
    sim.step();
    drive(&mut sim.world, scene);
    sim
}

fn advance(sim: &mut Sim, scene: Scene) {
    sim.step();
    drive(&mut sim.world, scene);
}

#[test]
fn le_catalogue_contient_les_neuf_plans_du_lot_b() {
    let ids: Vec<&str> = catalog().iter().map(|s| s.id()).collect();
    for id in LOT_B {
        assert_eq!(
            ids.iter().filter(|i| **i == id).count(),
            1,
            "{id} : absent ou en double"
        );
    }
    assert!(!ids.contains(&"strobe"), "le strobe reste hors périmètre");
    for id in LOT_B {
        assert!(!is_strobing(id), "{id} ne doit pas enchaîner de coupes à chaque temps");
    }
}

#[test]
fn les_fiches_reprennent_celles_de_sillage() {
    // (id, rang, énergie, min, max, scripté, réactif, cooldown, poids)
    let sillage = [
        ("takeoff", "uncommon", (0.0, 1.0), 3.0, 5.5, true, true, 25.0, 2.0),
        ("touchdown", "uncommon", (0.0, 1.0), 3.0, 6.0, true, true, 25.0, 2.0),
        ("dodge-pass", "uncommon", (0.1, 1.0), 3.0, 6.0, true, true, 30.0, 1.6),
        ("barrel", "uncommon", (0.0, 1.0), 1.9, 2.4, true, false, 14.0, 2.0),
        ("stunt-cam", "uncommon", (0.0, 1.0), 3.0, 9.0, false, false, 6.0, 3.0),
        (
            "ascension-cam",
            "uncommon",
            (0.0, 1.0),
            8.0,
            40.0,
            true,
            false,
            20.0,
            4.0,
        ),
        ("bullet-time", "epic", (0.4, 1.0), 3.5, 9.0, true, false, 300.0, 1.0),
        ("fpv-dive", "rare", (0.4, 1.0), 4.0, 7.0, true, false, 200.0, 1.0),
        ("city-wide", "uncommon", (0.0, 0.5), 9.0, 16.0, false, false, 90.0, 1.0),
    ];
    for (id, tier, energy, min, max, scripted, reactive, cooldown, weight) in sillage {
        let spec = spec_of(id);
        assert_eq!(spec.id(), id);
        assert_eq!(spec.tier().as_str(), tier, "{id}");
        assert_eq!(spec.energy(), energy, "{id}");
        assert!((spec.min_seconds() - min).abs() < 1e-6, "{id}");
        assert!((spec.max_seconds() - max).abs() < 1e-6, "{id}");
        assert_eq!(spec.scripted(), scripted, "{id}");
        assert_eq!(spec.reactive(), reactive, "{id}");
        assert!((spec.cooldown() - cooldown).abs() < 1e-6, "{id}");
        assert!((spec.weight() - weight).abs() < 1e-6, "{id}");
    }
}

#[test]
fn chaque_plan_du_lot_b_brut_reste_fini_pendant_30_s() {
    for id in LOT_B {
        let spec = spec_of(id);
        let scene = scene_for(id);
        let mut sim = night(scene);
        let mut rng = CinemaRng::new(11);
        let mut shot: Option<Box<dyn Shot>> = None;
        let mut played = 0usize;
        for _ in 0..FRAMES {
            advance(&mut sim, scene);
            let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
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
            played >= 60,
            "{id} : joué {played} images sur {FRAMES}, trop peu pour valider"
        );
    }
}

#[test]
fn l_ascension_sans_planete_reste_finie() {
    let scene = Scene::Ascension { planet: false };
    let spec = AscensionCam;
    let mut sim = night(scene);
    let mut rng = CinemaRng::new(3);
    let mut shot: Option<Box<dyn Shot>> = None;
    for _ in 0..FRAMES {
        advance(&mut sim, scene);
        let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
        if shot.is_none() && spec.available(&frame) {
            shot = Some(spec.create(&frame, &mut rng));
        }
        if let Some(current) = shot.as_mut() {
            let mut pose = CameraPose::default();
            current.update(&frame, &mut pose);
            assert!(is_finite(&pose), "pose non finie à t={}", frame.time);
        }
    }
    assert!(shot.is_some());
}

#[test]
fn chaque_plan_du_lot_b_par_le_realisateur_reste_dans_les_bornes_pendant_30_s() {
    for id in LOT_B {
        let scene = scene_for(id);
        let mut director = CinemaDirector::new(vec![Box::new(Chase), spec_of(id)], false, 7);
        let mut sim = night(scene);
        director.start(&CameraPose::default(), &CinemaFrame::new(&sim.world, &BoxCity, &[]));
        // Le plan épique n'a pas le droit d'antenne avant 90 s de show, et tire au sort sur chaque drop : on le laisse
        // chauffer, puis on observe plus longtemps. Les autres sont jugés sur leurs 30 premières secondes.
        let (warm, frames) = if id == "bullet-time" {
            (95.0, (90.0 / DT) as usize)
        } else {
            (0.0, FRAMES)
        };
        while sim.world.time < warm {
            advance(&mut sim, scene);
            director.update(&CinemaFrame::new(&sim.world, &BoxCity, &[]));
        }
        let mut on_air = 0usize;
        for _ in 0..frames {
            advance(&mut sim, scene);
            let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
            let pose = *director.update(&frame);
            check_pose(&pose, &frame);
            if director.shot_id() == Some(id) {
                on_air += 1;
            }
        }
        assert!(on_air > 0, "{id} : jamais à l'antenne");
    }
}

/// Rejoue 8 s de route calme (le plan d'ouverture a dépassé sa garde), puis laisse `start` déclencher l'animation :
/// renvoie le délai (s) avant que le plan `expected` soit à l'antenne, ou `None`.
fn reaction_delay(expected: &str, start: fn(&mut CinemaWorld, f32)) -> Option<f32> {
    let mut director = CinemaDirector::new(
        vec![
            Box::new(Chase),
            Box::new(Takeoff),
            Box::new(Touchdown),
            Box::new(DodgePass),
        ],
        false,
        5,
    );
    let mut sim = Sim::new(true);
    sim.step();
    director.start(&CameraPose::default(), &CinemaFrame::new(&sim.world, &BoxCity, &[]));
    while sim.world.time < 8.0 {
        sim.step();
        let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
        director.update(&frame);
        assert_ne!(director.shot_id(), Some(expected), "plan réactif sans animation");
    }
    let begun = sim.world.time;
    for _ in 0..(3.0 / DT) as usize {
        sim.step();
        let since = (sim.world.time - begun) as f32;
        start(&mut sim.world, since);
        let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
        director.update(&frame);
        if director.shot_id() == Some(expected) {
            return Some((sim.world.time - begun) as f32);
        }
    }
    None
}

/// Délai maximal (s) entre le début de l'animation et la bascule : l'animation dure plus de 1 s, le plan doit être là
/// bien avant sa fin (« dès le début »).
const REACTION_BUDGET: f32 = 0.25;

#[test]
fn le_decollage_bascule_des_le_debut_du_deploiement() {
    let delay = reaction_delay("takeoff", |world, since| {
        world.animation.phase = FlightPhase::Deploying;
        world.animation.hover = (since / 1.5).min(1.0);
    });
    assert!(delay.is_some_and(|d| d <= REACTION_BUDGET), "délai : {delay:?}");
}

#[test]
fn l_atterrissage_bascule_des_le_debut_des_roues_qui_se_replient() {
    let delay = reaction_delay("touchdown", |world, since| {
        world.animation.phase = FlightPhase::Retracting;
        world.animation.hover = (1.0 - since / 0.7).max(0.0);
        set_altitude(world, (8.0 - since * 8.0).max(0.0));
    });
    assert!(delay.is_some_and(|d| d <= REACTION_BUDGET), "délai : {delay:?}");
}

#[test]
fn l_esquive_bascule_des_le_debut_du_mouvement() {
    let delay = reaction_delay("dodge-pass", |world, _| {
        world.animation.dodge = 1;
    });
    assert!(delay.is_some_and(|d| d <= REACTION_BUDGET), "délai : {delay:?}");
}

#[test]
fn les_plans_reactifs_ne_jouent_que_pendant_leur_animation() {
    let mut sim = Sim::new(true);
    sim.step();
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
    for spec in [&Takeoff as &dyn ShotSpec, &Touchdown, &DodgePass] {
        assert!(spec.reactive());
        assert!(
            !spec.available(&frame),
            "{} disponible au sol sans animation",
            spec.id()
        );
        assert!(spec.opportunity(&frame) < 0.7);
    }
    let mut world = sim.world;
    world.animation.phase = FlightPhase::Deploying;
    assert!(Takeoff.available(&CinemaFrame::new(&world, &BoxCity, &[])));
    world.trick = Some(Trick {
        kind: TrickKind::Roll,
        progress: 0.1,
    });
    assert!(
        !Takeoff.available(&CinemaFrame::new(&world, &BoxCity, &[])),
        "pas pendant une figure"
    );
}

/// Un faux chef du spectacle qui retient les ralentis demandés.
#[derive(Default)]
struct Recorder {
    inner: NoShow,
    scales: Vec<(f32, f32)>,
}

impl ShowSide for Recorder {
    fn gates(&self) -> Option<&cf_show::model::ShowGates> {
        self.inner.gates()
    }
    fn heat(&self) -> f32 {
        self.inner.heat()
    }
    fn drop_plan(&self) -> Option<&cf_show::drop::DropPlan> {
        self.inner.drop_plan()
    }
    fn slot_seq(&self) -> u32 {
        self.inner.slot_seq()
    }
    fn take(&mut self, order: cf_show::director::Order) -> bool {
        self.inner.take(order)
    }
    fn freshness(&self) -> &cf_show::freshness::Freshness {
        self.inner.freshness()
    }
    fn freshness_mut(&mut self) -> &mut cf_show::freshness::Freshness {
        self.inner.freshness_mut()
    }
    fn request_time_scale(&mut self, scale: f32, seconds: f32) {
        self.scales.push((scale, seconds));
    }
}

#[test]
fn le_bullet_time_demande_son_ralenti_une_seule_fois() {
    let mut sim = Sim::new(true);
    sim.step();
    sim.world.show.drop_now = true;
    let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
    assert!(BulletTime.available(&frame));
    let mut rng = CinemaRng::new(9);
    let mut shot = BulletTime.create(&frame, &mut rng);
    let (scale, seconds) = shot.take_time_scale().expect("demande");
    assert!((0.15..=0.25).contains(&scale), "{scale}");
    assert!((1.6..=4.5).contains(&seconds), "{seconds}");
    assert!(shot.take_time_scale().is_none(), "une seule demande");
    let mut pose = CameraPose::default();
    assert!(shot.holding());
    shot.update(&frame, &mut pose);
    assert!(shot.holding(), "on ne coupe pas au milieu du ralenti");
}

#[test]
fn le_bullet_time_exige_un_spectacle_de_haut_niveau_et_un_drop() {
    let mut sim = Sim::new(true);
    sim.step();
    sim.world.show.drop_now = true;
    let mut world = sim.world;
    assert!(BulletTime.available(&CinemaFrame::new(&world, &BoxCity, &[])));
    world.show.level = 3;
    assert!(
        !BulletTime.available(&CinemaFrame::new(&world, &BoxCity, &[])),
        "niveau 3"
    );
    world.show.level = 4;
    world.show.drop_now = false;
    assert!(
        !BulletTime.available(&CinemaFrame::new(&world, &BoxCity, &[])),
        "pas de drop"
    );
    world.drop = true;
    assert!(BulletTime.available(&CinemaFrame::new(&world, &BoxCity, &[])));
    world.show.active = false;
    assert!(
        !BulletTime.available(&CinemaFrame::new(&world, &BoxCity, &[])),
        "sans spectacle"
    );
}

#[test]
fn le_realisateur_transmet_le_ralenti_du_bullet_time_au_chef_du_spectacle() {
    let mut asked = Vec::new();
    for seed in 0..8 {
        let mut director = CinemaDirector::new(vec![Box::new(Chase), Box::new(BulletTime)], false, seed);
        let mut show = Recorder::default();
        let mut sim = night(Scene::Flight);
        director.start_with(
            &CameraPose::default(),
            &CinemaFrame::new(&sim.world, &BoxCity, &[]),
            &mut show,
        );
        for _ in 0..(150.0 / DT) as usize {
            advance(&mut sim, Scene::Flight);
            let frame = CinemaFrame::new(&sim.world, &BoxCity, &[]);
            director.update_with(&frame, &mut show);
        }
        asked.extend(show.scales);
    }
    assert!(!asked.is_empty(), "aucun ralenti demandé sur 8 soirées de 150 s");
    for (scale, seconds) in asked {
        assert!(
            (0.15..=0.25).contains(&scale) && (1.6..=4.5).contains(&seconds),
            "{scale} {seconds}"
        );
    }
}

#[test]
fn les_cascades_ne_depassent_pas_le_plafond_de_coupes() {
    let scene = Scene::Stunts;
    let mut director = CinemaDirector::new(
        vec![
            Box::new(Chase),
            Box::new(StuntCam),
            Box::new(Barrel),
            Box::new(CityWide),
        ],
        false,
        13,
    );
    let mut sim = night(scene);
    let start = CameraPose {
        position: sim.world.car + DVec3::new(0.0, 3.0, 8.0),
        ..CameraPose::default()
    };
    director.start(&start, &CinemaFrame::new(&sim.world, &NoObstacles, &[]));
    let mut cuts = Vec::new();
    let mut previous = start;
    let mut previous_car = sim.world.car;
    let mut stunt_air = 0usize;
    while sim.world.time < 120.0 {
        advance(&mut sim, scene);
        let frame = CinemaFrame::new(&sim.world, &NoObstacles, &[]);
        let pose = *director.update(&frame);
        check_pose(&pose, &frame);
        if is_jump(&previous, previous_car, &pose, frame.car, frame.dt) {
            cuts.push(frame.time);
        }
        if director.shot_id() == Some("stunt-cam") {
            stunt_air += 1;
        }
        previous = pose;
        previous_car = frame.car;
    }
    assert!(
        stunt_air > 60,
        "la caméra de cascade n'a presque pas joué : {stunt_air} images"
    );
    let busiest = cuts
        .iter()
        .map(|&t| cuts.iter().filter(|&&c| c > t - WINDOW_SECONDS && c <= t).count())
        .max()
        .unwrap_or(0);
    assert!(
        busiest <= MAX_CUTS_PER_SECOND as usize,
        "{busiest} coupes dans une seconde glissante"
    );
}

/// Le plan de cascade seul (sans réalisateur, route dégagée) ne fait aucun saut de pose : un plan continu n'est jamais
/// une coupe, donc ne peut pas contribuer au plafond ni à une variation brusque de l'image. Seule la jonction entre deux
/// cascades (un plan neuf) en fait un, et c'est le réalisateur qui la compte.
#[test]
fn la_camera_de_cascade_glisse_sans_saut_pendant_un_meme_genre() {
    let mut sim = night(Scene::Stunts);
    let spec = StuntCam;
    let mut rng = CinemaRng::new(2);
    let mut shot: Option<Box<dyn Shot>> = None;
    let mut previous: Option<(CameraPose, DVec3)> = None;
    let mut kind = None;
    for _ in 0..FRAMES {
        advance(&mut sim, Scene::Stunts);
        let frame = CinemaFrame::new(&sim.world, &NoObstacles, &[]);
        let now = frame.stunt.map(|s| s.kind);
        if now != kind {
            // Nouvelle cascade : un nouveau plan, la jonction relève de la transition du réalisateur.
            shot = Some(spec.create(&frame, &mut rng));
            previous = None;
            kind = now;
        }
        let Some(current) = shot.as_mut() else {
            continue;
        };
        let mut pose = CameraPose::default();
        current.update(&frame, &mut pose);
        if let Some((before, before_car)) = previous {
            let jump = is_jump(&before, before_car, &pose, frame.car, frame.dt);
            // Le genre « inverted » est solidaire de la caisse qui tourne : lui seul peut pivoter vite.
            let moved = ((pose.position - frame.car) - (before.position - before_car)).length();
            let angle = before.rotation.angle_between(pose.rotation);
            let dfov = pose.fov_deg - before.fov_deg;
            assert!(
                !jump || kind == Some("inverted"),
                "saut de pose en {kind:?} à t={} : {moved} m, {angle} rad, {dfov}°",
                frame.time
            );
        }
        previous = Some((pose, frame.car));
    }
}
