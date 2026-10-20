//! Plan de route du pilote automatique (`route-plan.ts`) : parmi les objets à venir, quel chemin (abscisse + étage,
//! dans le temps) en ramasse le plus sans toucher de malus.
//!
//! Le volant et la verticale du pilote (J5-06) lisent le même plan, calculé une seule fois par image (cache) : ils ne
//! se contredisent jamais. Les trois règles de Sillage sont gardées :
//!
//! - les objets arrivent à `vitesse × SCROLL` m/s ;
//! - un bonus B est atteignable depuis A si la voiture a le temps de rejoindre sa ligne (retard du volant, du premier
//!   ordre) et de changer d'étage (~0,9 s par étage) ;
//! - on cherche le meilleur chemin de ce graphe orienté (programmation dynamique) ; un malus inévitable sur un trajet
//!   coûte très cher. Les constantes sont volontairement prudentes.
//!
//! Écarts avec Sillage, tous voulus :
//!
//! - l'état partagé `pilotHint`, qui était un objet global du module, est le champ [`RoutePlanner::hint`] : un seul
//!   propriétaire, passé explicitement ;
//! - le cache ne compare plus l'identité du tableau d'objets (`pickups === cached`) : l'appelant appelle
//!   [`RoutePlanner::invalidate`] une fois par image, quand les objets ont changé ;
//! - aucune allocation : tous les tampons sont des tableaux de taille fixe, comme les `Float64Array` de Sillage.
//!
//! Cette logique est pure (sans Bevy) : le système Bevy qui la nourrit est celui du pilote, en J5-06.

use bevy::prelude::Resource;

use super::pilot::{
    FULL_RATE, LIMIT, MAX_DODGES, MAX_NODES, MIN_RATE, PickupKind, PickupView, PilotHint, Route, RouteInput, SAFE,
    floor_of, floor_time, occupies,
};

/// Défilement de la route par rapport à la vitesse (voir `SCROLL` dans la scène).
const SCROLL: f64 = 0.85;
const PLAN_SHARE: f64 = 0.72;
/// Écart latéral toléré à l'arrivée sur un bonus (le contact réel est à 1,5 m).
const ARRIVE_TOL: f64 = 0.8;
/// Décalage maximal de visée pour fuir un malus de la même rangée, en m.
const SHIFT_MAX: f64 = 0.6;
const MALUS_PENALTY: f64 = 5.0;
const SAME_TIME: f64 = 0.06;
/// Demi-longueur de contact le long de la route (`HIT_DEPTH` de `pickups.ts`, 2,2 m, plus la marge d'une image).
const CONTACT_DEPTH: f64 = 3.0;
/// Durée (s) pendant laquelle la voiture occupe encore l'ancien étage après un ordre de changement.
const LEAVE_TIME: f64 = 0.4;

/// Les entrées du dernier plan, comparées une à une (bit à bit : `NaN` égal à lui-même, sans `float_cmp`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CacheKey {
    car_z: u64,
    floor: i32,
    physical: i32,
    speed: u64,
    delay: u64,
    rate: u64,
    lookahead: u64,
    can_change: bool,
}

/// Le planificateur : les tampons du plan, le [`PilotHint`] et le cache de l'image. Ressource du monde : le pilote
/// (J5-06) l'écrit et le lit, un seul propriétaire.
#[derive(Resource, Debug, Clone)]
pub struct RoutePlanner {
    /// État publié par le volant et la verticale.
    pub hint: PilotHint,
    plan_rate: f64,
    node_x: [f64; MAX_NODES],
    node_aim: [f64; MAX_NODES],
    node_t: [f64; MAX_NODES],
    node_f: [i32; MAX_NODES],
    node_value: [f64; MAX_NODES],
    best: [f64; MAX_NODES],
    prev: [i8; MAX_NODES],
    path: [usize; MAX_NODES],
    malus_x: [f64; MAX_NODES],
    malus_t: [f64; MAX_NODES],
    malus_f: [i32; MAX_NODES],
    nodes: usize,
    maluses: usize,
    /// Demi-durée (s) pendant laquelle un objet est « en contact » : sa zone fait ±2,2 m le long de la route.
    window_time: f64,
    physical_floor: i32,
    route: Route,
    cached: Option<CacheKey>,
}

impl Default for RoutePlanner {
    fn default() -> Self {
        Self {
            hint: PilotHint::default(),
            plan_rate: FULL_RATE * PLAN_SHARE,
            node_x: [0.0; MAX_NODES],
            node_aim: [0.0; MAX_NODES],
            node_t: [0.0; MAX_NODES],
            node_f: [0; MAX_NODES],
            node_value: [0.0; MAX_NODES],
            best: [0.0; MAX_NODES],
            prev: [-1; MAX_NODES],
            path: [0; MAX_NODES],
            malus_x: [0.0; MAX_NODES],
            malus_t: [0.0; MAX_NODES],
            malus_f: [0; MAX_NODES],
            nodes: 0,
            maluses: 0,
            window_time: 0.0,
            physical_floor: 0,
            route: Route::EMPTY,
            cached: None,
        }
    }
}

impl RoutePlanner {
    /// Oublie le dernier plan : les objets ont changé (nouvelle image).
    pub fn invalidate(&mut self) {
        self.cached = None;
    }

    /// Plan de route à cet instant. Le plan renvoyé est réutilisé : à lire dans l'image, sans le conserver.
    pub fn plan(&mut self, pickups: &[PickupView], input: &RouteInput) -> &Route {
        self.physical_floor = input.physical.unwrap_or(input.floor);
        let rate = self.hint.rate.max(MIN_RATE);
        let key = CacheKey {
            car_z: input.car_z.to_bits(),
            floor: input.floor,
            physical: self.physical_floor,
            speed: input.speed.to_bits(),
            delay: input.delay.to_bits(),
            rate: rate.to_bits(),
            lookahead: input.lookahead.to_bits(),
            can_change: input.can_change,
        };
        // Même image, mêmes données : le volant et la verticale partagent le résultat. Une valeur `NaN` ne sert jamais
        // le cache (en JavaScript `NaN === NaN` est faux).
        if self.cached == Some(key) && !input.car_z.is_nan() {
            return &self.route;
        }
        self.cached = Some(key);
        self.plan_rate = rate * PLAN_SHARE;
        self.route.count = 0;
        self.route.dodge_count = 0;
        self.collect(pickups, input);
        if self.nodes == 0 {
            return &self.route;
        }
        self.search(input);
        &self.route
    }

    /// Programmation dynamique sur les bonus triés par instant, puis chemin retenu et malus à fuir.
    fn search(&mut self, input: &RouteInput) {
        let mut end: Option<usize> = None;
        let mut end_value = 0.05;
        for j in 0..self.nodes {
            let mut top = self.edge(input.car_x, 0.0, input.floor, j, input.delay, true);
            self.prev[j] = -1;
            for i in 0..j {
                if !self.best[i].is_finite() || self.node_t[i] >= self.node_t[j] - SAME_TIME {
                    continue;
                }
                let total = self.best[i] + self.edge(self.node_aim[i], self.node_t[i], self.node_f[i], j, 0.0, false);
                if total > top {
                    top = total;
                    self.prev[j] = i as i8;
                }
            }
            self.best[j] = top;
            if top > end_value {
                end_value = top;
                end = Some(j);
            }
        }
        let Some(end) = end else { return };

        // Chemin de la fin au début, puis remis dans l'ordre.
        let mut n = 0;
        let mut j = Some(end);
        while let Some(node) = j {
            self.path[n] = node;
            n += 1;
            j = usize::try_from(self.prev[node]).ok();
        }
        self.route.count = n;
        for k in 0..n {
            let node = self.path[n - 1 - k];
            self.route.aim[k] = self.node_aim[node];
            self.route.time[k] = self.node_t[node];
            self.route.floor[k] = self.node_f[node] as i8;
        }
        // Malus à fuir avant le premier bonus, sur la rangée où la voiture se trouvera.
        let first_t = self.route.time[0];
        let first_f = i32::from(self.route.floor[0]);
        for m in 0..self.maluses {
            if self.route.dodge_count >= MAX_DODGES {
                break;
            }
            let tm = self.malus_t[m];
            let clear = tm + self.window_time;
            let here = occupies(input.floor, 0.0, first_f, first_t, self.malus_f[m], tm.max(0.0))
                || self.lingers(self.malus_f[m], tm);
            if clear <= 0.0 || tm >= first_t - SAME_TIME || !here {
                continue;
            }
            self.route.dodge_x[self.route.dodge_count] = self.malus_x[m];
            self.route.dodge_time[self.route.dodge_count] = clear;
            self.route.dodge_count += 1;
        }
    }

    /// Le malus est-il sur l'étage que la voiture est en train de quitter ?
    fn lingers(&self, g: i32, tm: f64) -> bool {
        g == self.physical_floor && tm < LEAVE_TIME
    }

    /// Existe-t-il une abscisse à l'instant `tm` qui garde ses distances au malus, en partant de (ax, at) et en rejoignant
    /// bx à `tb` ?
    fn avoidable(&self, ax: f64, at: f64, tm: f64, bx: f64, tb: f64, xm: f64) -> bool {
        let forward = 1.0 - (-self.plan_rate * (tm - at)).exp();
        let half = ARRIVE_TOL * (self.plan_rate * (tb - tm)).exp();
        let lo = (ax - (ax + LIMIT) * forward).max(bx - half).max(-LIMIT);
        let hi = (ax + (LIMIT - ax) * forward).min(bx + half).min(LIMIT);
        if hi < lo {
            return false;
        }
        lo <= xm - SAFE || hi >= xm + SAFE
    }

    /// Valeur du trajet (ax, at, af) → bonus `j`, ou -∞ s'il est hors de portée. `delay` ne compte que pour le départ.
    fn edge(&self, ax: f64, at: f64, af: i32, j: usize, delay: f64, first: bool) -> f64 {
        let tb = self.node_t[j];
        let fb = self.node_f[j];
        let bx = self.node_aim[j];
        let dt = tb - at;
        let change = if af == fb {
            0.0
        } else {
            floor_time(af, fb) + if first { delay } else { 0.0 }
        };
        if dt < change {
            return f64::NEG_INFINITY;
        }
        let gap = (bx - ax).abs();
        if gap * (-self.plan_rate * dt).exp() > ARRIVE_TOL {
            return f64::NEG_INFINITY;
        }
        let mut value = self.node_value[j] - 0.02 * gap - 0.03 * f64::from((af - fb).abs());
        for m in 0..self.maluses {
            let tm = self.malus_t[m];
            // Un malus ne se quitte qu'une fois sa zone de contact (±2,2 m) dépassée.
            let clear = tm + self.window_time;
            if clear <= 0.0 || (clear <= at + SAME_TIME && !first) {
                continue;
            }
            if tm > tb + SAME_TIME {
                continue;
            }
            let on_path = occupies(af, at, fb, tb, self.malus_f[m], tm.max(0.0));
            if !(on_path || (first && self.lingers(self.malus_f[m], tm))) {
                continue;
            }
            if !self.avoidable(ax, at, clear.min(tb), bx, tb, self.malus_x[m]) {
                value -= MALUS_PENALTY;
            }
        }
        value
    }

    /// Trie les bonus par instant d'arrivée, range les malus, et fixe visée et valeur de chaque bonus.
    fn collect(&mut self, pickups: &[PickupView], input: &RouteInput) {
        self.nodes = 0;
        self.maluses = 0;
        let v = input.speed.max(6.0) * SCROLL;
        self.window_time = CONTACT_DEPTH / v + 0.02;
        for p in pickups {
            let ahead = input.car_z - p.z;
            // Un malus qu'on vient de croiser touche encore tant que sa zone n'est pas dépassée ; un bonus, lui, ne se
            // ramasse plus.
            let behind_limit = if p.kind == PickupKind::Malus {
                -CONTACT_DEPTH
            } else {
                0.0
            };
            if ahead > input.lookahead || ahead < behind_limit {
                continue;
            }
            let floor = floor_of(p.y);
            if floor < 0 || (!input.can_change && floor != input.floor) {
                continue;
            }
            let t = ahead / v;
            if p.kind == PickupKind::Malus {
                if self.maluses == MAX_NODES {
                    continue;
                }
                self.malus_x[self.maluses] = p.x;
                self.malus_t[self.maluses] = t;
                self.malus_f[self.maluses] = floor;
                self.maluses += 1;
                continue;
            }
            if self.nodes == MAX_NODES {
                continue;
            }
            // Insertion triée par instant d'arrivée.
            let mut i = self.nodes;
            self.nodes += 1;
            while i > 0 && self.node_t[i - 1] > t {
                self.node_x[i] = self.node_x[i - 1];
                self.node_t[i] = self.node_t[i - 1];
                self.node_f[i] = self.node_f[i - 1];
                i -= 1;
            }
            self.node_x[i] = p.x;
            self.node_t[i] = t;
            self.node_f[i] = floor;
        }
        for j in 0..self.nodes {
            let mut aim = self.node_x[j];
            // Un malus de la même rangée : on vise le bord du bonus le plus éloigné de lui.
            for m in 0..self.maluses {
                if self.malus_f[m] != self.node_f[j] || (self.malus_t[m] - self.node_t[j]).abs() > SAME_TIME {
                    continue;
                }
                let gap = aim - self.malus_x[m];
                if gap.abs() >= SAFE + 0.15 {
                    continue;
                }
                let side = if gap == 0.0 {
                    if aim > 0.0 { -1.0 } else { 1.0 }
                } else {
                    gap.signum()
                };
                aim += side * SHIFT_MAX.min(SAFE + 0.15 - gap.abs());
            }
            self.node_aim[j] = aim.clamp(-LIMIT, LIMIT);
            // Légère préférence pour les plus proches : à gain égal, on prend d'abord ce qui arrive.
            self.node_value[j] = 1.0 + 0.15 * (1.0 - (self.node_t[j] / 6.0).min(1.0));
        }
    }
}

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;
