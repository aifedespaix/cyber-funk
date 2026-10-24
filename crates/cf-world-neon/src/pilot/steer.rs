//! Le volant du pilote automatique (`autopilot.ts`) : parmi toutes les positions latérales possibles, il retient la moins
//! coûteuse. Une voiture au sol devant lui rend une position dangereuse ; une position préférée (la danse) ou la ligne de
//! course du spectacle départage le reste.
//!
//! Sillage suit en plus le plan de route (`follow`) quand des bonus sont à venir. Ce chemin est celui du mode Jeu : il n'y
//! a ni bonus ni malus à ramasser en cinéma, seules les voitures au sol comptent (voir la section « Écarts » de la PR).

use super::input::ShowInput;
use super::race_line::RaceLine;
use crate::route::{LIMIT, PickupView};

/// Pas de la recherche de la meilleure position latérale, en m.
const STEP: f64 = 0.2;
/// Nombre de positions testées : de −`LIMIT` à `LIMIT` par pas de `STEP` (23 pas de chaque côté).
const POSITIONS: usize = 47;
/// Écart d'altitude (m) au-delà duquel un objet n'est ni attrapé ni évité.
pub const PICKUP_HEIGHT: f64 = 2.2;
/// Part de l'inertie (position visée et position réelle) gardée quand le pilote suit la ligne de course.
///
/// Écart avec Sillage (voir « Écarts » de la PR) : là-bas l'inertie pèse `0,12 + 0,03` contre `0,1` pour la ligne ; une
/// voiture visant déjà sa position réelle (donc toujours, sans obstacle) y reste, et la ligne ne fait que départager.
/// La ligne est déjà lissée par son propre ressort, plus vif que le volant jamais : l'inertie y est redondante. Mettre
/// 1 pour retrouver les poids de Sillage.
const LINE_INERTIA: f64 = 0.1;
/// Marge autour d'une voiture au sol à dépasser, et distance (équivalente, voir `GroundTraffic.views`) où l'on commence à
/// s'en écarter.
const GROUND_DANGER: f64 = 2.7;
const GROUND_LOOK: f64 = 150.0;

/// Ce que le volant lit à chaque image.
#[derive(Debug, Clone, Copy)]
pub struct SteerInput<'a> {
    /// Abscisse de la voiture, m.
    pub car_x: f64,
    /// Profondeur de la voiture (z absolu), m.
    pub car_z: f64,
    /// Altitude de la voiture, m : en vol, les voitures au sol sont hors de portée.
    pub altitude: f64,
    /// Voitures au sol devant la voiture.
    pub ground: &'a [PickupView],
    /// Abscisse préférée (la danse), en m, ou `None`.
    pub preferred: Option<f64>,
    /// Durée de l'image, s.
    pub dt: f64,
    /// Taux de rattrapage latéral réel de la voiture (1/s) : la ligne de course n'est jamais plus vive.
    pub lateral_rate: f64,
    /// Temps musical (horloge de danse).
    pub beat: f64,
    /// Énergie du morceau, 0 → 1.
    pub energy: f64,
    /// Le spectacle de l'image, ou `None`.
    pub show: Option<&'a ShowInput>,
}

/// Pilote automatique du volant.
#[derive(Debug, Clone)]
pub struct Autopilot {
    target: f64,
    line: RaceLine,
}

impl Autopilot {
    /// Un pilote dont la ligne de course tire ses files de la graine `seed`.
    #[must_use]
    pub fn new(seed: f64) -> Self {
        Self {
            target: 0.0,
            line: RaceLine::new(seed),
        }
    }

    /// Repart de `x` sans à-coup (changement de voiture, téléportation).
    pub fn reset(&mut self, x: f64) {
        self.target = x;
        self.line.reset(x);
    }

    /// Renvoie la direction voulue, entre -1 et 1.
    pub fn steer(&mut self, s: &SteerInput<'_>) -> f64 {
        // Spectacle : sans rien à ramasser on suit la ligne de course.
        let line = match (s.show, s.preferred) {
            (Some(show), None) => {
                let energy = s.energy * 0.7 + show.heat * 0.3;
                Some(self.line.update(
                    s.dt,
                    s.beat,
                    if show.breathing { energy * 0.5 } else { energy },
                    s.lateral_rate,
                ))
            }
            _ => {
                self.line.reset(s.car_x);
                None
            }
        };
        let mut best = self.target;
        let mut best_cost = f64::INFINITY;
        for i in 0..POSITIONS {
            let x = -LIMIT + i as f64 * STEP;
            let cost = self.cost(x, s, line);
            if cost < best_cost {
                best_cost = cost;
                best = x;
            }
        }
        self.target += (best - self.target) * (1.0 - (-s.dt * 12.0).exp());
        (self.target / LIMIT).clamp(-1.0, 1.0)
    }

    fn cost(&self, x: f64, s: &SteerInput<'_>, line: Option<f64>) -> f64 {
        let inertia = if line.is_some() { LINE_INERTIA } else { 1.0 };
        let mut cost = ((x - self.target).abs() * 0.12 + (x - s.car_x).abs() * 0.03) * inertia;
        cost += match (line, s.preferred) {
            (Some(line), _) => (x - line).abs() * 0.1,
            (None, None) => x.abs() * 0.03,
            (None, Some(preferred)) => (x - preferred).abs() * 0.1,
        };
        if s.altitude.abs() > PICKUP_HEIGHT {
            return cost;
        }
        for g in s.ground {
            let ahead = s.car_z - g.z;
            if !(-1.0..=GROUND_LOOK).contains(&ahead) {
                continue;
            }
            let w = 0.3 + 0.7 * (1.0 - ahead.max(0.0) / GROUND_LOOK);
            let gap = (x - g.x).abs();
            if gap < GROUND_DANGER {
                cost += 14.0 * w;
            } else if gap < GROUND_DANGER + 1.0 {
                cost += (GROUND_DANGER + 1.0 - gap) * 2.0 * w;
            }
        }
        cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::PickupKind;

    fn input<'a>(ground: &'a [PickupView], car_x: f64, preferred: Option<f64>) -> SteerInput<'a> {
        SteerInput {
            car_x,
            car_z: -1000.0,
            altitude: 0.0,
            ground,
            preferred,
            dt: 1.0 / 60.0,
            lateral_rate: 5.0,
            beat: 0.0,
            energy: 0.4,
            show: None,
        }
    }

    fn malus(x: f64, z: f64) -> PickupView {
        PickupView {
            kind: PickupKind::Malus,
            x,
            z,
            y: 0.0,
        }
    }

    #[test]
    fn tient_la_position_choisie() {
        // Comme dans Sillage, la position préférée ne fait que départager : l'inertie (position visée, position réelle)
        // pèse plus qu'elle. Une voiture déjà sur la position préférée n'en bouge pas.
        let mut pilot = Autopilot::new(1.0);
        pilot.reset(3.0);
        let mut steer = 0.0;
        for _ in 0..300 {
            steer = pilot.steer(&input(&[], 3.0, Some(3.0)));
        }
        assert!((steer * LIMIT - 3.0).abs() < 0.3, "{steer}");
    }

    #[test]
    fn contourne_une_voiture_au_sol() {
        let mut pilot = Autopilot::new(1.0);
        let blocker = [malus(0.0, -1060.0)];
        let mut steer = 0.0;
        for _ in 0..300 {
            steer = pilot.steer(&input(&blocker, 0.0, None));
        }
        assert!(steer.abs() * LIMIT > 2.0, "il s'écarte : {steer}");
    }

    #[test]
    fn en_vol_les_voitures_au_sol_sont_ignorees() {
        let blocker = [malus(0.0, -1060.0)];
        let mut ground = Autopilot::new(1.0);
        let mut air = Autopilot::new(1.0);
        let (mut a, mut b) = (0.0, 0.0);
        for _ in 0..300 {
            a = ground.steer(&input(&blocker, 0.0, None));
            let mut s = input(&blocker, 0.0, None);
            s.altitude = 8.0;
            b = air.steer(&s);
        }
        assert!(a.abs() > b.abs() + 0.2, "{a} {b}");
    }

    #[test]
    fn sortie_toujours_dans_moins_un_un() {
        let mut pilot = Autopilot::new(5.0);
        let show = ShowInput {
            level: 5,
            heat: 1.0,
            breathing: false,
            drop_now: false,
            time_scale: 1.0,
            gates: super::super::input::PilotGates {
                cruise: 1.0,
                flight_rate: 1.0,
                max_floor: 6,
                flight_on_drop_only: false,
                tricks: true,
            },
        };
        for i in 0..2000 {
            let mut s = input(&[], 0.0, None);
            s.show = Some(&show);
            s.beat = f64::from(i) * 0.03;
            s.energy = 1.0;
            let steer = pilot.steer(&s);
            assert!((-1.0..=1.0).contains(&steer));
        }
    }
}
