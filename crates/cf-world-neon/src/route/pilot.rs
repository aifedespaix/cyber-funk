//! Ce que le pilote et le plan de route se partagent (`route-plan.ts`) : objets vus, état publié par le volant et la
//! verticale, plan retenu, étages et leur chronométrage. Les constantes réglées à la main sont celles de Sillage.

use super::floors::FLOORS;

/// Demi-largeur de la route accessible à la voiture, en m.
pub const LIMIT: f64 = 4.6;
/// Distance (m) jusqu'où le volant et la verticale regardent : la même pour les deux, sans quoi leurs plans
/// divergeraient.
pub const LOOKAHEAD_PLAN: f64 = 110.0;
/// Vitesse de rattrapage latéral (1/s) de la voiture à pleine adhérence. Le vrai taux, publié à chaque image par la
/// scène ([`PilotHint::rate`]), baisse quand la voiture roule lentement (peu d'adhérence) ou très vite (sous-virage) :
/// le plan en prend une part, prudente pour la planification, plus proche du réel pour la commande.
pub const FULL_RATE: f64 = 5.0;
pub(super) const CTRL_SHARE: f64 = 0.88;
/// Plancher du taux : à l'arrêt presque complet, le plan ne doit pas diviser par zéro.
pub(super) const MIN_RATE: f64 = 0.8;
/// Distance minimale à garder d'un malus (contact à 1,5 m), en m.
pub const SAFE: f64 = 1.9;
/// Temps pour changer d'étage : un ordre toutes les 0,9 s, la caisse se pose ~0,55 s après le dernier.
pub(super) const FLOOR_STEP: f64 = 0.9;
pub(super) const FLOOR_SETTLE: f64 = 0.55;
/// Décollage : déploiement des roues puis montée au premier étage.
pub(super) const TAKEOFF: f64 = 2.2;
/// Marge (s) avec laquelle on lance un changement d'étage avant le dernier moment.
pub const DEPART_SLACK: f64 = 0.45;
/// Autant que d'objets que la route peut porter à la fois (`POOL` de `pickups.ts`) : plus n'a pas de sens.
pub const MAX_NODES: usize = 16;
/// Malus à fuir gardés dans un plan.
pub const MAX_DODGES: usize = 6;

/// Genre d'un objet de la route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickupKind {
    /// À ramasser.
    Bonus,
    /// À fuir : en spectacle, les voitures au sol en sont.
    Malus,
}

/// Un objet vu par le pilote (`PickupView` de `autopilot.ts`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickupView {
    /// Bonus ou malus.
    pub kind: PickupKind,
    /// Abscisse, m.
    pub x: f64,
    /// Profondeur (z absolu), m.
    pub z: f64,
    /// Hauteur (m) : 0 pour un objet posé sur la route, l'altitude d'un étage pour un objet en vol.
    pub y: f64,
}

/// État partagé entre le volant et la verticale : chacun publie ce que l'autre ne voit pas (`pilotHint`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PilotHint {
    /// Abscisse de la voiture, m.
    pub car_x: f64,
    /// Vitesse, m/s.
    pub speed: f64,
    /// Taux de rattrapage latéral réel de la voiture (1/s), selon sa vitesse.
    pub rate: f64,
    /// Étage visé par la verticale.
    pub floor: i32,
    /// Délai (s) avant que la verticale puisse donner un nouvel ordre.
    pub delay: f64,
    /// Position z de la dernière mise à jour de la verticale (`NaN` : jamais).
    pub floor_z: f64,
}

impl Default for PilotHint {
    fn default() -> Self {
        Self {
            car_x: 0.0,
            speed: 40.0,
            rate: FULL_RATE,
            floor: 1,
            delay: 0.0,
            floor_z: f64::NAN,
        }
    }
}

impl PilotHint {
    /// Taux de rattrapage utilisé par la commande du volant pour l'image en cours (`ctrlRate`).
    #[must_use]
    pub fn ctrl_rate(&self) -> f64 {
        self.rate.max(MIN_RATE) * CTRL_SHARE
    }
}

/// Le plan : le chemin retenu et les malus à fuir avant son premier bonus (`Route`). Les tableaux sont de taille fixe ;
/// seules les `count` (resp. `dodge_count`) premières cases comptent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    /// Nombre de bonus retenus ; 0 = rien à viser.
    pub count: usize,
    /// Pour chaque bonus du chemin : abscisse visée, m.
    pub aim: [f64; MAX_NODES],
    /// Instant d'arrivée, s.
    pub time: [f64; MAX_NODES],
    /// Étage (0 = sol).
    pub floor: [i8; MAX_NODES],
    /// Malus à fuir avant le premier bonus.
    pub dodge_count: usize,
    /// Abscisse du malus à fuir, m.
    pub dodge_x: [f64; MAX_DODGES],
    /// Instant où l'on en est quitte, s.
    pub dodge_time: [f64; MAX_DODGES],
}

impl Route {
    pub(super) const EMPTY: Self = Self {
        count: 0,
        aim: [0.0; MAX_NODES],
        time: [0.0; MAX_NODES],
        floor: [0; MAX_NODES],
        dodge_count: 0,
        dodge_x: [0.0; MAX_DODGES],
        dodge_time: [0.0; MAX_DODGES],
    };
}

/// Ce que le plan doit savoir de la voiture (`RouteInput`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RouteInput {
    /// Abscisse de la voiture, m.
    pub car_x: f64,
    /// Profondeur (z absolu) de la voiture, m.
    pub car_z: f64,
    /// Vitesse, m/s.
    pub speed: f64,
    /// Étage de départ : 0 = sol.
    pub floor: i32,
    /// Étage où la voiture se trouve vraiment (altitude) : après un ordre, elle met un moment à quitter l'ancien. Par
    /// défaut `floor`.
    pub physical: Option<i32>,
    /// Secondes avant que le prochain ordre d'étage puisse partir.
    pub delay: f64,
    /// Faux si la verticale n'est pas pilotée par le plan : seuls les objets de l'étage courant comptent.
    pub can_change: bool,
    /// Distance (m) jusqu'où l'on regarde.
    pub lookahead: f64,
}

/// Étage d'un objet d'après sa hauteur : 0 au sol, 1 à 6 en vol, -1 s'il n'est à aucun étage (`floorOf`).
#[must_use]
pub fn floor_of(y: f64) -> i32 {
    if y < 2.5 {
        return 0;
    }
    FLOORS
        .iter()
        .position(|&f| (f + 0.9 - y).abs() < 2.2)
        .map_or(-1, |i| i32::try_from(i + 1).unwrap_or(-1))
}

/// Étage le plus proche d'une altitude, en m (`floorAt`).
#[must_use]
pub fn floor_at(altitude: f64) -> i32 {
    if altitude < 1.5 {
        return 0;
    }
    let mut best = 1;
    for (i, floor) in FLOORS.iter().enumerate().skip(1) {
        if (floor - altitude).abs() < (FLOORS[best - 1] - altitude).abs() {
            best = i + 1;
        }
    }
    i32::try_from(best).unwrap_or(0)
}

/// Durée (s) pour aller de l'étage `from` à l'étage `to`, ordres compris (`floorTime`).
#[must_use]
pub fn floor_time(from: i32, to: i32) -> f64 {
    let n = (from - to).abs();
    if n == 0 {
        return 0.0;
    }
    if from == 0 {
        return TAKEOFF + f64::from(to - 1) * FLOOR_STEP;
    }
    f64::from(n - 1) * FLOOR_STEP + FLOOR_SETTLE
}

/// Étage que la verticale doit viser maintenant d'après le plan, ou `None` s'il n'y a rien à viser. Tant que le premier
/// bonus est sur l'étage courant, ou que le changement peut attendre, on reste où l'on est (`routeFloor`).
#[must_use]
pub fn route_floor(plan: &Route, current: i32) -> Option<i32> {
    if plan.count == 0 {
        return None;
    }
    let target = i32::from(plan.floor[0]);
    if target == current {
        return Some(current);
    }
    Some(if plan.time[0] - floor_time(current, target) > DEPART_SLACK {
        current
    } else {
        target
    })
}

/// Le malus (étage `g`, instant `tm`) est-il sur le chemin de la voiture entre (étage `fa`, instant `ta`) et (étage `fb`,
/// instant `tb`) ?
pub(super) fn occupies(fa: i32, ta: f64, fb: i32, tb: f64, g: i32, tm: f64) -> bool {
    if fa == fb {
        return g == fa;
    }
    let leave = ta.max(tb - floor_time(fa, fb) - DEPART_SLACK);
    if tm <= leave + 0.2 {
        return g == fa;
    }
    if tm >= tb - 0.3 {
        return g == fb;
    }
    g >= fa.min(fb) && g <= fa.max(fb)
}
