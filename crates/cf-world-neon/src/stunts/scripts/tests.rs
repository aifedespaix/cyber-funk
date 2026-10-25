//! Les douze cascades sans Bevy : 30 s simulées sans pose invalide (NaN, mur, hors bornes), respect de `max_floor` (aucun
//! vol au niveau 0, cascade coupée en route), déterminisme par graine, véhicules toujours rendus. Les scripts tournent
//! d'abord seuls (monde de test), puis derrière le vrai directeur et le registre complet.

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_show::model::{ShowGates, StuntKind as ShowStuntKind, StuntTier as ShowStuntTier};

use super::*;
use crate::pilot::flight::Obstruction;
use crate::stunts::kind::{STUNT_COUNT, StuntKind};
use crate::stunts::permit::StuntLink;
use crate::stunts::script::{Planned, StuntEnv, StuntRegistry, StuntScript, StuntShow};
use crate::stunts::{StuntDirector, StuntFrame};
use crate::traffic::{Craft, CraftId, Fleet, TrafficBeat};

const DT: f64 = 1.0 / 60.0;
/// Axe de la façade des premiers immeubles dans la ville de test.
const FACADE: f64 = 13.5;
/// Marge numérique des comparaisons de pose (m).
const EPS: f64 = 1e-6;

/// Le monde de test : une vraie flotte (le pool de véhicules) et, au choix, une ville (façades, toits, portique).
#[derive(Debug)]
struct TestWorld {
    fleet: Fleet,
    city: bool,
    held: bool,
}

impl TestWorld {
    fn new(city: bool) -> Self {
        Self {
            fleet: Fleet::default(),
            city,
            held: false,
        }
    }

    /// Véhicules vivants dans la flotte.
    fn live(&self) -> usize {
        self.fleet.crafts().iter().filter(|c| c.active).count()
    }

    /// Hauteur du toit à la profondeur `z` (m) : varie de 12 à 22 m, sous l'altitude de croisière des toits (31 m).
    fn roof(z: f64) -> f32 {
        (12.0 + 10.0 * (z / 37.0).sin().abs()) as f32
    }
}

impl StuntWorld for TestWorld {
    fn spawn(&mut self, variant: u8, at: DVec3, scale: f32, _tint: i64) -> Option<CraftId> {
        let mut rng = Rand::new(1);
        self.fleet
            .spawn(variant, at, bevy::math::Vec3::ZERO, LinearRgba::WHITE, scale, &mut rng)
    }
    fn release(&mut self, id: CraftId) {
        self.fleet.release(id);
    }
    fn craft_mut(&mut self, id: CraftId) -> Option<&mut Craft> {
        self.fleet.get_mut(id)
    }
    fn roof_at(&self, x: f64, z: f64) -> Option<f32> {
        (self.city && x.abs() > FACADE).then(|| Self::roof(z))
    }
    fn facade(&self, _side: i8, _z: f64, _depth: f64) -> Option<f32> {
        self.city.then_some(FACADE as f32)
    }
    fn overhead(&self, out: &mut Vec<Obstruction>, car_z: f64) {
        // Un portique néon devant la voiture, toujours à 150 m : de quoi jouer le saut d'enseigne.
        if self.city {
            out.push(Obstruction {
                z_near: car_z - 148.0,
                z_far: car_z - 152.0,
                lo: 6.0,
                hi: 12.0,
            });
        }
    }
    fn bridge_ahead(&self, _car_z: f64, _distance: f64, _margin: f64) -> bool {
        false
    }
    fn busy(&self) -> bool {
        false
    }
    fn hold(&mut self, on: bool) {
        self.held = on;
    }
}

/// Le script d'une cascade.
fn script_of(kind: StuntKind) -> Box<dyn StuntScript> {
    match kind {
        StuntKind::OffRoad => Box::new(OffRoad),
        StuntKind::Slalom => Box::new(Slalom),
        StuntKind::NearMiss => Box::new(NearMiss),
        StuntKind::Loop => Box::new(Loop),
        StuntKind::HeadOn => Box::new(HeadOn),
        StuntKind::Inverted => Box::new(Inverted),
        StuntKind::WallRide => Box::new(WallRide),
        StuntKind::Threading => Box::new(Threading),
        StuntKind::SignHop => Box::new(SignHop),
        StuntKind::Rooftop => Box::new(Rooftop),
        StuntKind::BigJump => Box::new(BigJump),
        StuntKind::Ascension => Box::new(Ascension),
    }
}

/// Toutes les cascades, dans l'ordre du catalogue.
fn every_script() -> Vec<Box<dyn StuntScript>> {
    StuntKind::ALL.into_iter().map(script_of).collect()
}

/// Les entrées d'une image : la voiture file à 60 m/s, serpente un peu, un kick toutes les 0,5 s.
fn env_at(time: f64, show: Option<StuntShow<'_>>) -> StuntEnv<'_> {
    let phase = time.rem_euclid(0.5);
    StuntEnv {
        dt: DT,
        time,
        car_x: 2.0 * (time * 0.3).sin(),
        car_z: -time * 60.0,
        altitude: 0.0,
        speed: 60.0,
        period: 0.5,
        beat: TrafficBeat {
            kick: phase < DT,
            pulse: (1.0 - phase * 2.0) as f32,
            bpm: Some(120.0),
            ..TrafficBeat::SILENT
        },
        show,
    }
}

/// Une pose est-elle valide ? Retourne la raison du refus.
fn invalid(pose: &StuntPose) -> Option<String> {
    let numbers = [
        pose.x,
        pose.altitude,
        pose.pitch,
        pose.roll,
        pose.focus.x,
        pose.focus.y,
        pose.focus.z,
    ];
    if numbers.iter().any(|n| !n.is_finite()) {
        return Some(format!("pose non finie : {pose:?}"));
    }
    // Hors bornes : sous la route, trop haut (l'Ascension monte à 250 m), hors de la ville, plus de deux tours.
    if pose.altitude < -EPS || pose.altitude > 300.0 {
        return Some(format!("altitude hors bornes : {}", pose.altitude));
    }
    if pose.x.abs() > 25.0 {
        return Some(format!("abscisse hors bornes : {}", pose.x));
    }
    let two_turns = 2.0 * std::f64::consts::TAU + 0.5;
    if pose.pitch.abs() > two_turns || pose.roll.abs() > two_turns {
        return Some(format!("angle hors bornes : {pose:?}"));
    }
    None
}

/// Mur : la pose à la profondeur `z` est-elle dans un immeuble ?
fn inside_building(pose: &StuntPose, world: &TestWorld, z: f64) -> bool {
    world
        .roof_at(pose.x, z)
        .is_some_and(|roof| pose.altitude < f64::from(roof) - EPS)
}

/// Ce qu'un déroulé de 30 s a produit.
#[derive(Debug, Default, PartialEq)]
struct Trace {
    poses: Vec<StuntPose>,
    runs: usize,
    max_live: usize,
}

/// Joue `script` seul pendant `seconds` : les cascades s'enchaînent, chacune finie ou coupée proprement.
fn play(script: &dyn StuntScript, seed: u64, city: bool, seconds: f64) -> Trace {
    let mut world = TestWorld::new(city);
    let mut rng = Rand::new(seed);
    let mut trace = Trace::default();
    let mut run: Option<(Box<dyn StuntRun>, f64)> = None;
    let mut pose = StuntPose::default();
    let mut time = 100.0;
    let end = time + seconds;
    while time < end {
        time += DT;
        let env = env_at(time, None);
        if run.is_none() {
            if !script.available(&env, &world) {
                continue;
            }
            let mut ctx = StuntCtx {
                env: &env,
                world: &mut world,
                rng: &mut rng,
            };
            run = Some((script.begin(&mut ctx), 0.0));
            trace.runs += 1;
            pose = StuntPose {
                x: env.car_x,
                focus: DVec3::new(env.car_x, 0.0, env.car_z),
                ..StuntPose::default()
            };
        }
        let Some((current, t)) = run.as_mut() else { continue };
        *t += DT;
        let mut ctx = StuntCtx {
            env: &env,
            world: &mut world,
            rng: &mut rng,
        };
        let going = current.step(*t, &mut ctx, &mut pose);
        let why = invalid(&pose);
        assert!(why.is_none(), "{} à t = {t:.2} s : {why:?}", script.kind().as_str());
        assert!(
            !inside_building(&pose, &world, env.car_z),
            "{} dans un immeuble à t = {t:.2} s : {pose:?}",
            script.kind().as_str()
        );
        trace.poses.push(pose);
        trace.max_live = trace.max_live.max(world.live());
        if !going || *t >= current.duration() {
            current.end(&mut world);
            run = None;
            assert_eq!(world.live(), 0, "{} rend tous ses véhicules", script.kind().as_str());
        }
    }
    if let Some((mut current, _)) = run {
        current.end(&mut world);
    }
    assert_eq!(world.live(), 0);
    trace
}

#[test]
fn trente_secondes_sans_pose_invalide() {
    for script in every_script() {
        // Sans ville (route seule) et avec (façades, toits, portique).
        for city in [false, true] {
            for seed in 1..=3 {
                let trace = play(&*script, seed, city, 30.0);
                let name = script.kind().as_str();
                // Sans ville, le saut d'enseigne n'a pas de structure : il ne part pas (`available`).
                if script.kind() == StuntKind::SignHop && !city {
                    assert_eq!(trace.runs, 0, "{name} sans portique");
                    continue;
                }
                assert!(
                    trace.runs >= 1,
                    "{name} se joue au moins une fois en 30 s (ville : {city})"
                );
                assert!(trace.poses.len() > 100, "{name} écrit des poses");
            }
        }
    }
}

#[test]
fn chaque_cascade_est_rejouable_par_graine() {
    for script in every_script() {
        let name = script.kind().as_str();
        let a = play(&*script, 42, true, 30.0);
        let b = play(&*script, 42, true, 30.0);
        assert_eq!(a, b, "{name} : mêmes graine et entrées, même cascade");
        let c = play(&*script, 43, true, 30.0);
        assert_ne!(a.poses, c.poses, "{name} : une autre graine joue une autre cascade");
    }
}

#[test]
fn les_cascades_posent_leurs_vehicules_et_les_rendent() {
    // Celles qui amènent des véhicules en posent, et la flotte est vide à la fin (vérifié dans `play`).
    for kind in [
        StuntKind::Slalom,
        StuntKind::NearMiss,
        StuntKind::HeadOn,
        StuntKind::Inverted,
        StuntKind::Threading,
    ] {
        let script = script_of(kind);
        let trace = play(&*script, 7, true, 30.0);
        assert!(trace.max_live >= 1, "{} pose au moins un véhicule", kind.as_str());
    }
    // Les autres n'en touchent aucun.
    for kind in [
        StuntKind::OffRoad,
        StuntKind::Loop,
        StuntKind::WallRide,
        StuntKind::SignHop,
        StuntKind::Rooftop,
        StuntKind::BigJump,
        StuntKind::Ascension,
    ] {
        let script = script_of(kind);
        assert_eq!(play(&*script, 7, true, 30.0).max_live, 0, "{}", kind.as_str());
    }
}

#[test]
fn les_angles_commencent_et_finissent_a_plat() {
    // « Les angles sont nuls (à 2π près) en début et en fin » : le directeur fond la pose sur la conduite sans saut.
    let tau = std::f64::consts::TAU;
    for script in every_script() {
        let name = script.kind().as_str();
        let mut world = TestWorld::new(true);
        let mut rng = Rand::new(5);
        let env = env_at(100.0, None);
        let mut ctx = StuntCtx {
            env: &env,
            world: &mut world,
            rng: &mut rng,
        };
        let mut run = script.begin(&mut ctx);
        let duration = run.duration();
        let mut pose = StuntPose::default();
        let mut time = 100.0;
        let mut t = 0.0;
        let mut first = None;
        while t < duration {
            t += DT;
            time += DT;
            let env = env_at(time, None);
            let mut ctx = StuntCtx {
                env: &env,
                world: &mut world,
                rng: &mut rng,
            };
            run.step(t, &mut ctx, &mut pose);
            first.get_or_insert(pose);
        }
        let flat = |a: f64| a.rem_euclid(tau).min(tau - a.rem_euclid(tau));
        if script.kind() != StuntKind::SignHop {
            // Le saut d'enseigne se cale sur la structure, pas sur le temps : il finit quand la voiture l'a dépassée.
            assert!(flat(pose.pitch) < 0.5, "{name} finit à plat (tangage) : {}", pose.pitch);
            assert!(flat(pose.roll) < 0.6, "{name} finit à plat (roulis) : {}", pose.roll);
        }
        let first = first.unwrap_or_default();
        assert!(
            flat(first.pitch) < 0.5 && flat(first.roll) < 0.5,
            "{name} démarre à plat"
        );
        run.end(&mut world);
    }
}

// --- Derrière le vrai directeur -------------------------------------------------------------------------------------

/// Les portes d'un niveau 3 : tout le catalogue, tous les rangs, Ascension ouverte, `max_floor` au choix.
fn gates(max_floor: u8) -> ShowGates {
    let mut g = cf_show::gates::open_gates();
    g.max_floor = max_floor;
    g.stunt_tiers = ["common", "uncommon", "rare"]
        .into_iter()
        .map(|s| ShowStuntTier::new(s).unwrap())
        .collect();
    g.stunt_kinds = StuntKind::ALL
        .into_iter()
        .map(|k| ShowStuntKind::new(k.as_str()).unwrap())
        .collect();
    g.ascension = true;
    g.stunt_gap_scale = 1.0;
    g
}

/// Un lien de spectacle de test : ordre d'Ascension et ralentis du monde observables.
#[derive(Default)]
struct TestLink {
    time_scales: Vec<(f32, f32)>,
}

impl StuntLink for TestLink {
    fn take_ascension(&mut self) -> bool {
        false
    }
    fn freshness_weight(&self, _id: &str, _now: f64) -> f64 {
        1.0
    }
    fn freshness_mark(&mut self, _id: &str, _now: f64) {}
    fn request_time_scale(&mut self, scale: f32, seconds: f32) {
        self.time_scales.push((scale, seconds));
    }
}

/// Le directeur, un monde et un registre, à 60 images par seconde et un kick toutes les 0,5 s.
struct Sim {
    director: StuntDirector,
    registry: StuntRegistry,
    world: TestWorld,
    now: f64,
    count: u32,
    link: TestLink,
    /// Les cascades vues et le nombre d'images où la pose a pesé.
    seen: [bool; STUNT_COUNT],
}

impl Sim {
    fn new(seed: u64, scripts: impl IntoIterator<Item = Box<dyn StuntScript>>) -> Self {
        let mut registry = StuntRegistry::default();
        for script in scripts {
            registry.register(script);
        }
        Self {
            director: StuntDirector::new(seed),
            registry,
            world: TestWorld::new(true),
            now: 100.0,
            count: 0,
            link: TestLink::default(),
            seen: [false; STUNT_COUNT],
        }
    }

    /// Une image. `drop_now` : le drop frappe, avec la cascade préparée `planned`.
    fn frame(&mut self, gates: &ShowGates, planned: Planned, drop_now: bool) -> f64 {
        self.now += DT;
        let phase = self.now.rem_euclid(0.5);
        let kick = phase < DT;
        if kick {
            self.count += 1;
        }
        let frame = StuntFrame {
            dt: DT,
            now: self.now,
            enabled: true,
            car_x: 0.0,
            car_z: -self.now * 60.0,
            altitude: 0.0,
            road_speed: 60.0,
            beat: TrafficBeat {
                kick,
                count: self.count,
                bpm: Some(120.0),
                pulse: (1.0 - phase * 2.0) as f32,
                ..TrafficBeat::SILENT
            },
            drop: false,
            intensity: 0.6,
            show: Some(StuntShow {
                gates,
                breathing: false,
                drop_now,
                planned,
            }),
        };
        let x = self
            .director
            .update(&frame, &self.registry, &mut self.world, &mut self.link);
        if let Some(view) = self.director.view() {
            self.seen[view.kind.index()] = true;
        }
        x
    }

    /// Les images d'une durée. Pour les cascades alignées « drop » : un drop toutes les 30 s avec `planned`.
    fn run(&mut self, gates: &ShowGates, seconds: f64, planned: Planned) {
        let frames = (seconds / DT) as usize;
        for i in 0..frames {
            let drop_now = planned != Planned::None && i % 1800 == 1799;
            let x = self.frame(gates, planned, drop_now);
            assert!(x.is_finite());
            let w = self.director.weight();
            assert!((0.0..=1.0).contains(&w), "poids de la pose : {w}");
            let pose = self.director.pose();
            if self.director.active() {
                assert!(
                    pose.altitude.is_finite() && pose.x.is_finite() && pose.pitch.is_finite() && pose.roll.is_finite()
                );
            }
        }
    }
}

/// La cascade `kind`, préparée au drop quand elle est alignée « drop ».
fn plan_for(script: &dyn StuntScript) -> Planned {
    if script.align() == crate::stunts::StuntAlign::Drop {
        Planned::Kind(script.kind())
    } else {
        Planned::None
    }
}

#[test]
fn la_voiture_ne_vole_jamais_au_niveau_zero() {
    for script in every_script() {
        let name = script.kind().as_str();
        let plan = plan_for(&*script);
        // `max_floor` à 0 : même avec tous les rangs ouverts, aucune cascade, aucun véhicule, aucun poids.
        let grounded = gates(0);
        let mut sim = Sim::new(9, [script_of(script.kind())]);
        sim.run(&grounded, 600.0, plan);
        assert!(!sim.seen[script.kind().index()], "{name} ne part pas au niveau 0");
        assert!(sim.director.weight().abs() < f64::EPSILON && sim.director.view().is_none());
        assert_eq!(sim.world.live(), 0);
        assert!(!sim.world.held);

        // Et dès que le niveau le permet, elle part (le test n'est pas vacueux).
        let open = gates(4);
        let mut sim = Sim::new(9, [script_of(script.kind())]);
        sim.run(&open, 600.0, plan);
        assert!(sim.seen[script.kind().index()], "{name} part quand max_floor > 0");
    }
}

#[test]
fn le_retour_au_niveau_zero_coupe_la_cascade_en_cours() {
    for script in every_script() {
        let name = script.kind().as_str();
        let plan = plan_for(&*script);
        let open = gates(4);
        let mut sim = Sim::new(11, [script_of(script.kind())]);
        let mut guard = 0.0;
        while sim.director.view().is_none() {
            let drop_now = plan != Planned::None && (guard / DT) as usize % 600 == 599;
            sim.frame(&open, plan, drop_now);
            guard += DT;
            assert!(guard < 600.0, "{name} finit par partir");
        }
        // Le niveau se ferme : coupée en moins d'une seconde, véhicules rendus, ville rendue au trafic.
        let grounded = gates(0);
        for _ in 0..(1.0 / DT) as usize {
            sim.frame(&grounded, plan, false);
        }
        assert!(sim.director.view().is_none(), "{name} coupée en moins d'une seconde");
        assert_eq!(sim.world.live(), 0, "{name} rend ses véhicules");
        assert!(!sim.world.held);
    }
}

#[test]
fn le_repertoire_complet_tient_une_heure_sans_pose_invalide() {
    let open = gates(4);
    let mut sim = Sim::new(21, every_script());
    // Un drop toutes les 30 s, avec la cascade préparée qui tourne : Grand saut, Ascension et les autres.
    let kinds = StuntKind::ALL;
    let frames = (3600.0 / DT) as usize;
    for i in 0..frames {
        let drop_now = i % 1800 == 1799;
        let planned = if drop_now {
            Planned::Kind(kinds[(i / 1800) % kinds.len()])
        } else {
            Planned::None
        };
        // Un plan sans drop bloquerait le tirage normal : on ne l'annonce qu'au moment du drop.
        let x = sim.frame(&open, planned, drop_now);
        assert!(x.is_finite());
        assert!(sim.director.pose().altitude.is_finite());
    }
    let seen = sim.seen.iter().filter(|s| **s).count();
    assert!(seen >= 8, "le tirage varie : {seen} cascades sur {STUNT_COUNT}");
    assert!(
        sim.seen[StuntKind::Ascension.index()],
        "l'Ascension part sur son drop préparé"
    );
    assert!(
        sim.link
            .time_scales
            .iter()
            .all(|(s, secs)| (*s - 0.3).abs() < 1e-6 && *secs > 0.0),
        "l'Ascension ralentit le monde à 0,3 : {:?}",
        sim.link.time_scales
    );
    assert!(
        !sim.link.time_scales.is_empty(),
        "le ralenti est demandé au chef du spectacle"
    );
}

#[test]
fn hors_spectacle_l_ascension_ne_ralentit_rien() {
    let mut world = TestWorld::new(true);
    let mut rng = Rand::new(3);
    let mut pose = StuntPose::default();
    let env = env_at(100.0, None);
    let mut ctx = StuntCtx {
        env: &env,
        world: &mut world,
        rng: &mut rng,
    };
    let mut run = Ascension.begin(&mut ctx);
    let mut t = 0.0;
    while t < run.duration() {
        t += DT;
        let env = env_at(100.0 + t, None);
        let mut ctx = StuntCtx {
            env: &env,
            world: &mut world,
            rng: &mut rng,
        };
        run.step(t, &mut ctx, &mut pose);
        assert!(pose.time_scale.is_none());
    }
}

#[test]
fn l_ascension_demande_son_ralenti_une_seule_fois_et_atterrit_sur_une_mesure() {
    let g = gates(4);
    let show = |g| StuntShow {
        gates: g,
        breathing: false,
        drop_now: false,
        planned: Planned::None,
    };
    let mut world = TestWorld::new(true);
    let mut rng = Rand::new(3);
    let mut pose = StuntPose::default();
    let env = env_at(100.0, Some(show(&g)));
    let mut ctx = StuntCtx {
        env: &env,
        world: &mut world,
        rng: &mut rng,
    };
    let mut run = Ascension.begin(&mut ctx);
    let mut requests = 0;
    let mut apex = 0.0_f64;
    let mut t = 0.0;
    while t < run.duration() {
        t += DT;
        let env = env_at(100.0 + t, Some(show(&g)));
        let mut ctx = StuntCtx {
            env: &env,
            world: &mut world,
            rng: &mut rng,
        };
        run.step(t, &mut ctx, &mut pose);
        if let Some((scale, seconds)) = pose.time_scale.take() {
            requests += 1;
            assert!(
                (scale - 0.3).abs() < 1e-6 && (seconds - 2.0).abs() < 1e-6,
                "{scale} {seconds}"
            );
        }
        apex = apex.max(pose.altitude);
    }
    assert_eq!(requests, 1);
    assert!((150.0..=251.5).contains(&apex), "sommet : {apex}");
    // Durée visée : 32 temps moins le temps que le ralenti fait perdre ; la fin retombe sur une mesure (4 temps).
    let expected = 32.0 * 0.5 - (1.0 - 0.3) * (4.0 * 0.5 + 0.72);
    assert!((run.duration() - expected).abs() < 1e-9);
}
