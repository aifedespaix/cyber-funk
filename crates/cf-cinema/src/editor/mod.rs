//! La mémoire éditoriale du réalisateur (`cinema/editor.ts`) : ce qui a été montré, quand, et ce que la rareté autorise.
//! Elle tire le plan suivant (rang pondéré, puis plan dans le rang) sans rien savoir des transitions.
//!
//! Les plans sont désignés par leur rang dans le catalogue ; les listes de Sillage indexées par id (bonus, respiration,
//! plans nerveux) restent des listes d'ids, comparées à [`ShotSpec::id`].
//!
//! # Sécurité du public
//!
//! Le plan `strobe` enchaîne des coupes à chaque temps. Il est **écarté de tout tirage** (routine, drop, occasion, ordre
//! de l'opérateur, plan de drop préparé) quand les flashs sont coupés ou en respiration : voir [`is_strobing`]. Le
//! plafond de cadence des coupes ([`crate::choreo::MAX_CUTS_PER_SECOND`]) s'y ajoute, pour tous les plans.

mod draw;
#[cfg(test)]
pub(crate) mod testkit;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use crate::choreo::ShowSide;
use crate::rng::CinemaRng;
use crate::screen_time::ScreenTime;
use crate::shot::ShotSpec;
use crate::types::ShotTier;
use crate::world::CinemaFrame;

/// Au-delà de cette occasion (0 → 1), le réalisateur bascule dès le prochain temps.
pub const OPPORTUNITY_THRESHOLD: f32 = 0.7;
/// Écart minimal (s) entre deux plans rares (ou épiques) : garde-fou de rareté.
pub const RARE_GAP: f64 = 75.0;
/// Écart minimal (s) entre deux plans épiques.
pub const EPIC_GAP: f64 = 240.0;
/// Aucun plan épique avant ce délai (s) depuis le départ du réalisateur.
pub const EPIC_NOT_BEFORE: f64 = 90.0;
/// Nombre de derniers plans écartés du tirage strict.
const RECENT: usize = 3;
/// Une montée garde son influence ce temps (s), un break celui-ci.
const BUILD_SECONDS: f64 = 14.0;
const BREAK_SECONDS: f64 = 25.0;
/// Plans trop nerveux pour le mode `calm` (mouvement réduit).
const CALM_BLOCK: &[&str] = &["strobe", "dutch", "flyby", "spiral", "vertigo", "wheel"];
/// Coups de pouce selon l'humeur du morceau.
const BUILD_BONUS: &[(&str, f32)] = &[
    ("vertigo", 3.0),
    ("crane", 3.0),
    ("spiral", 3.0),
    ("push-in", 3.0),
    ("dolly-zoom", 3.0),
];
const BREAK_BONUS: &[(&str, f32)] = &[
    ("god-view", 3.0),
    ("sky-tilt", 3.0),
    ("ad-glance", 2.5),
    ("moon-pass", 2.5),
    ("ringworld", 3.0),
    ("orbit", 1.5),
    ("side-track", 1.3),
    ("city-wide", 3.5),
];
/// Projecteur : les plans serrés et sombres passent moins souvent (poids), l'habitacle est plafonné à une part du
/// temps.
const PROJECTOR_WEIGHT: &[(&str, f32)] = &[("wheel", 0.35), ("bumper", 0.5), ("cockpit", 0.5)];
/// Part d'écran maximale de l'habitacle en spectacle (0 → 1).
pub const COCKPIT_CAP: f64 = 0.08;
/// Respiration : plans lents et larges seulement.
const BREATH_SHOTS: &[&str] = &[
    "chase",
    "orbit",
    "side-track",
    "crane",
    "sky-tilt",
    "god-view",
    "top-down",
    "city-wide",
    "ad-glance",
    "landmark-reveal",
    "ride-along",
    "moon-pass",
    "ringworld",
    "dj-sign",
    "push-in",
];
/// Coups de pouce d'un drop.
const DROP_BONUS: &[(&str, f32)] = &[
    ("chase", 1.6),
    ("wheel", 2.0),
    ("side-track", 1.5),
    ("bumper", 1.5),
    ("flyby", 1.5),
    ("strobe", 2.0),
    ("orbit", 1.2),
];
/// Plans dont le rythme coupe l'image à chaque temps : coupés quand les flashs le sont.
const STROBING: &[&str] = &["strobe"];

/// Pourquoi on tire un plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PickMode {
    /// Premier plan au démarrage (voiture, commun).
    Start,
    /// Coupe de routine (grille, phrase, durée max).
    Normal,
    /// Le drop part : plan énergique.
    Drop,
    /// Le plan précédent s'est terminé de lui-même.
    Ended,
}

/// Ce plan enchaîne-t-il des coupes à chaque temps (`strobe`) ? Ces plans n'ont pas leur place quand les flashs sont
/// coupés ou en respiration (WCAG 2.3.1), quelle que soit la demande, ordre de l'opérateur compris.
#[must_use]
pub fn is_strobing(id: &str) -> bool {
    STROBING.contains(&id)
}

fn lookup(table: &[(&str, f32)], id: &str) -> Option<f32> {
    table.iter().find(|(k, _)| *k == id).map(|&(_, v)| v)
}

/// La mémoire éditoriale. Tout est dimensionné à la construction : un tirage n'alloue pas.
#[derive(Debug, Clone)]
pub struct Editor {
    /// Dernière montée (horloge du monde).
    pub build_at: f64,
    /// Dernier break (horloge du monde).
    pub break_at: f64,
    /// Le dernier plan montré n'était pas un plan voiture : le suivant y revient.
    pub after_special: bool,
    origin: f64,
    recent: Vec<usize>,
    last_used: Vec<f64>,
    pool: Vec<(usize, f32)>,
    last_rare: f64,
    last_epic: f64,
    breather: u32,
    car_streak: u32,
    streak_goal: u32,
    screen: ScreenTime,
    by_id: HashMap<&'static str, usize>,
    /// `shot:<id>` de chaque plan (clés de la fraîcheur commune), écrits une fois.
    keys: Vec<String>,
    cockpit: Option<usize>,
    calm: bool,
    rng: CinemaRng,
}

impl Editor {
    /// Éditeur d'un catalogue. `calm` : mode calme (plans nerveux bloqués).
    #[must_use]
    pub fn new(catalog: &[Box<dyn ShotSpec>], calm: bool, rng: CinemaRng) -> Self {
        let n = catalog.len();
        Self {
            build_at: f64::NEG_INFINITY,
            break_at: f64::NEG_INFINITY,
            after_special: false,
            origin: 0.0,
            recent: Vec::with_capacity(RECENT + 1),
            last_used: vec![f64::NEG_INFINITY; n],
            pool: Vec::with_capacity(n),
            last_rare: f64::NEG_INFINITY,
            last_epic: f64::NEG_INFINITY,
            breather: 0,
            car_streak: 0,
            streak_goal: 3,
            screen: ScreenTime::new(n),
            by_id: catalog.iter().enumerate().map(|(i, s)| (s.id(), i)).collect(),
            keys: catalog.iter().map(|s| format!("shot:{}", s.id())).collect(),
            cockpit: catalog.iter().position(|s| s.id() == "cockpit"),
            calm,
            rng,
        }
    }

    /// Oublie l'historique (démarrage du réalisateur) et déclare le catalogue à la fraîcheur commune, pour que la
    /// moyenne d'usage se calcule sur tous les plans.
    pub fn reset(&mut self, now: f64, show: &mut dyn ShowSide) {
        self.recent.clear();
        self.last_used.fill(f64::NEG_INFINITY);
        self.origin = now;
        self.last_rare = f64::NEG_INFINITY;
        self.last_epic = f64::NEG_INFINITY;
        self.build_at = f64::NEG_INFINITY;
        self.break_at = f64::NEG_INFINITY;
        self.breather = 0;
        self.car_streak = 0;
        self.streak_goal = 3;
        self.after_special = false;
        self.screen.reset();
        show.freshness_mut().known(&self.keys);
    }

    /// Mode calme (moins de coupes).
    #[must_use]
    pub fn calm(&self) -> bool {
        self.calm
    }

    /// Part d'écran du plan `index`, plan en cours compris.
    #[must_use]
    pub fn share(&self, index: usize, now: f64) -> f64 {
        self.screen.share(index, now)
    }

    /// Le plan du catalogue portant cet id.
    #[must_use]
    pub fn find(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    /// Ce plan est-il jouable maintenant ? Cooldown ignoré ; `ignore_gates` : ordre de l'opérateur, hors verrous du
    /// look (le garde-fou des flashs, lui, ne s'ignore jamais).
    #[must_use]
    pub fn can_use(
        &self,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        catalog: &[Box<dyn ShotSpec>],
        index: usize,
        ignore_gates: bool,
    ) -> bool {
        catalog
            .get(index)
            .is_some_and(|spec| self.allowed(frame, show, spec.as_ref(), index, 2, ignore_gates))
    }

    /// Tire un plan parmi ces ids (poids × fraîcheur), hors le plan courant ; `None` si aucun n'est jouable.
    pub fn pick_among(
        &mut self,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        catalog: &[Box<dyn ShotSpec>],
        ids: &[&str],
        current: Option<usize>,
    ) -> Option<usize> {
        let mut pool = std::mem::take(&mut self.pool);
        pool.clear();
        let mut sum = 0.0;
        for id in ids {
            let Some(index) = self.find(id) else { continue };
            let Some(spec) = catalog.get(index) else { continue };
            if Some(index) == current || !self.allowed(frame, show, spec.as_ref(), index, 1, false) {
                continue;
            }
            let w = spec.weight().max(0.0) * self.freshness(frame, show, index);
            pool.push((index, w));
            sum += w;
        }
        let picked = if pool.is_empty() {
            None
        } else {
            let mut roll = self.rng.next_f32() * sum;
            let mut found = pool.last().map(|&(i, _)| i);
            for &(index, w) in &pool {
                roll -= w;
                if roll <= 0.0 {
                    found = Some(index);
                    break;
                }
            }
            found
        };
        self.pool = pool;
        picked
    }

    /// Coefficient de fraîcheur commune du plan (1 hors spectacle).
    fn freshness(&self, frame: &CinemaFrame<'_>, show: &dyn ShowSide, index: usize) -> f32 {
        if !frame.show.active {
            return 1.0;
        }
        self.keys
            .get(index)
            .map_or(1.0, |key| show.freshness().weight(key, frame.time) as f32)
    }

    /// Verrous du spectacle : rangs et ids permis par le look, plans lents seulement en respiration.
    fn gated(frame: &CinemaFrame<'_>, show: &dyn ShowSide, spec: &dyn ShotSpec) -> bool {
        if !frame.show.active {
            return true;
        }
        if let Some(gates) = show.gates() {
            let tier = spec.tier().as_str();
            let tier_ok = gates.shot_tiers.iter().any(|t| t.as_str() == tier);
            if !tier_ok || gates.shot_block.iter().any(|b| b.as_str() == spec.id()) {
                return false;
            }
        }
        !frame.show.breathing || BREATH_SHOTS.contains(&spec.id())
    }

    /// Une occasion à saisir (figure, astre qui tombe, poursuite) : le plan qui la signale le plus fort, ou `None`.
    /// `reactive` : seuls les plans qui jouent une animation de la voiture.
    #[must_use]
    pub fn best_opportunity(
        &self,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        catalog: &[Box<dyn ShotSpec>],
        current: Option<usize>,
        reactive: bool,
    ) -> Option<usize> {
        let mut best = None;
        let mut top = OPPORTUNITY_THRESHOLD;
        for (index, spec) in catalog.iter().enumerate() {
            if Some(index) == current
                || (reactive && !spec.reactive())
                || !self.allowed(frame, show, spec.as_ref(), index, 1, false)
            {
                continue;
            }
            let value = spec.opportunity(frame);
            if value > top {
                top = value;
                best = Some(index);
            }
        }
        best
    }

    /// Plus on descend de niveau (0 → 2), plus les règles se relâchent : le réalisateur ne reste jamais sans plan.
    /// Niveau 0 : tout (derniers plans, respiration après un rare, retour voiture) ; 1 : disponible, énergie, cooldown ;
    /// 2 : disponible seulement (cooldown ignoré, plan courant permis : un catalogue d'un seul plan le relance).
    pub fn pick(
        &mut self,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        catalog: &[Box<dyn ShotSpec>],
        mode: PickMode,
        current: Option<usize>,
    ) -> Option<usize> {
        (0..3).find_map(|level| self.draw(frame, show, catalog, mode, level, current))
    }

    /// Mémoire : derniers plans, cooldowns, rareté, respiration après un rare, fraîcheur commune. `counted` : le chef
    /// du spectacle a déjà compté ce plan (plan de drop choisi par `DropConductor`).
    pub fn register(
        &mut self,
        index: usize,
        spec: &dyn ShotSpec,
        frame: &CinemaFrame<'_>,
        show: &mut dyn ShowSide,
        counted: bool,
    ) {
        let now = frame.time;
        self.screen.switch_to(index, now);
        if frame.show.active
            && !counted
            && let Some(key) = self.keys.get(index)
        {
            show.freshness_mut().mark(key, now);
        }
        self.recent.push(index);
        if self.recent.len() > RECENT {
            self.recent.remove(0);
        }
        if let Some(at) = self.last_used.get_mut(index) {
            *at = now;
        }
        let tier = spec.tier();
        if matches!(tier, ShotTier::Rare | ShotTier::Epic) {
            self.last_rare = now;
            if tier == ShotTier::Epic {
                self.last_epic = now;
            }
            self.breather = if tier == ShotTier::Epic {
                3
            } else {
                2 + (self.rng.next_f32() * 2.0) as u32
            };
        } else if self.breather > 0 {
            self.breather -= 1;
        }
        if spec.follows_car() {
            self.car_streak += 1;
        } else {
            self.car_streak = 0;
            self.streak_goal = 3 + (self.rng.next_f32() * 3.0) as u32;
        }
        self.after_special = !spec.follows_car();
    }

    fn allowed(
        &self,
        frame: &CinemaFrame<'_>,
        show: &dyn ShowSide,
        spec: &dyn ShotSpec,
        index: usize,
        level: u8,
        ignore_gates: bool,
    ) -> bool {
        let now = frame.time;
        // Sûreté d'abord, et sans exception : ni verrou de look ni ordre de l'opérateur n'y change rien.
        if is_strobing(spec.id()) && (!frame.show.flashes || frame.show.breathing) {
            return false;
        }
        if !ignore_gates && !Self::gated(frame, show, spec) {
            return false;
        }
        if self.calm && CALM_BLOCK.contains(&spec.id()) {
            return false;
        }
        match spec.tier() {
            ShotTier::Rare if now - self.last_rare < RARE_GAP => return false,
            ShotTier::Epic
                if now - self.origin < EPIC_NOT_BEFORE
                    || now - self.last_epic < EPIC_GAP
                    || now - self.last_rare < RARE_GAP =>
            {
                return false;
            }
            _ => {}
        }
        if level < 2
            && self
                .last_used
                .get(index)
                .is_some_and(|&at| at + f64::from(spec.cooldown()) > now)
        {
            return false;
        }
        spec.available(frame)
    }
}
