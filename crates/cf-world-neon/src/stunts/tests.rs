//! Les cascades sans Bevy : portes, rangs, liste blanche, drop préparé, Ascension, fondu, et la statistique de l'écart
//! entre cascades selon `stunt_gap_scale`. Les scripts sont des faux (les vrais sont J7-07).

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_show::model::{ShowGates, StuntKind as ShowStuntKind, StuntTier as ShowStuntTier};

use super::kind::{StuntAlign, StuntKind, StuntTier};
use super::math::{bump, lerp};
use super::permit::{NoLink, StuntLink};
use super::script::{Planned, StuntCtx, StuntEnv, StuntPose, StuntRegistry, StuntRun, StuntScript, StuntShow};
use super::{StuntDirector, StuntFrame, StuntWorld};
use crate::pilot::CarAnimation;
use crate::pilot::flight::Obstruction;
use crate::traffic::{Craft, CraftId, Fleet, TrafficBeat};

const DT: f64 = 1.0 / 60.0;

/// Un faux monde : une vraie flotte (le pool de véhicules), une ville vide.
#[derive(Debug, Default)]
struct FakeWorld {
    fleet: Fleet,
    held: bool,
    busy: bool,
    spawned: u32,
}

impl StuntWorld for FakeWorld {
    fn spawn(&mut self, variant: u8, at: DVec3, scale: f32, _tint: i64) -> Option<CraftId> {
        let mut rng = crate::pilot::rand::Rand::new(1);
        self.spawned += 1;
        self.fleet
            .spawn(variant, at, bevy::math::Vec3::ZERO, LinearRgba::WHITE, scale, &mut rng)
    }
    fn release(&mut self, id: CraftId) {
        self.fleet.release(id);
    }
    fn craft_mut(&mut self, id: CraftId) -> Option<&mut Craft> {
        self.fleet.get_mut(id)
    }
    fn roof_at(&self, _x: f64, _z: f64) -> Option<f32> {
        None
    }
    fn facade(&self, _side: i8, _z: f64, _depth: f64) -> Option<f32> {
        None
    }
    fn overhead(&self, _out: &mut Vec<Obstruction>, _car_z: f64) {}
    fn bridge_ahead(&self, _car_z: f64, _distance: f64, _margin: f64) -> bool {
        false
    }
    fn busy(&self) -> bool {
        self.busy
    }
    fn hold(&mut self, on: bool) {
        self.held = on;
    }
}

/// Une cascade factice : un saut de 6 m avec un salto, un véhicule posé.
struct Fake {
    kind: StuntKind,
    tier: StuntTier,
    align: StuntAlign,
    duration: f64,
}

impl Fake {
    fn new(kind: StuntKind, tier: StuntTier) -> Self {
        Self {
            kind,
            tier,
            align: StuntAlign::Bar,
            duration: 6.0,
        }
    }
}

struct FakeRun {
    duration: f64,
    craft: Option<CraftId>,
}

impl StuntRun for FakeRun {
    fn duration(&self) -> f64 {
        self.duration
    }
    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let u = t / self.duration;
        out.x = lerp(ctx.env.car_x, ctx.env.car_x + 4.0, bump(u, 0.0, 0.2, 0.8, 1.0));
        out.altitude = 6.0 * bump(u, 0.1, 0.3, 0.7, 0.9);
        out.pitch = std::f64::consts::TAU * super::math::turn(u, 0.3, 0.7);
        out.hover = out.altitude > 0.5;
        super::math::move_craft(ctx.world, self.craft, 0.0, 3.0, ctx.env.car_z - 30.0, ctx.env.dt);
        t < self.duration
    }
    fn end(&mut self, world: &mut dyn StuntWorld) {
        if let Some(id) = self.craft.take() {
            world.release(id);
        }
    }
}

impl StuntScript for Fake {
    fn kind(&self) -> StuntKind {
        self.kind
    }
    fn tier(&self) -> StuntTier {
        self.tier
    }
    fn weight(&self) -> f64 {
        1.0
    }
    fn align(&self) -> StuntAlign {
        self.align
    }
    fn available(&self, _env: &StuntEnv<'_>, _world: &dyn StuntWorld) -> bool {
        true
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let craft = ctx.world.spawn(0, DVec3::new(0.0, 3.0, ctx.env.car_z - 30.0), 1.0, 0);
        Box::new(FakeRun {
            duration: self.duration,
            craft,
        })
    }
}

fn registry(fakes: impl IntoIterator<Item = Fake>) -> StuntRegistry {
    let mut registry = StuntRegistry::default();
    for fake in fakes {
        registry.register(Box::new(fake));
    }
    registry
}

fn full_registry() -> StuntRegistry {
    registry(StuntKind::ALL.into_iter().map(|kind| {
        let tier = match kind {
            StuntKind::BigJump | StuntKind::Ascension | StuntKind::Loop => StuntTier::Rare,
            StuntKind::Rooftop | StuntKind::Inverted => StuntTier::Uncommon,
            _ => StuntTier::Common,
        };
        Fake::new(kind, tier)
    }))
}

/// Des portes de niveau 3 : tout le catalogue, tous les rangs, l'Ascension fermée.
fn gates() -> ShowGates {
    let mut g = cf_show::gates::open_gates();
    g.max_floor = 4;
    g.stunt_tiers = ["common", "uncommon", "rare"]
        .into_iter()
        .map(|s| ShowStuntTier::new(s).unwrap())
        .collect();
    g.ascension = false;
    g.stunt_gap_scale = 1.0;
    g
}

fn kinds(list: &[StuntKind]) -> Vec<ShowStuntKind> {
    list.iter().map(|k| ShowStuntKind::new(k.as_str()).unwrap()).collect()
}

/// Un lien de spectacle de test : ordre d'Ascension et mémoire de fraîcheur observables.
#[derive(Default)]
struct TestLink {
    ascension_order: bool,
    marked: Vec<String>,
}

impl StuntLink for TestLink {
    fn take_ascension(&mut self) -> bool {
        std::mem::take(&mut self.ascension_order)
    }
    fn freshness_weight(&self, _id: &str, _now: f64) -> f64 {
        1.0
    }
    fn freshness_mark(&mut self, id: &str, _now: f64) {
        self.marked.push(id.to_owned());
    }
}

/// Ce que la simulation enregistre.
#[derive(Debug, Default)]
struct Trace {
    /// (kind, début, fin).
    runs: Vec<(StuntKind, f64, f64)>,
    max_weight: f64,
}

struct Sim<'g> {
    director: StuntDirector,
    registry: StuntRegistry,
    world: FakeWorld,
    gates: &'g ShowGates,
    breathing: bool,
    now: f64,
    count: u32,
    kick_clock: f64,
    trace: Trace,
    open_run: Option<(StuntKind, f64)>,
}

impl<'g> Sim<'g> {
    fn new(seed: u64, registry: StuntRegistry, gates: &'g ShowGates) -> Self {
        Self {
            director: StuntDirector::new(seed),
            registry,
            world: FakeWorld::default(),
            gates,
            breathing: false,
            now: 100.0,
            count: 0,
            kick_clock: 0.0,
            trace: Trace::default(),
            open_run: None,
        }
    }

    /// Une image à 60 i/s, un kick toutes les 0,5 s (120 bpm). Renvoie l'abscisse de la voiture et l'animation fondue.
    fn frame(&mut self, planned: Planned, drop_now: bool, link: &mut dyn StuntLink) -> (f64, CarAnimation) {
        self.now += DT;
        self.kick_clock += DT;
        let kick = self.kick_clock >= 0.5;
        if kick {
            self.kick_clock -= 0.5;
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
                ..TrafficBeat::SILENT
            },
            drop: false,
            intensity: 0.5,
            show: Some(StuntShow {
                gates: self.gates,
                breathing: self.breathing,
                drop_now,
                planned,
            }),
        };
        let x = self.director.update(&frame, &self.registry, &mut self.world, link);
        let mut animation = CarAnimation::default();
        self.director.apply(&mut animation);
        self.trace.max_weight = self.trace.max_weight.max(self.director.weight());
        match (self.director.view(), self.open_run) {
            (Some(view), None) => self.open_run = Some((view.kind, self.now)),
            (None, Some((kind, start))) => {
                self.trace.runs.push((kind, start, self.now));
                self.open_run = None;
            }
            _ => {}
        }
        (x, animation)
    }

    fn run(&mut self, seconds: f64) {
        let mut link = NoLink;
        for _ in 0..(seconds / DT) as usize {
            self.frame(Planned::None, false, &mut link);
        }
    }
}

#[test]
fn les_niveaux_sans_cascade_n_en_lancent_aucune() {
    // Niveaux 0 et 1 : rangs vides.
    let mut closed = gates();
    closed.stunt_tiers.clear();
    let mut sim = Sim::new(1, full_registry(), &closed);
    sim.run(900.0);
    assert_eq!(sim.trace.runs.len(), 0);
    assert!(!sim.world.held);

    // Liste blanche vide.
    let mut none = gates();
    none.stunt_kinds.clear();
    let mut sim = Sim::new(1, full_registry(), &none);
    sim.run(900.0);
    assert_eq!(sim.trace.runs.len(), 0);
}

#[test]
fn la_voiture_ne_vole_jamais_au_niveau_zero() {
    // `max_floor` à 0 : même avec des rangs ouverts, rien ne part.
    let mut grounded = gates();
    grounded.max_floor = 0;
    let mut sim = Sim::new(2, full_registry(), &grounded);
    sim.run(900.0);
    assert_eq!(sim.trace.runs.len(), 0);
    assert!(sim.trace.max_weight.abs() < f64::EPSILON);
}

#[test]
fn le_niveau_qui_se_ferme_coupe_la_cascade_en_cours() {
    let open = gates();
    let mut sim = Sim::new(3, full_registry(), &open);
    let mut link = NoLink;
    while sim.director.view().is_none() {
        sim.frame(Planned::None, false, &mut link);
        assert!(sim.now < 400.0, "une cascade finit par partir");
    }
    assert!(sim.world.held);
    let mut grounded = gates();
    grounded.max_floor = 0;
    sim.gates = &grounded;
    for _ in 0..(1.0 / DT) as usize {
        sim.frame(Planned::None, false, &mut link);
    }
    assert!(sim.director.view().is_none(), "coupée en moins d'une seconde");
    assert!(!sim.world.held, "la ville reprend ses événements");
    assert_eq!(
        sim.world.fleet.stats().active,
        0,
        "les véhicules de la cascade sont rendus"
    );
}

#[test]
fn liste_blanche_et_rangs_sont_respectes() {
    let mut g = gates();
    g.stunt_kinds = kinds(&[StuntKind::Slalom, StuntKind::Loop, StuntKind::Rooftop]);
    g.stunt_tiers = vec![
        ShowStuntTier::new("common").unwrap(),
        ShowStuntTier::new("uncommon").unwrap(),
    ];
    let mut sim = Sim::new(4, full_registry(), &g);
    sim.run(3600.0);
    assert_ne!(sim.trace.runs.len(), 0);
    for (kind, ..) in &sim.trace.runs {
        // Loop est « rare » dans ce répertoire : refusée par les rangs ; le reste doit être sur la liste blanche.
        assert!(
            matches!(kind, StuntKind::Slalom | StuntKind::Rooftop),
            "cascade hors portes : {kind:?}"
        );
    }
    let seen: std::collections::HashSet<_> = sim.trace.runs.iter().map(|r| r.0).collect();
    assert_eq!(seen.len(), 2, "les deux cascades permises sortent : {seen:?}");
}

#[test]
fn jamais_l_une_des_trois_dernieres() {
    let g = gates();
    let mut sim = Sim::new(5, full_registry(), &g);
    sim.run(3600.0);
    let runs = &sim.trace.runs;
    assert!(runs.len() > 20, "{} cascades", runs.len());
    for window in runs.windows(4) {
        let last = window[3].0;
        assert!(window[..3].iter().all(|r| r.0 != last), "répétition : {window:?}");
    }
}

#[test]
fn l_ascension_a_sa_porte_son_delai_et_l_ordre_de_l_operateur() {
    // Porte fermée : jamais tirée au hasard.
    let g = gates();
    let mut sim = Sim::new(6, full_registry(), &g);
    sim.run(3600.0);
    assert!(sim.trace.runs.iter().all(|r| r.0 != StuntKind::Ascension));

    // Porte ouverte : tirée, mais pas deux fois en moins de 30 minutes.
    let mut open = gates();
    open.ascension = true;
    let mut sim = Sim::new(7, full_registry(), &open);
    sim.run(3.0 * 3600.0);
    let starts: Vec<f64> = sim
        .trace
        .runs
        .iter()
        .filter(|r| r.0 == StuntKind::Ascension)
        .map(|r| r.1)
        .collect();
    assert!(!starts.is_empty(), "l'Ascension sort porte ouverte");
    for pair in starts.windows(2) {
        assert!(pair[1] - pair[0] >= 1800.0, "délai respecté : {pair:?}");
    }

    // Ordre de l'opérateur : part au prochain temps fort malgré la porte fermée.
    let mut sim = Sim::new(8, full_registry(), &g);
    let mut link = TestLink {
        ascension_order: true,
        ..TestLink::default()
    };
    for _ in 0..(10.0 / DT) as usize {
        sim.frame(Planned::None, false, &mut link);
    }
    assert!(
        sim.director.view().is_some_and(|v| v.kind == StuntKind::Ascension)
            || sim.trace.runs.first().is_some_and(|r| r.0 == StuntKind::Ascension),
        "ordre honoré : {:?}",
        sim.trace.runs
    );
}

#[test]
fn le_drop_prepare_part_sur_le_drop_et_pas_avant() {
    let g = gates();
    let mut sim = Sim::new(9, full_registry(), &g);
    let mut link = TestLink::default();
    let plan = Planned::Kind(StuntKind::BigJump);
    // Montée du drop : la cascade préparée attend, rien d'autre ne part.
    for _ in 0..(60.0 / DT) as usize {
        sim.frame(plan, false, &mut link);
    }
    assert!(sim.director.view().is_none() && sim.trace.runs.is_empty());
    // Le drop frappe : elle part sur cette image, sans attendre une mesure.
    sim.frame(plan, true, &mut link);
    let view = sim.director.view().or_else(|| {
        // Le poids démarre à 0 : la vue est publiée dès que la pose pèse. Laisser une image de plus.
        sim.frame(Planned::None, false, &mut link);
        sim.director.view()
    });
    assert_eq!(view.map(|v| v.kind), Some(StuntKind::BigJump));
}

#[test]
fn un_drop_prepare_inconnu_retombe_sur_le_tirage() {
    let g = gates();
    let mut sim = Sim::new(10, full_registry(), &g);
    let mut link = TestLink::default();
    for _ in 0..(30.0 / DT) as usize {
        sim.frame(Planned::Unknown, false, &mut link);
    }
    sim.frame(Planned::Unknown, true, &mut link);
    for _ in 0..(60.0 / DT) as usize {
        sim.frame(Planned::None, false, &mut link);
    }
    assert!(
        !sim.trace.runs.is_empty() || sim.director.view().is_some(),
        "le tirage normal reprend"
    );
}

#[test]
fn la_respiration_et_la_route_occupee_retiennent_les_cascades() {
    let g = gates();
    let mut sim = Sim::new(11, full_registry(), &g);
    sim.breathing = true;
    sim.run(600.0);
    assert!(sim.trace.runs.is_empty(), "respiration : aucune cascade");

    let mut sim = Sim::new(11, full_registry(), &g);
    sim.world.busy = true;
    sim.run(600.0);
    assert!(sim.trace.runs.is_empty(), "une structure occupe la route");
}

#[test]
fn la_pose_est_fondue_et_toujours_valide() {
    let g = gates();
    let mut sim = Sim::new(12, full_registry(), &g);
    let mut link = NoLink;
    let mut flew = false;
    for _ in 0..(1200.0 / DT) as usize {
        let (x, a) = sim.frame(Planned::None, false, &mut link);
        let w = sim.director.weight();
        assert!((0.0..=1.0).contains(&w), "poids {w}");
        assert!(x.is_finite() && a.altitude.is_finite() && a.trick_pitch.is_finite() && a.trick_roll.is_finite());
        assert!(a.altitude >= 0.0 && a.altitude <= 6.001, "altitude {}", a.altitude);
        if w <= 0.0 {
            assert!(a.altitude.abs() < f32::EPSILON, "pas de vol hors cascade");
            assert!((x - 0.0).abs() < 1e-12);
        }
        flew |= a.altitude > 1.0;
    }
    assert!(flew, "la voiture a volé pendant une cascade");
}

#[test]
fn l_usage_est_note_pour_la_fraicheur_du_tirage() {
    let g = gates();
    let mut sim = Sim::new(13, full_registry(), &g);
    let mut link = TestLink::default();
    for _ in 0..(300.0 / DT) as usize {
        sim.frame(Planned::None, false, &mut link);
    }
    assert_ne!(link.marked.len(), 0);
    assert!(link.marked.iter().all(|id| id.starts_with("stunt:")));
}

/// Écart moyen (s) entre la fin d'une cascade et le départ de la suivante, sur une heure.
fn mean_gap(scale: f32, seed: u64) -> (f64, usize) {
    let mut g = gates();
    g.stunt_gap_scale = scale;
    let mut sim = Sim::new(seed, full_registry(), &g);
    sim.run(3600.0);
    let gaps: Vec<f64> = sim.trace.runs.windows(2).map(|w| w[1].1 - w[0].2).collect();
    (gaps.iter().sum::<f64>() / gaps.len() as f64, sim.trace.runs.len())
}

#[test]
fn l_ecart_entre_cascades_suit_stunt_gap_scale_sur_une_heure() {
    // Moyenne sur plusieurs graines : le tirage de l'attente est aléatoire.
    let avg = |scale: f32| {
        let mut total = 0.0;
        for seed in 20..28 {
            total += mean_gap(scale, seed).0;
        }
        total / 8.0
    };
    let (tight, normal, loose) = (avg(0.5), avg(1.0), avg(2.0));
    // Attente de Sillage : 16 à 34 s × (1,2 − 0,4 × 0,5) = 25 s en moyenne, plus le temps de saisir une mesure (≈ 1 s).
    assert!((normal - 26.0).abs() < 4.0, "écart nominal ≈ 26 s, mesuré {normal}");
    assert!(tight < normal && normal < loose, "{tight} < {normal} < {loose}");
    assert!((tight / normal - 0.5).abs() < 0.12, "échelle 0,5 : {tight} / {normal}");
    assert!((loose / normal - 2.0).abs() < 0.2, "échelle 2 : {loose} / {normal}");
}

#[test]
fn deux_graines_identiques_jouent_la_meme_suite() {
    let a = mean_gap(1.0, 99);
    let b = mean_gap(1.0, 99);
    assert!((a.0 - b.0).abs() < f64::EPSILON && a.1 == b.1);
}

#[test]
fn le_registre_vide_ne_lance_rien_et_dispose_rend_la_ville() {
    let g = gates();
    let mut sim = Sim::new(14, StuntRegistry::default(), &g);
    sim.run(300.0);
    assert_eq!(sim.trace.runs.len(), 0);

    let mut sim = Sim::new(15, full_registry(), &g);
    let mut link = NoLink;
    while sim.director.view().is_none() {
        sim.frame(Planned::None, false, &mut link);
    }
    sim.director.dispose(&mut sim.world);
    assert!(
        !sim.world.held && sim.world.fleet.stats().active == 0 || sim.world.fleet.crafts().iter().all(|c| !c.active)
    );
    assert!(sim.director.view().is_none());
}
