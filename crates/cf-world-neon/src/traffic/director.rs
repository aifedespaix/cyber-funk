//! Le directeur du trafic (`director.ts`) : chef d'orchestre de la vie de la ville. Le trafic de fond ne s'arrête
//! jamais ; par-dessus, des événements se déclenchent à intervalles irréguliers (voir [`Scheduler`]).
//!
//! Il possède tout ce que le trafic fait vivre : la flotte, le trafic au sol, les couloirs aériens, les points
//! lumineux et les murs de lumière. Logique pure : la couche Bevy (`systems`, `render`) lui donne les entrées de
//! l'image et recopie ce qu'il produit.

use bevy::ecs::resource::Resource;
use bevy::math::Vec3;
use cf_render::{Subject, SubjectKind};

use super::air_lanes::AirLanes;
use super::event::{EventCtx, EventRegistry, SignLender, TrafficEvent};
use super::fleet::{Craft, Fleet};
use super::frame::TrafficFrame;
use super::glints::Glints;
use super::ground::GroundTraffic;
use super::names::{EventName, MAX_PARALLEL};
use super::scene::Scene;
use super::scheduler::{Running, Scheduler, ShowLink};
use super::variant::{BIKE, CRAFT_LENGTH, HEAVY, POLICE};
use super::walls::LightWalls;
use crate::pilot::flight::{Band, Obstruction};
use crate::pilot::rand::Rand;
use crate::route::is_near_bridge;
use crate::scenery::Clearing;

/// Les ids des structures d'événements sont décalés pour ne jamais croiser ceux des véhicules (`100000 + uid`).
const STRUCTURE_ID_BASE: u64 = 100_000;
/// Une structure ne se pose pas sur une arche-immeuble : marge de recul et d'écart (m).
const BRIDGE_BACK_M: f64 = 400.0;
const BRIDGE_MARGIN_M: f64 = 90.0;
/// Un véhicule en vol de l'espace aérien : fenêtre autour de la voiture (m) et hauteur minimale (m).
const AIR_MIN_Y_M: f64 = 3.0;
const AIR_MAX_X_M: f64 = 14.0;
const AIR_AHEAD_M: f64 = 50.0;
const AIR_BEHIND_M: f64 = 190.0;

struct ActiveEvent {
    name: EventName,
    event: Box<dyn TrafficEvent>,
    uid: u32,
}

/// Le directeur du trafic. En ressource Bevy : un seul propriétaire, le système `drive_traffic`.
#[derive(Resource)]
pub struct TrafficDirector {
    /// Tous les véhicules volants.
    pub fleet: Fleet,
    /// Voitures au sol à dépasser (mode spectacle) : malus virtuels pour le pilote automatique.
    pub ground: GroundTraffic,
    /// Points lumineux de l'image.
    pub glints: Glints,
    /// Murs de lumière des motos.
    pub walls: LightWalls,
    /// Panneaux des structures, projecteurs et effets de ville que les événements écrivent à chaque image.
    pub scene: Scene,
    lanes: AirLanes,
    scheduler: Scheduler,
    rng: Rand,
    active: Vec<ActiveEvent>,
    uids: u32,
    /// Événement demandé de force (outil de développement) : tiré à l'image suivante.
    forced: Option<EventName>,
    /// Une cascade de la voiture est en cours : plus d'événements aléatoires.
    held: bool,
    /// Mesure en cours sur l'horloge de danse (quatre temps) : un événement part au début d'une mesure.
    bar: i64,
    flyby: bool,
    /// Ce que les événements en cours publient, recalculé à chaque image (tampons réutilisés).
    claims: Vec<Band>,
    obstructions: Vec<Obstruction>,
    clearings: Vec<Clearing>,
    clearings_changed: bool,
    last_glints: usize,
}

impl std::fmt::Debug for TrafficDirector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrafficDirector")
            .field("events", &self.active_names().collect::<Vec<_>>())
            .field("held", &self.held)
            .finish_non_exhaustive()
    }
}

impl TrafficDirector {
    /// Un directeur au hasard déterministe de graine `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            fleet: Fleet::new(),
            ground: GroundTraffic::default(),
            glints: Glints::new(),
            walls: LightWalls::new(),
            scene: Scene::new(),
            lanes: AirLanes::default(),
            scheduler: Scheduler::default(),
            rng: Rand::new(seed),
            active: Vec::with_capacity(MAX_PARALLEL + 1),
            uids: 0,
            forced: None,
            held: false,
            bar: 0,
            flyby: false,
            claims: Vec::with_capacity(8),
            obstructions: Vec::with_capacity(16),
            clearings: Vec::with_capacity(MAX_PARALLEL),
            clearings_changed: false,
            last_glints: 0,
        }
    }

    /// Suspend (ou reprend) le tirage des événements aléatoires : une cascade scénarisée a la ville pour elle.
    pub fn hold(&mut self, on: bool) {
        self.held = on;
    }

    /// Outil de développement : lance cet événement à la prochaine image, quelles que soient les portes.
    pub fn force(&mut self, name: EventName) {
        self.forced = Some(name);
    }

    /// Les événements en cours.
    pub fn active_names(&self) -> impl Iterator<Item = EventName> + '_ {
        self.active.iter().map(|a| a.name)
    }

    /// Nombre d'événements en cours.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    /// Une structure (route croisée, méga-tour, immeuble-vitrine) occupe la route en ce moment.
    #[must_use]
    pub fn structure_busy(&self) -> bool {
        self.active.iter().any(|a| a.event.structure())
    }

    /// Vrai tant qu'une animation de véhicules volants (poursuite, convoi, nuée…) passe près de la voiture : on ne
    /// change pas d'étage.
    #[must_use]
    pub fn flyby_near(&self) -> bool {
        self.flyby
    }

    /// Points lumineux posés à la dernière image (outil de développement, compteurs).
    #[must_use]
    pub fn glint_count(&self) -> usize {
        self.last_glints
    }

    /// Les structures qui enjambent la route en ce moment (route croisée surélevée, arche de voxels, mecha…), telles
    /// que les événements les ont publiées à la dernière image.
    #[must_use]
    pub fn obstructions(&self) -> &[Obstruction] {
        &self.obstructions
    }

    /// Les zones sans immeuble que les événements en cours réclament, si elles ont changé depuis le dernier appel.
    pub fn take_clearings(&mut self) -> Option<&[Clearing]> {
        std::mem::take(&mut self.clearings_changed).then_some(self.clearings.as_slice())
    }

    /// Tranches d'altitude occupées près de la route, devant et derrière la voiture : les véhicules en vol et les
    /// réservations des événements. La voiture ne monte pas dans une tranche occupée.
    pub fn airspace(&self, out: &mut Vec<Band>, car_z: f64) {
        self.fleet.for_each_active(|craft| {
            let p = craft.position;
            if p.y < AIR_MIN_Y_M || p.x.abs() > AIR_MAX_X_M || p.z > car_z + AIR_AHEAD_M || p.z < car_z - AIR_BEHIND_M {
                return;
            }
            let half = f64::from(craft.scale) * 0.8 + 1.4;
            out.push(Band {
                lo: p.y - half,
                hi: p.y + half,
            });
        });
        out.extend_from_slice(&self.claims);
    }

    /// Les sujets filmables du mode cinéma : véhicules de la flotte (police, fuyard d'une poursuite, motos, gros
    /// porteurs, autres) puis structures d'événements.
    pub fn collect_subjects(&self, mut push: impl FnMut(Subject)) {
        let quarry = self.active.iter().find_map(|a| a.event.quarry());
        self.fleet.for_each_active(|craft| {
            push(Subject {
                id: u64::from(craft.id),
                kind: subject_kind(craft, quarry),
                position: craft.position,
                velocity: craft.velocity,
                size: craft.scale * CRAFT_LENGTH,
                facing: None,
            });
        });
        for a in &self.active {
            if let Some(focus) = a.event.focus() {
                push(Subject {
                    id: STRUCTURE_ID_BASE + u64::from(a.uid),
                    kind: SubjectKind::Landmark,
                    position: focus.position,
                    velocity: Vec3::ZERO,
                    size: focus.size,
                    facing: None,
                });
            }
        }
    }

    /// Avance le trafic d'une image : couloirs, tirage et vie des événements, trafic au sol, flotte, murs.
    ///
    /// `due(craft)` dit si c'est le tour d'un véhicule (cadence réduite hors champ, voir [`Fleet::update`]).
    pub fn update(
        &mut self,
        frame: &TrafficFrame<'_>,
        registry: &mut EventRegistry,
        link: &mut dyn ShowLink,
        due: impl FnMut(&Craft) -> bool,
    ) {
        self.update_with_signs(frame, registry, link, None, due);
    }

    /// Comme [`TrafficDirector::update`], avec le gestionnaire de panneaux de la route prêté aux événements pour cette
    /// image (le vaisseau publicitaire y prend ses écrans).
    pub fn update_with_signs(
        &mut self,
        frame: &TrafficFrame<'_>,
        registry: &mut EventRegistry,
        link: &mut dyn ShowLink,
        mut signs: Option<&mut (dyn SignLender + '_)>,
        due: impl FnMut(&Craft) -> bool,
    ) {
        self.scheduler.track_show(frame, &mut self.rng);
        self.glints.begin(frame.car);
        self.scene.begin(frame.car);
        let bar = (frame.dance_beat / 4.0).floor() as i64;
        let bar_started = bar != self.bar;
        self.bar = bar;

        {
            let mut ctx = EventCtx {
                frame,
                fleet: &mut self.fleet,
                glints: &mut self.glints,
                walls: &mut self.walls,
                rng: &mut self.rng,
                scene: &mut self.scene,
                // `Some(s)` est un site de coercion : la durée de vie de l'objet-trait raccourcit avec celle de l'emprunt
                // (derrière `Option`, `as_deref_mut` seul ne la raccourcirait pas : `&mut` est invariant).
                signs: match signs.as_deref_mut() {
                    Some(s) => Some(s),
                    None => None,
                },
            };
            if frame.playing {
                self.lanes.update(frame.dt, &mut ctx);
            }
            if frame.playing && !self.held {
                Self::schedule(
                    &mut self.scheduler,
                    &mut self.active,
                    &mut self.uids,
                    &mut self.forced,
                    registry,
                    link,
                    bar_started,
                    &mut ctx,
                );
            }
            let mut i = self.active.len();
            while i > 0 {
                i -= 1;
                if !self.active[i].event.update(frame.dt, &mut ctx) {
                    let mut ended = self.active.remove(i);
                    ended.event.end(&mut ctx);
                }
            }
            Self::publish(
                &self.active,
                &mut self.claims,
                &mut self.obstructions,
                &mut self.flyby,
                &ctx,
            );
        }
        self.publish_clearings();

        let busy = self.held || self.active.iter().any(|a| a.event.structure());
        self.ground.update(frame, busy, &mut self.fleet, &mut self.rng);
        self.fleet.update(frame.dt, frame.car.z, due);
        self.walls.update(frame.time);
        self.last_glints = self.glints.used();
    }

    /// Arrête tous les événements et rend tous les véhicules (arrêt du spectacle, fermeture).
    pub fn shutdown(&mut self, frame: &TrafficFrame<'_>) {
        {
            let mut ctx = EventCtx {
                frame,
                fleet: &mut self.fleet,
                glints: &mut self.glints,
                walls: &mut self.walls,
                rng: &mut self.rng,
                scene: &mut self.scene,
                signs: None,
            };
            for mut a in self.active.drain(..) {
                a.event.end(&mut ctx);
            }
        }
        self.ground.clear(&mut self.fleet);
        self.fleet.clear();
        // Plus rien à montrer : ni panneaux, ni projecteurs, ni panne de courant.
        self.scene.begin(frame.car);
        self.claims.clear();
        self.obstructions.clear();
        self.flyby = false;
        self.publish_clearings();
    }

    #[allow(clippy::too_many_arguments)] // Les morceaux disjoints du directeur : les passer ensemble évite d'emprunter `self`.
    fn schedule(
        scheduler: &mut Scheduler,
        active: &mut Vec<ActiveEvent>,
        uids: &mut u32,
        forced: &mut Option<EventName>,
        registry: &mut EventRegistry,
        link: &mut dyn ShowLink,
        bar_started: bool,
        ctx: &mut EventCtx<'_>,
    ) {
        let frame = ctx.frame;
        let mut running = [Running {
            name: EventName::Chase,
            structure: false,
        }; MAX_PARALLEL];
        let n = active.len().min(MAX_PARALLEL);
        for (slot, a) in running.iter_mut().zip(active.iter()) {
            *slot = Running {
                name: a.name,
                structure: a.event.structure(),
            };
        }
        let next = forced
            .take()
            .or_else(|| scheduler.next(frame, &running[..n], registry, link, bar_started, ctx.rng));
        let Some(name) = next else { return };
        // Une structure ne se pose pas sur une arche-immeuble : on retente un peu plus tard.
        if name.is_structure_kind() && is_near_bridge(frame.car.z - BRIDGE_BACK_M, BRIDGE_MARGIN_M) {
            scheduler.postponed();
            return;
        }
        let Some(event) = registry.create(name, ctx) else {
            return;
        };
        scheduler.started(name, frame, link, ctx.rng);
        *uids += 1;
        active.push(ActiveEvent {
            name,
            event,
            uid: *uids,
        });
    }

    /// Ce que les événements en cours publient pour la voiture et la ville, une fois leur image calculée.
    fn publish(
        active: &[ActiveEvent],
        claims: &mut Vec<Band>,
        obstructions: &mut Vec<Obstruction>,
        flyby: &mut bool,
        ctx: &EventCtx<'_>,
    ) {
        claims.clear();
        obstructions.clear();
        *flyby = false;
        for a in active {
            if let Some(band) = a.event.claim(ctx) {
                claims.push(band);
            }
            if let Some(o) = a.event.obstruction() {
                obstructions.push(o);
            }
            a.event.obstructions(obstructions);
            *flyby |= a.event.proximity(ctx);
        }
    }

    /// Transmet à la ville les zones à dégager, uniquement quand elles changent.
    fn publish_clearings(&mut self) {
        let mut same = true;
        let mut n = 0;
        for a in &self.active {
            if let Some(c) = a.event.clearing() {
                same &= self.clearings.get(n) == Some(&c);
                n += 1;
            }
        }
        if same && n == self.clearings.len() {
            return;
        }
        self.clearings.clear();
        for a in &self.active {
            if let Some(c) = a.event.clearing() {
                self.clearings.push(c);
            }
        }
        self.clearings_changed = true;
    }
}

/// Le genre de sujet d'un véhicule : police, moto, fuyard, gros porteur ou ordinaire.
fn subject_kind(craft: &Craft, quarry: Option<u16>) -> SubjectKind {
    match craft.variant {
        POLICE => SubjectKind::Police,
        BIKE => SubjectKind::Bike,
        _ if quarry == Some(craft.id) => SubjectKind::Quarry,
        v if HEAVY.contains(&v) => SubjectKind::Heavy,
        _ => SubjectKind::Craft,
    }
}

/// La position d'un point de l'espace pour les tests : `DVec3` à `y` donné.
#[cfg(test)]
fn at(x: f64, y: f64, z: f64) -> bevy::math::DVec3 {
    bevy::math::DVec3::new(x, y, z)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::scheduler::NoShow;
    use crate::traffic::variant::SLEEK;
    use bevy::color::LinearRgba;

    /// Un événement de test : prend `n` véhicules, en rend un à la fin, dure `life` secondes.
    #[derive(Debug)]
    struct Probe {
        life: f32,
        ids: Vec<u16>,
        structure: bool,
        clearing: Option<Clearing>,
    }

    impl TrafficEvent for Probe {
        fn structure(&self) -> bool {
            self.structure
        }

        fn clearing(&self) -> Option<Clearing> {
            self.clearing
        }

        fn focus(&self) -> Option<super::super::event::Focus> {
            self.structure.then_some(super::super::event::Focus {
                position: at(0.0, 10.0, -100.0),
                size: 40.0,
            })
        }

        fn update(&mut self, dt: f32, _ctx: &mut EventCtx<'_>) -> bool {
            self.life -= dt;
            self.life > 0.0
        }

        fn end(&mut self, ctx: &mut EventCtx<'_>) {
            for id in self.ids.drain(..) {
                ctx.fleet.release(id);
            }
        }
    }

    fn probe(structure: bool) -> Box<dyn TrafficEvent> {
        Box::new(Probe {
            life: 3.0,
            ids: Vec::new(),
            structure,
            clearing: structure.then_some(Clearing {
                z0: -200.0,
                z1: -100.0,
                side: 0,
            }),
        })
    }

    fn step(director: &mut TrafficDirector, registry: &mut EventRegistry, frame: &TrafficFrame<'_>, seconds: f32) {
        let mut t = 0.0;
        while t < seconds {
            let now = TrafficFrame {
                time: frame.time + f64::from(t),
                ..*frame
            };
            director.update(&now, registry, &mut NoShow, |_| true);
            t += frame.dt;
        }
    }

    #[test]
    fn sans_evenement_inscrit_seul_le_trafic_de_fond_vit() {
        let mut director = TrafficDirector::new(1);
        let mut registry = EventRegistry::default();
        step(
            &mut director,
            &mut registry,
            &TrafficFrame {
                dt: 0.1,
                ..TrafficFrame::default()
            },
            60.0,
        );
        assert_eq!(director.active_count(), 0);
        assert!(
            director.fleet.stats().active > 0,
            "le trafic de fond ne s'arrête jamais"
        );
    }

    #[test]
    fn un_evenement_force_demarre_vit_puis_libere_ce_qu_il_tient() {
        let mut director = TrafficDirector::new(2);
        let mut registry = EventRegistry::default();
        registry.register(
            EventName::Fish,
            Box::new(|ctx| {
                let mut ids = Vec::new();
                for _ in 0..3 {
                    if let Some(id) = ctx.fleet.spawn(
                        SLEEK[0],
                        at(0.0, 20.0, -50.0),
                        Vec3::ZERO,
                        LinearRgba::WHITE,
                        1.0,
                        ctx.rng,
                    ) {
                        ids.push(id);
                    }
                }
                Box::new(Probe {
                    life: 3.0,
                    ids,
                    structure: false,
                    clearing: None,
                })
            }),
        );
        let frame = TrafficFrame {
            dt: 0.1,
            ..TrafficFrame::default()
        };
        let before = {
            step(&mut director, &mut registry, &frame, 0.1);
            director.fleet.stats().active
        };
        director.force(EventName::Fish);
        step(&mut director, &mut registry, &frame, 0.2);
        assert_eq!(director.active_names().collect::<Vec<_>>(), vec![EventName::Fish]);
        assert!(director.fleet.stats().active >= before + 3);
        step(&mut director, &mut registry, &frame, 4.0);
        assert_eq!(director.active_count(), 0, "terminé");
    }

    #[test]
    fn une_structure_publie_sa_zone_degagee_une_seule_fois_et_la_retire_a_la_fin() {
        let mut director = TrafficDirector::new(3);
        let mut registry = EventRegistry::default();
        registry.register(EventName::Tower, Box::new(|_| probe(true)));
        let frame = TrafficFrame {
            dt: 0.1,
            car: at(0.0, 0.0, 5000.0),
            ..TrafficFrame::default()
        };
        director.force(EventName::Tower);
        step(&mut director, &mut registry, &frame, 0.1);
        if director.structure_busy() {
            assert_eq!(director.take_clearings().map(<[Clearing]>::len), Some(1));
            assert!(director.take_clearings().is_none(), "déjà transmise");
            step(&mut director, &mut registry, &frame, 4.0);
            assert_eq!(director.take_clearings().map(<[Clearing]>::len), Some(0));
        }
    }

    #[test]
    fn les_sujets_distinguent_police_moto_gros_porteur_et_structure() {
        let mut director = TrafficDirector::new(4);
        let mut rng = Rand::new(1);
        for variant in [POLICE, BIKE, HEAVY[0], SLEEK[0]] {
            director.fleet.spawn(
                variant,
                at(0.0, 5.0, -80.0),
                Vec3::new(0.0, 0.0, -30.0),
                LinearRgba::WHITE,
                2.0,
                &mut rng,
            );
        }
        let mut subjects = Vec::new();
        director.collect_subjects(|s| subjects.push(s));
        let kinds: Vec<_> = subjects.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                SubjectKind::Police,
                SubjectKind::Bike,
                SubjectKind::Heavy,
                SubjectKind::Craft
            ]
        );
        assert!((subjects[0].size - 2.0 * CRAFT_LENGTH).abs() < 1e-6);
    }

    #[test]
    fn l_espace_aerien_liste_les_vehicules_proches_en_vol() {
        let mut director = TrafficDirector::new(5);
        let mut rng = Rand::new(1);
        // Dans la fenêtre, trop bas, trop loin sur le côté, trop loin devant.
        for pos in [
            at(0.0, 20.0, -20.0),
            at(0.0, 1.0, -20.0),
            at(30.0, 20.0, -20.0),
            at(0.0, 20.0, -400.0),
        ] {
            director
                .fleet
                .spawn(SLEEK[0], pos, Vec3::ZERO, LinearRgba::WHITE, 1.9, &mut rng);
        }
        let mut bands = Vec::new();
        director.airspace(&mut bands, 0.0);
        assert_eq!(bands.len(), 1);
        assert!((bands[0].hi - bands[0].lo - 2.0 * (1.9 * 0.8 + 1.4)).abs() < 1e-5);
    }
}
