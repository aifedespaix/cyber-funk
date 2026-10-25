//! Le chef d'orchestre des cascades (`stunts/director.ts`) : il choisit une cascade (rareté, jamais l'une des trois
//! dernières), la lance sur une mesure, une phrase ou un drop, la déroule, et fond sa pose sur celle de la conduite
//! normale (jamais de saut). Pendant ce temps, les événements aléatoires du trafic sont suspendus.
//!
//! Pur : le temps et le hasard sont injectés, la ville et les véhicules passent par [`StuntWorld`], le chef du spectacle
//! par [`StuntLink`]. La couche Bevy est dans [`super::systems`].

use bevy::math::DVec3;
use bevy::prelude::Resource;

use super::kind::{StuntKind, StuntTier};
use super::math::{between, lerp};
use super::permit::{AscensionGate, StuntLink, level_open, permitted};
use super::script::{Planned, StuntCtx, StuntEnv, StuntPose, StuntRegistry, StuntRun, StuntShow};
use super::world::StuntWorld;
use crate::pilot::CarAnimation;
use crate::pilot::flight::FlightPhase;
use crate::pilot::rand::Rand;
use crate::traffic::TrafficBeat;

/// Fondu d'entrée et de sortie de la pose sur la conduite normale (s).
pub const RAMP: f64 = 0.8;
/// Fondu quand la cascade est coupée en route (musique arrêtée, points rallumés) ou finit plus tôt que prévu.
pub const ABORT: f64 = 0.6;
/// Les trois dernières cascades ne reviennent pas.
const HISTORY: usize = 3;
/// Entre deux cascades : de quoi laisser la conduite normale respirer (s), plus court quand la musique est intense.
pub const GAP: (f64, f64) = (16.0, 34.0);
/// Avant la première cascade (s), et après l'entrée dans un niveau qui les permet.
pub const FIRST_GAP: (f64, f64) = (5.0, 10.0);
/// Deux grosses cascades ne se suivent pas de moins de ce délai (s).
const RARE_GAP: f64 = 100.0;
/// Sans mesure à saisir depuis ce retard (s), la cascade part quand même.
const OVERDUE: f64 = 6.0;
/// La route doit défiler à cette vitesse au moins pour qu'on lance une cascade (m/s).
const MIN_SPEED: f64 = 12.0;

/// Ce que le directeur lit à chaque image (`StuntFrame`).
#[derive(Debug, Clone, Copy)]
pub struct StuntFrame<'a> {
    /// Durée de l'image, s.
    pub dt: f64,
    /// Horloge de la scène, s.
    pub now: f64,
    /// Les conditions sont réunies : pilote en marche, sans points, musique, pas de mouvement réduit.
    pub enabled: bool,
    /// Abscisse de la voiture (conduite normale), m.
    pub car_x: f64,
    /// Profondeur de la voiture, m.
    pub car_z: f64,
    /// Altitude de la voiture hors cascade, m.
    pub altitude: f64,
    /// Vitesse du défilement de la route, m/s.
    pub road_speed: f64,
    /// L'audio de l'image.
    pub beat: TrafficBeat,
    /// Un drop commence cette image (hors spectacle).
    pub drop: bool,
    /// Énergie du morceau, 0 → 1.
    pub intensity: f64,
    /// Le spectacle (portes, drop préparé) ; `None` hors spectacle.
    pub show: Option<StuntShow<'a>>,
}

/// Ce que le mode cinéma sait de la cascade en cours (`StuntView`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StuntView {
    /// Laquelle.
    pub kind: StuntKind,
    /// Progression, 0 → 1.
    pub progress: f32,
    /// Le point à regarder (absolu, m).
    pub focus: DVec3,
}

/// Temps ramené entre 0,4 et 0,75 s : un tempo rapide ou lent se compte à la moitié ou au double.
#[must_use]
pub fn fold_period(bpm: Option<f32>) -> f64 {
    let mut period = bpm.filter(|b| *b > 0.0).map_or(0.5, |b| 60.0 / f64::from(b));
    while period < 0.4 {
        period *= 2.0;
    }
    while period > 0.75 {
        period /= 2.0;
    }
    period
}

/// Le chef d'orchestre. En ressource Bevy : un seul propriétaire, le système `drive_stunts`.
#[allow(clippy::struct_excessive_bools)] // Les drapeaux d'état de Sillage (`active`, `dying`, `aborted`…), repris tels quels.
#[derive(Resource)]
pub struct StuntDirector {
    /// Une cascade tient la voiture (la pose pèse plus que rien).
    active: bool,
    /// Poids de la pose sur la conduite normale, 0 → 1.
    weight: f64,
    /// Ni traînée ni fumée : la voiture vole, roule sur un mur ou hors de la route.
    fx_off: bool,
    run: Option<Box<dyn StuntRun>>,
    kind: StuntKind,
    t: f64,
    fade: f64,
    /// La ligne de temps est finie (ou coupée) : la pose est figée et s'estompe.
    dying: bool,
    /// Coupée en route : les angles s'éteignent aussi, au lieu de rester sur un tour complet.
    aborted: bool,
    wait: f64,
    last_rare: f64,
    history: Vec<StuntKind>,
    pose: StuntPose,
    progress: f64,
    period: f64,
    candidates: Vec<StuntKind>,
    weights: Vec<f64>,
    ascension: AscensionGate,
    entered: bool,
    show_seen: bool,
    scale: f64,
    warned_unknown: bool,
    rng: Rand,
}

impl std::fmt::Debug for StuntDirector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StuntDirector")
            .field("active", &self.active)
            .field("kind", &self.kind)
            .field("weight", &self.weight)
            .field("wait", &self.wait)
            .finish_non_exhaustive()
    }
}

impl StuntDirector {
    /// Un chef d'orchestre au hasard déterministe de graine `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rng = Rand::new(seed);
        let wait = rng.range(FIRST_GAP.0, FIRST_GAP.1);
        Self {
            active: false,
            weight: 0.0,
            fx_off: false,
            run: None,
            kind: StuntKind::Loop,
            t: 0.0,
            fade: 1.0,
            dying: false,
            aborted: false,
            wait,
            last_rare: f64::NEG_INFINITY,
            history: Vec::with_capacity(HISTORY + 1),
            pose: StuntPose::default(),
            progress: 0.0,
            period: 0.5,
            candidates: Vec::with_capacity(super::kind::STUNT_COUNT),
            weights: Vec::with_capacity(super::kind::STUNT_COUNT),
            ascension: AscensionGate::default(),
            entered: true,
            show_seen: false,
            scale: 1.0,
            warned_unknown: false,
            rng,
        }
    }

    /// Une cascade tient la voiture.
    #[must_use]
    pub fn active(&self) -> bool {
        self.active
    }

    /// Poids de la pose sur la conduite normale, 0 → 1.
    #[must_use]
    pub fn weight(&self) -> f64 {
        self.weight
    }

    /// Ni traînée ni fumée.
    #[must_use]
    pub fn fx_off(&self) -> bool {
        self.fx_off
    }

    /// La pose voulue à cet instant.
    #[must_use]
    pub fn pose(&self) -> &StuntPose {
        &self.pose
    }

    /// La cascade en cours pour le mode cinéma, ou `None`.
    #[must_use]
    pub fn view(&self) -> Option<StuntView> {
        self.active.then_some(StuntView {
            kind: self.kind,
            progress: self.progress as f32,
            focus: self.pose.focus,
        })
    }

    /// À appeler une fois par image, avant de poser la voiture. Renvoie l'abscisse de la voiture (la pose fondue sur
    /// `f.car_x`).
    pub fn update(
        &mut self,
        f: &StuntFrame<'_>,
        registry: &StuntRegistry,
        world: &mut dyn StuntWorld,
        link: &mut dyn StuntLink,
    ) -> f64 {
        let env = StuntEnv {
            dt: f.dt,
            time: f.now,
            car_x: f.car_x,
            car_z: f.car_z,
            altitude: f.altitude,
            speed: f.road_speed,
            period: self.period,
            beat: f.beat,
            show: f.show,
        };
        let enabled = f.enabled;
        let open = self.track(f.show.as_ref(), f.now, link);

        if self.run.is_none() {
            self.weight = 0.0;
            self.fx_off = false;
            self.active = false;
            // Niveaux 0 et 1, respiration : aucune cascade, et l'attente ne court pas.
            if !enabled || !open || f.show.is_some_and(|s| s.breathing) {
                return f.car_x;
            }
            self.wait -= f.dt;
            self.try_start(f, env, registry, world, link);
            if self.run.is_none() {
                return f.car_x;
            }
        }

        self.t += f.dt;
        // Le niveau qui se ferme en route (retour au niveau 0 : la voiture ne vole pas) coupe la cascade comme la musique.
        if !enabled || !open {
            self.dying = true;
            self.aborted = true;
        }
        if self.dying {
            self.fade = (self.fade - f.dt / ABORT).max(0.0);
        } else if let Some(run) = self.run.as_mut() {
            self.pose.focus = DVec3::new(f.car_x, f.altitude, f.car_z);
            let mut ctx = StuntCtx {
                env: &env,
                world: &mut *world,
                rng: &mut self.rng,
            };
            if !run.step(self.t, &mut ctx, &mut self.pose) {
                self.dying = true;
            }
        }

        let duration = self.run.as_ref().map_or(0.0, |r| r.duration());
        let left = (duration - self.t).max(0.0);
        let ramp = between(self.t, 0.0, RAMP) * between(left, 0.0, RAMP);
        self.weight = ramp * self.fade;
        if self.fade <= 0.0 || self.t >= duration {
            self.finish(world, f.intensity);
            return f.car_x;
        }
        self.active = self.weight > 0.0;
        self.progress = (self.t / duration).clamp(0.0, 1.0);
        let pose = &self.pose;
        self.fx_off = self.weight > 0.1 && (pose.altitude > 0.25 || pose.roll.abs() > 0.4 || pose.x.abs() > 9.2);
        lerp(f.car_x, pose.x, self.weight)
    }

    /// Fond la pose sur le vol normal (altitude, inclinaisons, figures) : la voiture n'a plus qu'à la jouer.
    pub fn apply(&self, frame: &mut CarAnimation) {
        let w = self.weight;
        if self.run.is_none() || w <= 0.0 {
            return;
        }
        let pose = &self.pose;
        // Coupée en route, la pose n'est plus tenue par la ligne de temps : les angles s'éteignent avec le poids.
        let angle = if self.aborted { self.fade } else { 1.0 };
        let keep = (1.0 - w) as f32;
        frame.altitude = lerp(f64::from(frame.altitude), pose.altitude, w) as f32;
        frame.pitch *= keep;
        frame.roll *= keep;
        frame.trick_pitch += (pose.pitch * angle) as f32;
        frame.trick_roll += (pose.roll * angle) as f32;
        frame.dodge = crate::pilot::DodgeSide::None;
        if pose.hover && w > 0.02 {
            frame.hover = true;
            frame.hover_amount = lerp(f64::from(frame.hover_amount), 1.0, w) as f32;
            if frame.phase == FlightPhase::Ground {
                frame.phase = FlightPhase::Flying;
            }
        }
    }

    /// Arrêt de force (fin du spectacle, fermeture) : libère les véhicules et rend la ville au trafic.
    pub fn dispose(&mut self, world: &mut dyn StuntWorld) {
        if let Some(mut run) = self.run.take() {
            run.end(world);
        }
        self.active = false;
        self.weight = 0.0;
        self.fx_off = false;
        world.hold(false);
    }

    /// Suit les portes du spectacle : l'attente ne compte que depuis l'entrée dans un niveau qui permet les cascades (le
    /// premier délai repart alors de zéro). Renvoie vrai si des cascades sont permises (toujours hors spectacle).
    fn track(&mut self, show: Option<&StuntShow<'_>>, now: f64, link: &mut dyn StuntLink) -> bool {
        if show.is_some() && !self.show_seen {
            self.entered = false;
        }
        self.show_seen = show.is_some();
        self.scale = show.map_or(1.0, |s| f64::from(s.gates.stunt_gap_scale));
        let open = show.is_none_or(level_open);
        if !open {
            self.entered = false;
        } else if !self.entered {
            self.entered = true;
            if show.is_some() {
                self.wait = self.rng.range(FIRST_GAP.0, FIRST_GAP.1) * self.scale;
            }
        }
        if let (Some(_), true) = (show, open) {
            self.ascension.poll(link, now);
        }
        open
    }

    /// Tente de lancer une cascade à ce temps de la musique : ordre de l'opérateur, drop préparé, sinon tirage.
    fn try_start(
        &mut self,
        f: &StuntFrame<'_>,
        mut env: StuntEnv<'_>,
        registry: &StuntRegistry,
        world: &mut dyn StuntWorld,
        link: &mut dyn StuntLink,
    ) {
        let b = f.beat;
        let bar = b.kick && b.count.is_multiple_of(4);
        let phrase = bar && b.count.is_multiple_of(16);
        let drop = f.show.map_or(f.drop, |s| s.drop_now);
        let planned = f.show.map_or(Planned::None, |s| s.planned);
        if f.road_speed < MIN_SPEED || world.busy() {
            return;
        }
        self.period = fold_period(b.bpm);
        env.period = self.period;

        // L'Ascension demandée à la main part au prochain temps fort, sans regarder la porte `ascension`.
        if f.show.is_some()
            && (bar || drop)
            && self.ascension.queued(f.now)
            && self.launch(StuntKind::Ascension, false, &env, registry, world, link)
        {
            return;
        }
        if planned != Planned::None && !drop {
            return;
        }
        // Le drop frappe : la cascade préparée pendant la montée part sur cette image, même si l'attente n'est pas finie.
        if drop {
            match planned {
                Planned::Kind(kind) => {
                    let cooled = kind != StuntKind::Ascension || self.ascension.ready(f.now);
                    if cooled && self.launch(kind, false, &env, registry, world, link) {
                        return;
                    }
                }
                Planned::Unknown if !self.warned_unknown => {
                    // Le chef des drops a préparé une cascade que le catalogue ne connaît pas : on tire comme sans plan.
                    self.warned_unknown = true;
                    tracing::warn!("cascades : le drop préparé réclame une cascade inconnue du catalogue");
                }
                _ => {}
            }
        }
        if planned != Planned::Unknown {
            self.warned_unknown = false;
        }

        // Un drop interrompt l'attente : c'est le moment de la plus grosse cascade.
        if !(self.wait <= 0.0 || (drop && self.wait < 10.0)) {
            return;
        }
        let overdue = self.wait < -OVERDUE;
        // Passé le retard, on part sur n'importe quel instant ; sinon il faut un temps fort ou un drop.
        if !bar && !drop && !overdue {
            return;
        }
        let when = Moment {
            bar,
            phrase,
            drop,
            overdue,
        };
        let mut total = self.gather(f, &env, registry, world, link, when, false);
        if total <= 0.0 && drop {
            total = self.gather(f, &env, registry, world, link, when, true);
        }
        if self.candidates.is_empty() || total <= 0.0 {
            return;
        }
        let mut roll = self.rng.unit() * total;
        let mut chosen = self.candidates[self.candidates.len() - 1];
        for (kind, weight) in self.candidates.iter().zip(&self.weights) {
            roll -= weight;
            if roll <= 0.0 {
                chosen = *kind;
                break;
            }
        }
        self.launch(chosen, true, &env, registry, world, link);
    }

    /// Remplit `candidates` et `weights` ; `relaxed` : au drop, faute de cascade alignée « drop », toutes les autres
    /// conviennent.
    #[allow(clippy::too_many_arguments)] // Le contexte du tirage : une struct de plus n'éclairerait rien.
    fn gather(
        &mut self,
        f: &StuntFrame<'_>,
        env: &StuntEnv<'_>,
        registry: &StuntRegistry,
        world: &mut dyn StuntWorld,
        link: &mut dyn StuntLink,
        when: Moment,
        relaxed: bool,
    ) -> f64 {
        use super::kind::StuntAlign as Align;
        self.candidates.clear();
        self.weights.clear();
        let mut total = 0.0;
        let ready = self.ascension.ready(f.now);
        for script in registry.iter() {
            let kind = script.kind();
            if self.history.contains(&kind) || !permitted(script, f.show.as_ref(), ready) {
                continue;
            }
            let fits = match script.align() {
                Align::Drop => when.drop,
                _ if relaxed => true,
                Align::Phrase => when.phrase || when.overdue,
                Align::Bar => when.bar || when.overdue,
            };
            if !fits {
                continue;
            }
            let tier = script.tier();
            if tier == StuntTier::Rare && f.now - self.last_rare < RARE_GAP {
                continue;
            }
            if !script.available(env, &*world) {
                continue;
            }
            // Les grosses cascades aiment les moments intenses ; le spectacle écarte celles qui ont trop servi.
            let mut weight = script.weight()
                * tier.factor()
                * if tier == StuntTier::Rare {
                    0.5 + f.intensity
                } else {
                    1.0
                };
            if f.show.is_some() {
                weight *= link.freshness_weight(kind.freshness_id(), f.now);
            }
            self.candidates.push(kind);
            self.weights.push(weight);
            total += weight;
        }
        total
    }

    /// Lance `kind` si la route le permet ; `mark` : noter l'usage (le chef des drops l'a déjà fait pour sa cascade
    /// préparée).
    fn launch(
        &mut self,
        kind: StuntKind,
        mark: bool,
        env: &StuntEnv<'_>,
        registry: &StuntRegistry,
        world: &mut dyn StuntWorld,
        link: &mut dyn StuntLink,
    ) -> bool {
        let Some(script) = registry.get(kind) else {
            return false;
        };
        if !script.available(env, &*world) {
            return false;
        }
        let mut ctx = StuntCtx {
            env,
            world: &mut *world,
            rng: &mut self.rng,
        };
        self.run = Some(script.begin(&mut ctx));
        self.t = 0.0;
        self.fade = 1.0;
        self.dying = false;
        self.aborted = false;
        self.kind = kind;
        self.progress = 0.0;
        self.pose = StuntPose {
            x: env.car_x,
            altitude: env.altitude,
            pitch: 0.0,
            roll: 0.0,
            hover: false,
            focus: DVec3::new(env.car_x, env.altitude, env.car_z),
        };
        world.hold(true);
        if script.tier() == StuntTier::Rare {
            self.last_rare = env.time;
        }
        if kind == StuntKind::Ascension {
            self.ascension.started(env.time, &mut self.rng);
        }
        if mark && env.show.is_some() {
            link.freshness_mark(kind.freshness_id(), env.time);
        }
        self.history.push(kind);
        if self.history.len() > HISTORY {
            self.history.remove(0);
        }
        true
    }

    fn finish(&mut self, world: &mut dyn StuntWorld, intensity: f64) {
        if let Some(mut run) = self.run.take() {
            run.end(world);
        }
        self.active = false;
        self.weight = 0.0;
        self.fx_off = false;
        world.hold(false);
        // Un morceau intense enchaîne un peu plus vite.
        self.wait = self.rng.range(GAP.0, GAP.1) * (1.2 - 0.4 * intensity) * self.scale;
    }
}

/// L'instant du tirage : sur quoi une cascade peut s'aligner.
#[allow(clippy::struct_excessive_bools)] // Les quatre alignements possibles d'un instant de tirage.
#[derive(Debug, Clone, Copy)]
struct Moment {
    bar: bool,
    phrase: bool,
    drop: bool,
    overdue: bool,
}
