//! Tests du lot C (J6-05) : plans qui filment un sujet du monde (panneau, moto, poursuite, structure, enseigne du DJ,
//! astre) ou le ciel (lune, géante à anneau).
//!
//! Le monde réel (J7) n'existe pas encore : les sujets sont factices, injectés dans le [`CinemaFrame`] à chaque image
//! par [`Night`], qui rejoue aussi la chute d'un astre (écrasement contre la ville de boîtes de [`crate::testkit`]) et
//! répond à l'ordre `LaunchMeteor`. Chaque plan est joué 30 s simulées avec les mêmes contrôles de pose que les lots A
//! et B ; les plans « tenus » (`holding`) sont vérifiés jusqu'au bout, drops forcés compris.

use bevy::math::{DVec3, Vec3};
use cf_render::{CameraPose, Subject, SubjectKind};

use super::{
    AdGlance, Bikers, DjSign, LandmarkReveal, MeteorImpactWatch, MeteorRide, MoonPass, Pursuit, Ringworld, catalog,
};
use crate::choreo::{MAX_CUTS_PER_SECOND, WINDOW_SECONDS, is_jump};
use crate::director::CinemaDirector;
use crate::editor::is_strobing;
use crate::pose::is_finite;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::shots::Chase;
use crate::testkit::{BoxCity, DT, Sim, check_pose};
use crate::types::{MeteorBlast, RingPlanet, Section, TransitionKind};
use crate::world::{CinemaFrame, CinemaWorld, NoObstacles, Obstacles};

const FRAMES: usize = (30.0 / DT as f64) as usize;

/// Les neuf plans du lot C.
const LOT_C: [&str; 9] = [
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

fn spec_of(id: &str) -> Box<dyn ShotSpec> {
    match id {
        "ad-glance" => Box::new(AdGlance),
        "pursuit" => Box::new(Pursuit),
        "bikers" => Box::new(Bikers),
        "landmark-reveal" => Box::new(LandmarkReveal),
        "meteor-ride" => Box::new(MeteorRide),
        "ringworld" => Box::new(Ringworld),
        "moon-pass" => Box::new(MoonPass),
        "meteor-impact-watch" => Box::new(MeteorImpactWatch),
        _ => Box::new(DjSign),
    }
}

/// Ce que la soirée factice met dans le monde.
// Un drapeau par sujet de la soirée : un enum de combinaisons serait plus lourd à lire.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, Default)]
struct Cast {
    sign: bool,
    bike: bool,
    police: bool,
    landmark: bool,
    dj_sign: bool,
    /// Un astre tombe au début de chaque cycle de 20 s.
    meteors: bool,
    ring: bool,
}

impl Cast {
    const EVERYTHING: Self = Self {
        sign: true,
        bike: true,
        police: true,
        landmark: true,
        dj_sign: true,
        meteors: true,
        ring: true,
    };

    fn for_shot(id: &str) -> Self {
        let none = Self::default();
        match id {
            "ad-glance" => Self { sign: true, ..none },
            "pursuit" => Self { police: true, ..none },
            "bikers" => Self { bike: true, ..none },
            "landmark-reveal" => Self { landmark: true, ..none },
            "dj-sign" => Self { dj_sign: true, ..none },
            "ringworld" => Self { ring: true, ..none },
            "meteor-impact-watch" => Self { meteors: true, ..none },
            _ => none,
        }
    }
}

/// Un astre en chute libre.
#[derive(Debug, Clone, Copy)]
struct Meteor {
    id: u64,
    position: DVec3,
    velocity: Vec3,
    size: f32,
}

/// La soirée de test : la soirée de [`Sim`] plus les sujets factices et la chute d'un astre.
struct Night {
    sim: Sim,
    cast: Cast,
    subjects: Vec<Subject>,
    meteor: Option<Meteor>,
    next_id: u64,
    /// Horloges de la scène des explosions.
    blasts: Vec<f64>,
    /// À partir de cet instant (s), un break : section calme, intensité basse.
    break_from: Option<f64>,
    /// Route dégagée, sans immeuble : le dégagement des murs du réalisateur (une téléportation de la pose, qui n'est pas
    /// une coupe de plan) ne vient pas fausser le comptage des coupes.
    open_road: bool,
}

/// La rangée de repère (ligne de grille) la plus proche devant la voiture : position z et identifiant stable.
fn grid_ahead(car_z: f64, grid: f64) -> (f64, u64) {
    let k = (car_z / grid).floor();
    (k * grid, (-k) as u64)
}

impl Night {
    fn new(cast: Cast) -> Self {
        let mut sim = Sim::new(true);
        sim.world.show.drop_planned = true;
        sim.world.show.dj_sign = cast.dj_sign;
        let mut night = Self {
            sim,
            cast,
            subjects: Vec::with_capacity(8),
            meteor: None,
            next_id: 1,
            blasts: Vec::new(),
            break_from: None,
            open_road: false,
        };
        night.advance();
        night
    }

    fn world(&self) -> &CinemaWorld {
        &self.sim.world
    }

    fn frame(&self) -> CinemaFrame<'_> {
        let obstacles: &dyn Obstacles = if self.open_road { &NoObstacles } else { &BoxCity };
        CinemaFrame::new(&self.sim.world, obstacles, &self.subjects)
    }

    /// Le monde répond à `LaunchMeteor` : un astre apparaît au-dessus de la ville, devant la voiture.
    fn launch(&mut self) {
        if self.meteor.is_none() {
            let car = self.sim.world.car;
            self.spawn(car + DVec3::new(-30.0, 520.0, -150.0), Vec3::new(8.0, -45.0, -25.0));
        }
    }

    fn spawn(&mut self, position: DVec3, velocity: Vec3) {
        self.meteor = Some(Meteor {
            id: self.next_id,
            position,
            velocity,
            size: 4.0,
        });
        self.next_id += 1;
    }

    fn advance(&mut self) {
        self.sim.step();
        let t = self.sim.world.time;
        let car = self.sim.world.car;
        let w = &mut self.sim.world;
        if self.break_from.is_some_and(|from| t >= from) {
            w.section = Section::Calm;
            w.intensity = 0.1;
        }
        w.ring_planet = self.cast.ring.then_some(RingPlanet {
            center: Vec3::new(2000.0, 800.0, -3000.0),
            radius: 900.0,
            ring_inner: 1200.0,
            ring_outer: 1900.0,
            ring_normal: Vec3::new(0.3, 1.0, 0.2).normalize(),
        });
        self.step_meteor(t);
        if self.cast.meteors && t % 20.0 < f64::from(DT) && self.meteor.is_none() {
            // Il tombe sur le côté droit de la chaussée, là où se dressent les immeubles.
            self.spawn(car + DVec3::new(15.0, 350.0, -300.0), Vec3::new(0.0, -40.0, 0.0));
        }
        self.fill_subjects(t);
    }

    fn step_meteor(&mut self, t: f64) {
        let Some(m) = self.meteor.as_mut() else { return };
        m.position += m.velocity.as_dvec3() * f64::from(DT);
        let hit = BoxCity.collides(m.position, f64::from(m.size) * 0.8) || m.position.y <= 0.0;
        if hit {
            self.sim.world.last_blast = Some(MeteorBlast {
                at: m.position,
                radius: 20.0,
                time: t,
                meteor_id: m.id,
            });
            self.blasts.push(t);
            self.meteor = None;
        }
    }

    fn fill_subjects(&mut self, t: f64) {
        self.subjects.clear();
        let car = self.sim.world.car;
        // Va-et-vient continu (onde triangulaire de 60 s) : un sujet qui se téléporte ferait sauter la caméra qui le suit.
        let cycle = ((t % 60.0) - 30.0).abs();
        let subject = |id, kind, position, velocity, size, facing| Subject {
            id,
            kind,
            position,
            velocity,
            size,
            facing,
        };
        if self.cast.sign {
            let (z, id) = grid_ahead(car.z, 400.0);
            let at = DVec3::new(-5.0, 6.0, z - 1.0);
            self.subjects
                .push(subject(id, SubjectKind::Sign, at, Vec3::ZERO, 20.0, Some(Vec3::Z)));
        }
        if self.cast.bike {
            let at = car + DVec3::new(3.0, 0.5, -100.0 - 3.0 * (30.0 - cycle));
            let v = Vec3::new(0.0, 0.0, -45.0);
            self.subjects.push(subject(1, SubjectKind::Bike, at, v, 2.0, None));
        }
        if self.cast.police {
            let at = car + DVec3::new(2.0, 0.5, -60.0 - 3.0 * (30.0 - cycle));
            let v = Vec3::new(0.0, 0.0, -48.0);
            self.subjects.push(subject(1, SubjectKind::Police, at, v, 4.0, None));
        }
        if self.cast.landmark {
            let (z, id) = grid_ahead(car.z, 500.0);
            let at = DVec3::new(-40.0, 0.0, z - 1.0);
            self.subjects
                .push(subject(id, SubjectKind::Landmark, at, Vec3::ZERO, 80.0, None));
        }
        if self.cast.dj_sign {
            let (z, id) = grid_ahead(car.z, 300.0);
            let at = DVec3::new(0.0, 30.0, z - 1.0);
            self.subjects
                .push(subject(id, SubjectKind::DjSign, at, Vec3::ZERO, 60.0, Some(Vec3::Z)));
        }
        if let Some(m) = self.meteor {
            self.subjects
                .push(subject(m.id, SubjectKind::Meteor, m.position, m.velocity, m.size, None));
        }
    }
}

/// Force un drop dans l'image (toutes les 7 s) : il couperait n'importe quel plan qui ne tient pas.
fn force_drops(night: &mut Night) {
    let t = night.world().time;
    if t % 7.0 < f64::from(DT) * 1.5 {
        night.sim.world.drop = true;
        night.sim.world.show.drop_now = true;
    }
}

#[test]
fn le_catalogue_contient_les_neuf_plans_du_lot_c() {
    let ids: Vec<&str> = catalog().iter().map(|s| s.id()).collect();
    for id in LOT_C {
        assert_eq!(
            ids.iter().filter(|i| **i == id).count(),
            1,
            "{id} : absent ou en double"
        );
        assert!(!is_strobing(id), "{id} ne doit pas enchaîner de coupes à chaque temps");
    }
    assert!(!ids.contains(&"strobe"), "le strobe reste hors périmètre");
}

#[test]
fn les_fiches_reprennent_celles_de_sillage() {
    // (id, rang, énergie, min, max, scripté, cooldown, poids, entrée)
    let swoop = TransitionKind::Swoop;
    let sillage = [
        ("ad-glance", "uncommon", (0.0, 0.75), 3.0, 5.5, false, 45.0, 1.0, swoop),
        ("pursuit", "uncommon", (0.25, 1.0), 6.0, 16.0, false, 50.0, 1.2, swoop),
        ("bikers", "uncommon", (0.3, 1.0), 5.0, 11.0, false, 50.0, 1.2, swoop),
        (
            "landmark-reveal",
            "uncommon",
            (0.0, 0.85),
            5.0,
            10.0,
            false,
            60.0,
            1.1,
            swoop,
        ),
        ("dj-sign", "uncommon", (0.0, 1.0), 6.0, 11.0, true, 40.0, 0.7, swoop),
        ("meteor-ride", "rare", (0.15, 1.0), 6.0, 22.0, true, 150.0, 1.0, swoop),
        (
            "meteor-impact-watch",
            "uncommon",
            (0.0, 1.0),
            4.0,
            14.0,
            true,
            70.0,
            1.0,
            TransitionKind::Cut,
        ),
        ("moon-pass", "rare", (0.0, 0.8), 15.0, 19.0, true, 240.0, 1.0, swoop),
        ("ringworld", "epic", (0.0, 0.6), 32.5, 38.5, true, 420.0, 1.0, swoop),
    ];
    for (id, tier, energy, min, max, scripted, cooldown, weight, entry) in sillage {
        let spec = spec_of(id);
        assert_eq!(spec.id(), id);
        assert_eq!(spec.tier().as_str(), tier, "{id}");
        assert_eq!(spec.energy(), energy, "{id}");
        assert!((spec.min_seconds() - min).abs() < 1e-6, "{id}");
        assert!((spec.max_seconds() - max).abs() < 1e-6, "{id}");
        assert_eq!(spec.scripted(), scripted, "{id}");
        assert!(!spec.reactive() && !spec.follows_car(), "{id}");
        assert!((spec.cooldown() - cooldown).abs() < 1e-6, "{id}");
        assert!((spec.weight() - weight).abs() < 1e-6, "{id}");
        assert_eq!(spec.entry(), entry, "{id}");
    }
}

#[test]
fn les_plans_a_sujet_ne_sont_pas_disponibles_sans_leur_sujet() {
    let night = Night::new(Cast::default());
    let frame = night.frame();
    for id in [
        "ad-glance",
        "pursuit",
        "bikers",
        "landmark-reveal",
        "dj-sign",
        "meteor-impact-watch",
        "ringworld",
    ] {
        assert!(!spec_of(id).available(&frame), "{id} : disponible sans sujet");
    }
    // Les deux voyages du ciel et la chevauchée (qui lance son astre) ne dépendent d'aucun sujet : ils jouent dès que la
    // voiture est assez bas, comme dans Sillage.
    for id in ["meteor-ride", "moon-pass"] {
        assert!(spec_of(id).available(&frame), "{id}");
    }
}

#[test]
fn chaque_plan_est_disponible_avec_ses_sujets_et_pas_sans_condition() {
    for id in [
        "ad-glance",
        "pursuit",
        "bikers",
        "landmark-reveal",
        "dj-sign",
        "ringworld",
    ] {
        let spec = spec_of(id);
        let mut night = Night::new(Cast::for_shot(id));
        let mut seen = false;
        for _ in 0..FRAMES {
            night.advance();
            seen |= spec.available(&night.frame());
        }
        assert!(seen, "{id} : jamais disponible avec ses sujets sur 30 s");
    }
    // Un astre qui s'écrase dans la ville : le plan d'impact le voit venir.
    let spec = MeteorImpactWatch;
    let mut night = Night::new(Cast::for_shot("meteor-impact-watch"));
    let mut seen = false;
    let mut hot = false;
    for _ in 0..FRAMES {
        night.advance();
        let frame = night.frame();
        seen |= spec.available(&frame);
        hot |= spec.opportunity(&frame) > 0.7;
    }
    assert!(seen, "l'impact n'a jamais été disponible");
    assert!(hot, "l'impact imminent n'a jamais été une occasion à saisir");
}

#[test]
fn l_enseigne_du_dj_exige_le_drapeau_du_spectacle() {
    let mut night = Night::new(Cast::for_shot("dj-sign"));
    for _ in 0..FRAMES {
        night.advance();
        if DjSign.available(&night.frame()) {
            break;
        }
    }
    assert!(DjSign.available(&night.frame()));
    night.sim.world.show.dj_sign = false;
    assert!(!DjSign.available(&night.frame()), "enseigne éteinte");
}

#[test]
fn un_panneau_vu_par_derriere_ou_une_voiture_en_vol_n_est_pas_lu() {
    let mut night = Night::new(Cast::for_shot("ad-glance"));
    for _ in 0..FRAMES {
        night.advance();
        if AdGlance.available(&night.frame()) {
            break;
        }
    }
    assert!(AdGlance.available(&night.frame()));
    night.sim.world.altitude = 60.0;
    assert!(!AdGlance.available(&night.frame()), "la voiture vole trop haut");
    night.sim.world.altitude = 0.0;
    for subject in &mut night.subjects {
        subject.facing = Some(-Vec3::Z);
    }
    assert!(!AdGlance.available(&night.frame()), "face tournée vers l'avant");
}

#[test]
fn les_occasions_croissent_avec_la_proximite() {
    for (id, kind, near, far) in [
        ("bikers", SubjectKind::Bike, 30.0, 150.0),
        ("pursuit", SubjectKind::Police, 30.0, 150.0),
    ] {
        let spec = spec_of(id);
        let mut night = Night::new(Cast::for_shot(id));
        night.advance();
        let mut at = |distance: f64| {
            for s in &mut night.subjects {
                if s.kind == kind {
                    s.position = night.sim.world.car + DVec3::new(2.0, 0.5, -distance);
                }
            }
            spec.opportunity(&night.frame())
        };
        let (close, distant) = (at(near), at(far));
        assert!(close > 0.7, "{id} : {close}");
        assert!(distant == 0.0, "{id} : {distant}");
    }
}

#[test]
fn la_poursuite_prefere_la_police_et_lache_un_fuyard_trop_loin() {
    let mut night = Night::new(Cast::for_shot("pursuit"));
    night.advance();
    let car = night.sim.world.car;
    let quarry = Subject {
        id: 9,
        kind: SubjectKind::Quarry,
        position: car + DVec3::new(0.0, 0.5, -30.0),
        velocity: Vec3::new(0.0, 0.0, -50.0),
        size: 4.0,
        facing: None,
    };
    night.subjects.push(quarry);
    let mut rng = CinemaRng::new(4);
    let mut shot = Pursuit.create(&night.frame(), &mut rng);
    let mut pose = CameraPose::default();
    assert!(shot.update(&night.frame(), &mut pose));
    // Un fuyard plus près que la police est ignoré tant que la police est là : la police est à 60 m, pas à 30 m.
    night.subjects.retain(|s| s.kind == SubjectKind::Police);
    assert!(
        shot.update(&night.frame(), &mut pose),
        "la police suivie est toujours là"
    );
    for s in &mut night.subjects {
        s.position = car + DVec3::new(2.0, 0.5, -300.0);
    }
    assert!(!shot.update(&night.frame(), &mut pose), "la police s'est trop éloignée");
}

#[test]
fn le_plan_s_arrete_quand_son_sujet_disparait() {
    for id in ["ad-glance", "pursuit", "bikers", "landmark-reveal", "dj-sign"] {
        let spec = spec_of(id);
        let mut night = Night::new(Cast::for_shot(id));
        let mut shot = None;
        for _ in 0..FRAMES {
            night.advance();
            if spec.available(&night.frame()) {
                shot = Some(spec.create(&night.frame(), &mut CinemaRng::new(2)));
                break;
            }
        }
        assert!(shot.is_some(), "{id} : jamais disponible");
        let Some(mut shot) = shot else { continue };
        let mut pose = CameraPose::default();
        assert!(shot.update(&night.frame(), &mut pose), "{id}");
        night.subjects.clear();
        assert!(!shot.update(&night.frame(), &mut pose), "{id} : sujet disparu");
    }
}

#[test]
fn chaque_plan_du_lot_c_brut_reste_fini_pendant_30_s() {
    for id in LOT_C {
        let spec = spec_of(id);
        let mut night = Night::new(Cast::for_shot(id));
        let mut rng = CinemaRng::new(11);
        let mut shot: Option<Box<dyn Shot>> = None;
        let mut played = 0usize;
        for _ in 0..FRAMES {
            night.advance();
            let mut launch = false;
            {
                let frame = night.frame();
                if shot.is_none() && spec.available(&frame) {
                    let mut created = spec.create(&frame, &mut rng);
                    launch = created.take_meteor_launch();
                    shot = Some(created);
                }
                if let Some(current) = shot.as_mut() {
                    let mut pose = CameraPose::default();
                    let alive = current.update(&frame, &mut pose);
                    played += 1;
                    assert!(is_finite(&pose), "{id} : pose non finie à t={}", frame.time);
                    if !alive {
                        shot = None;
                    }
                }
            }
            if launch {
                night.launch();
            }
        }
        assert!(
            played >= 120,
            "{id} : joué {played} images sur {FRAMES}, trop peu pour valider"
        );
    }
}

/// Ce qu'une soirée avec réalisateur a montré d'un plan.
#[derive(Debug, Default)]
struct Report {
    /// Premier instant où le plan est à l'antenne (la transition d'entrée comprise).
    air_from: Option<f64>,
    /// Premier instant où le réalisateur a décidé un autre plan.
    replaced_at: Option<f64>,
    /// Images à l'antenne.
    on_air: usize,
    /// Horloges des explosions de la soirée.
    blasts: Vec<f64>,
}

/// Fait jouer le plan `id` par le réalisateur, au milieu de la poursuite de la voiture (qui lance le spectacle : un plan
/// rare ou épique ne peut pas ouvrir la soirée). Chaque image est contrôlée (finie, hors des murs, dans les bornes).
/// S'arrête quand le plan a été remplacé, ou après `seconds`. `drops` : un drop forcé toutes les 7 s, qui couperait
/// n'importe quel plan qui ne tient pas.
fn play(id: &str, seconds: f64, drops: bool) -> Report {
    let mut director = CinemaDirector::new(vec![Box::new(Chase), spec_of(id)], false, 5);
    let mut night = Night::new(Cast::for_shot(id));
    // Les plans épiques ne passent pas avant 90 s ; la planète à anneau a sa meilleure occasion pendant un break.
    night.break_from = (id == "ringworld").then_some(95.0);
    let start = CameraPose {
        position: night.world().car + DVec3::new(0.0, 3.0, 8.0),
        ..CameraPose::default()
    };
    director.start(&start, &night.frame());
    let mut report = Report::default();
    let mut baseline = 0;
    while night.world().time < seconds {
        night.advance();
        if drops {
            force_drops(&mut night);
        }
        let pose = *director.update(&night.frame());
        check_pose(&pose, &night.frame());
        if director.take_meteor_launch() {
            night.launch();
        }
        let now = night.world().time;
        if director.shot_id() == Some(id) {
            report.on_air += 1;
            if report.air_from.is_none() {
                report.air_from = Some(now);
                baseline = director.stats().shots;
            }
        }
        if report.air_from.is_some() && report.replaced_at.is_none() && director.stats().shots != baseline {
            report.replaced_at = Some(now);
        }
        if report.replaced_at.is_some_and(|t| now > t + 1.0) {
            break;
        }
    }
    report.blasts = night.blasts;
    report
}

#[test]
fn chaque_plan_du_lot_c_par_le_realisateur_reste_dans_les_bornes() {
    for id in LOT_C {
        let report = play(id, 500.0, false);
        assert!(
            report.on_air >= 60,
            "{id} : à l'antenne {} images sur 500 s",
            report.on_air
        );
    }
}

#[test]
fn la_geante_a_anneau_n_est_jamais_coupee_pendant_son_tour() {
    let report = play("ringworld", 500.0, true);
    let started = report.air_from.expect("la séquence n'a jamais été à l'antenne");
    // 34,5 s de séquence, étirées de 0,92 à 1,04 : pas de coupe avant 31,7 s malgré les drops forcés toutes les 7 s.
    let held = report.replaced_at.map_or(f64::INFINITY, |t| t - started);
    assert!(held >= 31.4, "coupée après {held} s");
}

#[test]
fn la_lune_n_est_jamais_coupee_pendant_son_passage() {
    let report = play("moon-pass", 500.0, true);
    let started = report.air_from.expect("le passage n'a jamais été à l'antenne");
    // 16 s de séquence, étirées de 0,95 à 1,1.
    let held = report.replaced_at.map_or(f64::INFINITY, |t| t - started);
    assert!(held >= 15.1, "coupé après {held} s");
}

#[test]
fn la_chevauchee_d_un_astre_tient_jusqu_a_l_explosion() {
    let report = play("meteor-ride", 500.0, true);
    let started = report.air_from.expect("la chevauchée n'a jamais été à l'antenne");
    let blast = report
        .blasts
        .iter()
        .copied()
        .find(|&b| b >= started)
        .expect("l'astre lancé s'est écrasé");
    let left = report.replaced_at.expect("le plan finit seul après l'explosion");
    assert!(
        left >= blast,
        "coupée à {left} s, avant l'explosion de {blast} s malgré les drops"
    );
    // Une fois l'explosion passée, le plan rend la main après un court maintien.
    assert!(
        left <= blast + 3.0,
        "gardée trop longtemps : {left} s pour une explosion à {blast} s"
    );
}

#[test]
fn l_impact_tient_jusqu_a_l_explosion() {
    let report = play("meteor-impact-watch", 500.0, true);
    let started = report.air_from.expect("le plan d'impact n'a jamais été à l'antenne");
    let blast = report
        .blasts
        .iter()
        .copied()
        .find(|&b| b >= started)
        .expect("l'astre s'est écrasé");
    let left = report.replaced_at.expect("le plan finit seul");
    assert!(
        left >= blast,
        "coupé à {left} s, avant l'explosion de {blast} s malgré les drops"
    );
}

#[test]
fn la_chevauchee_demande_un_astre_une_seule_fois_et_seulement_s_il_n_y_en_a_pas() {
    let mut night = Night::new(Cast::default());
    let mut rng = CinemaRng::new(3);
    let mut shot = MeteorRide.create(&night.frame(), &mut rng);
    assert!(shot.take_meteor_launch(), "aucun astre en vol : il faut en lancer un");
    assert!(!shot.take_meteor_launch(), "une seule demande");
    night.launch();
    // Un astre en vol (assez de chemin devant lui) : pas de lancement.
    night.advance();
    let mut other = MeteorRide.create(&night.frame(), &mut rng);
    assert!(!other.take_meteor_launch(), "un astre est déjà en vol");
}

#[test]
fn la_chevauchee_adopte_l_astre_lance_et_s_arrete_si_le_monde_n_en_fait_pas() {
    // Le monde répond : l'astre paraît quelques images plus tard, la caméra le suit.
    let mut night = Night::new(Cast::default());
    let mut shot = MeteorRide.create(&night.frame(), &mut CinemaRng::new(1));
    assert!(shot.take_meteor_launch());
    let mut pose = CameraPose::default();
    for i in 0..30 {
        night.advance();
        if i == 6 {
            night.launch();
        }
        assert!(shot.update(&night.frame(), &mut pose), "image {i}");
        assert!(is_finite(&pose));
    }
    assert!(shot.holding(), "l'astre est en vol : on ne coupe pas");
    // Le monde ne répond pas (pas encore de J7) : le plan se termine après le délai de grâce.
    let mut night = Night::new(Cast::default());
    let mut shot = MeteorRide.create(&night.frame(), &mut CinemaRng::new(1));
    assert!(shot.take_meteor_launch());
    let mut alive = 0.0;
    while shot.update(&night.frame(), &mut pose) {
        night.advance();
        alive += DT;
        assert!(alive < 3.0, "le plan sans astre ne finit jamais");
    }
    assert!(alive > 1.0, "le délai de grâce est de 1,2 s : fini après {alive} s");
}

#[test]
fn la_geante_a_anneau_exige_la_planete_et_s_arrete_si_elle_disparait() {
    let mut night = Night::new(Cast::for_shot("ringworld"));
    night.advance();
    assert!(Ringworld.available(&night.frame()));
    night.sim.world.altitude = 100.0;
    assert!(!Ringworld.available(&night.frame()), "trop haut");
    night.sim.world.altitude = 0.0;
    let mut shot = Ringworld.create(&night.frame(), &mut CinemaRng::new(1));
    let mut pose = CameraPose::default();
    assert!(shot.update(&night.frame(), &mut pose));
    assert!(shot.holding());
    night.sim.world.ring_planet = None;
    assert!(!shot.update(&night.frame(), &mut pose), "plus de planète");
    // Le break est le bon moment : section calme et intensité basse.
    night.sim.world.ring_planet = Some(RingPlanet {
        center: Vec3::new(2000.0, 800.0, -3000.0),
        radius: 900.0,
        ring_inner: 1200.0,
        ring_outer: 1900.0,
        ring_normal: Vec3::Y,
    });
    night.sim.world.section = Section::Calm;
    night.sim.world.intensity = 0.1;
    assert!(Ringworld.opportunity(&night.frame()) > 0.7);
    night.sim.world.intensity = 0.5;
    assert!(Ringworld.opportunity(&night.frame()).abs() < f32::EPSILON);
}

#[test]
fn la_lune_n_est_pas_disponible_haut_dans_le_ciel() {
    let mut night = Night::new(Cast::default());
    assert!(MoonPass.available(&night.frame()));
    night.sim.world.altitude = 95.0;
    assert!(!MoonPass.available(&night.frame()));
}

#[test]
fn les_plans_du_lot_c_respectent_le_plafond_de_coupes() {
    // Tout le monde à la fois, deux minutes : chaque plan du lot C (et la poursuite de la voiture) se disputent l'antenne.
    let mut specs: Vec<Box<dyn ShotSpec>> = vec![Box::new(Chase)];
    specs.extend(LOT_C.iter().map(|id| spec_of(id)));
    let mut director = CinemaDirector::new(specs, false, 13);
    let mut night = Night::new(Cast::EVERYTHING);
    night.open_road = true;
    let start = CameraPose {
        position: night.world().car + DVec3::new(0.0, 3.0, 8.0),
        ..CameraPose::default()
    };
    director.start(&start, &night.frame());
    let mut cuts = Vec::new();
    let mut previous = start;
    let mut previous_car = night.world().car;
    let mut seen = std::collections::BTreeSet::new();
    while night.world().time < 240.0 {
        night.advance();
        let pose = *director.update(&night.frame());
        if director.take_meteor_launch() {
            night.launch();
        }
        let frame = night.frame();
        check_pose(&pose, &frame);
        // Un panoramique éclair (`Whip`) balaie l'écran par construction : c'est une transition, pas une coupe.
        let whip = director.transition() == Some(TransitionKind::Whip);
        if is_jump(&previous, previous_car, &pose, frame.car, frame.dt) && !whip {
            cuts.push(frame.time);
        }
        if let Some(id) = director.shot_id() {
            seen.insert(id);
        }
        previous = pose;
        previous_car = frame.car;
    }
    assert!(
        seen.iter().filter(|id| LOT_C.contains(id)).count() >= 3,
        "peu de plans du lot C à l'antenne : {seen:?}"
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
