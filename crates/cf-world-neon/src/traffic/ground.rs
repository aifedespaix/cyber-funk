//! Trafic au sol à dépasser (`ground.ts`), mode spectacle seulement. De petits véhicules roulent plus lentement que la
//! voiture sur trois files ; ce sont des malus virtuels pour le pilote : pas de points, pas de contact, il les évite
//! avec la logique habituelle ([`GroundTraffic::views`]). Quand une cascade, une structure ou un vol prend la route, ou
//! que le niveau ne le veut plus, ils s'effacent en douceur ; un véhicule gêné par la voiture s'écarte un peu de lui-même.
//! Les véhicules sont ceux de la flotte : ce sont aussi des sujets filmables pour la caméra.

use bevy::math::{DVec3, Vec3};

use super::air_lanes::range;
use super::fleet::{CraftId, Fleet};
use super::frame::TrafficFrame;
use super::motion::Motion;
use crate::pilot::rand::Rand;
use crate::route::{PickupKind, PickupView};

/// Abscisses des files (m) : assez écartées pour que la voiture double entre deux sans les toucher.
const LANES: [f32; 3] = [-3.1, 0.0, 3.1];
/// Part de la vitesse de la voiture que roule chaque file : toujours moins que 1, donc toujours dépassées.
const RATIOS: [f32; 3] = [0.7, 0.58, 0.65];
/// Véhicules au sol au plus (à pleine densité).
pub const MAX_CARS: usize = 5;
const HOVER_M: f32 = 0.32;
const BASE_SCALE: f32 = 1.9;
/// Distance (m) devant la voiture où naît un véhicule, et marge entre deux véhicules d'une même file à la naissance.
const SPAWN_MIN_M: f64 = 150.0;
const SPAWN_MAX_M: f64 = 215.0;
const SPAWN_GAP_M: f64 = 45.0;
/// Demi-longueur d'un véhicule : un malus ne disparaît pas tant que la voiture n'a pas dépassé l'arrière.
const REAR_M: f64 = 3.5;
/// Écart latéral minimal visé au moment de doubler, et décalage maximal qu'un véhicule s'accorde pour s'écarter.
const CLEAR_M: f32 = 2.4;
const YIELD_M: f32 = 0.7;
const FADE_IN_S: f32 = 1.8;
const FADE_OUT_S: f32 = 1.1;
/// Au-delà de cette altitude (m) la voiture vole : le sol se vide.
const AIR_LIMIT_M: f32 = 1.0;
/// Distance (m) derrière la voiture à partir de laquelle un véhicule dépassé s'efface.
const BEHIND_M: f64 = 30.0;

#[derive(Debug, Clone, Copy)]
struct Slot {
    craft: Option<CraftId>,
    lane: usize,
    x: f32,
    /// 0 (invisible) → 1 (plein).
    appear: f32,
    leaving: bool,
    /// Distance devant la voiture à l'image précédente : sert à détecter le dépassement.
    ahead: f64,
    passed: bool,
}

impl Slot {
    const EMPTY: Self = Self {
        craft: None,
        lane: 0,
        x: 0.0,
        appear: 0.0,
        leaving: false,
        ahead: 0.0,
        passed: false,
    };
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Le trafic au sol.
#[derive(Debug, Clone)]
pub struct GroundTraffic {
    /// Vitesse de la voiture dans le monde (m/s, positive), lissée.
    pub car_speed: f32,
    /// Dépassements réussis depuis le début.
    pub overtakes: u32,
    /// Instant (s de scène) du dernier dépassement.
    pub last_overtake_at: f64,
    pending: u32,
    slots: [Slot; MAX_CARS],
    countdown: f32,
    tint: i64,
    last_z: Option<f64>,
}

impl Default for GroundTraffic {
    fn default() -> Self {
        Self {
            car_speed: 0.0,
            overtakes: 0,
            last_overtake_at: -100.0,
            pending: 0,
            slots: [Slot::EMPTY; MAX_CARS],
            countdown: 2.0,
            tint: 0,
            last_z: None,
        }
    }
}

impl GroundTraffic {
    /// Nombre de véhicules visibles.
    #[must_use]
    pub fn count(&self) -> usize {
        self.slots.iter().filter(|s| s.craft.is_some()).count()
    }

    /// Un dépassement réussi depuis le dernier appel (une seule fois par dépassement).
    pub fn take_overtake(&mut self) -> bool {
        if self.pending == 0 {
            return false;
        }
        self.pending -= 1;
        true
    }

    /// Dépassements réussis non encore consommés.
    #[must_use]
    pub fn pending_overtakes(&self) -> u32 {
        self.pending
    }

    /// Ajoute à `out` les véhicules qui comptent pour le pilote, comme des malus posés devant la voiture. Leur position
    /// est ramenée à une distance « équivalente » (immobile) : l'écart se comble moins vite que pour un objet fixe, le
    /// plan de route du pilote se règle sur le vrai temps d'arrivée.
    pub fn views(&self, fleet: &Fleet, out: &mut Vec<PickupView>, car_z: f64) {
        for s in &self.slots {
            let Some(craft) = s.craft.and_then(|id| fleet.get(id)) else {
                continue;
            };
            if s.leaving || s.appear < 0.5 {
                continue;
            }
            let ahead = car_z - craft.position.z + REAR_M;
            let ratio = f64::from(RATIOS[s.lane]);
            out.push(PickupView {
                kind: PickupKind::Malus,
                x: f64::from(s.x),
                z: car_z - if ahead > 0.0 { ahead / (1.0 - ratio) } else { ahead },
                y: 0.0,
            });
        }
    }

    /// Avance d'une image. `busy` : une cascade ou une structure occupe la route.
    pub fn update(&mut self, frame: &TrafficFrame<'_>, busy: bool, fleet: &mut Fleet, rng: &mut Rand) {
        let car = frame.car;
        self.track(frame.dt, car.z);
        let density = frame
            .show
            .filter(|show| show.level >= 1)
            .map_or(0.0, |show| show.ground_traffic);
        let allowed = density > 0.02 && !busy && frame.altitude < AIR_LIMIT_M;
        // La qualité dégradée réduit le nombre de véhicules au sol comme celui du trafic de fond.
        let cap = if allowed {
            ((density * MAX_CARS as f32 * frame.density.clamp(0.0, 1.0)).round() as usize).clamp(1, MAX_CARS)
        } else {
            0
        };
        let live = self.slots.iter().filter(|s| s.craft.is_some() && !s.leaving).count();
        // Trop de véhicules pour la densité du moment : le plus lointain s'efface, un seul à la fois.
        if live > cap {
            self.dismiss_farthest(fleet, car.z);
        }
        for i in 0..MAX_CARS {
            if self.slots[i].craft.is_some() {
                self.step(i, frame, !allowed, fleet);
            }
        }
        if !allowed {
            return;
        }
        self.countdown -= frame.dt;
        if self.countdown <= 0.0 && live < cap {
            self.spawn(frame, density, fleet, rng);
        }
    }

    /// Remet tout à zéro, sans effacer en douceur (arrêt du spectacle, changement de voiture).
    pub fn clear(&mut self, fleet: &mut Fleet) {
        for slot in &mut self.slots {
            if let Some(id) = slot.craft.take() {
                fleet.release(id);
            }
        }
        self.pending = 0;
        self.last_z = None;
    }

    fn track(&mut self, dt: f32, z: f64) {
        if let Some(last) = self.last_z
            && dt > 0.0
        {
            let v = ((last - z) / f64::from(dt)) as f32;
            if v.is_finite() && (0.0..400.0).contains(&v) {
                self.car_speed += (v - self.car_speed) * (1.0 - (-dt * 6.0).exp());
            }
        }
        self.last_z = Some(z);
    }

    fn dismiss_farthest(&mut self, fleet: &Fleet, car_z: f64) {
        let mut far: Option<(usize, f64)> = None;
        for (i, s) in self.slots.iter().enumerate() {
            let Some(craft) = s.craft.and_then(|id| fleet.get(id)) else {
                continue;
            };
            let distance = car_z - craft.position.z;
            if !s.leaving && far.is_none_or(|(_, d)| distance > d) {
                far = Some((i, distance));
            }
        }
        if let Some((i, _)) = far {
            self.slots[i].leaving = true;
        }
    }

    fn spawn(&mut self, frame: &TrafficFrame<'_>, density: f32, fleet: &mut Fleet, rng: &mut Rand) {
        let Some(index) = self.slots.iter().position(|s| s.craft.is_none()) else {
            return;
        };
        let first = self.count() == 0;
        let ahead = if first {
            90.0 + rng.unit() * 50.0
        } else {
            SPAWN_MIN_M + rng.unit() * (SPAWN_MAX_M - SPAWN_MIN_M)
        };
        let z = frame.car.z - ahead;
        // Une file libre à cet endroit : aucun autre véhicule à moins de SPAWN_GAP (ils naissent échelonnés, jamais de
        // mur de trois).
        let blocked = self.slots.iter().any(|s| {
            s.craft
                .and_then(|id| fleet.get(id))
                .is_some_and(|c| (c.position.z - z).abs() < SPAWN_GAP_M)
        });
        let start = (rng.unit() * LANES.len() as f64) as usize % LANES.len();
        if blocked {
            self.countdown = 1.0;
            return;
        }
        let lane = start;
        let x = LANES[lane];
        let tint = frame.tint(self.tint);
        self.tint += 1;
        let variant = Fleet::sleek(rng.unit());
        let Some(id) = fleet.spawn(
            variant,
            DVec3::new(f64::from(x), 0.3, z),
            Vec3::ZERO,
            tint,
            BASE_SCALE,
            rng,
        ) else {
            return;
        };
        if let Some(craft) = fleet.get_mut(id) {
            craft.motion = Motion::Scripted;
            craft.trail = 0.0;
            craft.scale = 0.01;
            craft.position.y = f64::from(HOVER_M);
        }
        self.slots[index] = Slot {
            craft: Some(id),
            lane,
            x,
            appear: 0.0,
            leaving: false,
            ahead,
            passed: false,
        };
        self.countdown = (8.0 - 4.5 * density) * range(rng, 0.7, 1.3);
    }

    fn step(&mut self, index: usize, frame: &TrafficFrame<'_>, force_leave: bool, fleet: &mut Fleet) {
        let (dt, car) = (frame.dt, frame.car);
        let slot = &mut self.slots[index];
        let Some(id) = slot.craft else { return };
        let Some(craft) = fleet.get_mut(id) else {
            slot.craft = None;
            return;
        };
        if force_leave {
            slot.leaving = true;
        }
        let speed = self.car_speed * RATIOS[slot.lane];
        let z = craft.position.z - f64::from(speed * dt);
        let ahead = car.z - z;
        // Gêné par la voiture, il s'écarte un peu de son côté ; sinon il reprend le centre de sa file.
        let lane = LANES[slot.lane];
        let near = ahead.abs() < 14.0 && (car.x as f32 - slot.x).abs() < CLEAR_M && !slot.leaving;
        let want = if near {
            lane + if slot.x >= car.x as f32 { YIELD_M } else { -YIELD_M }
        } else {
            lane
        };
        let x = slot.x + (want - slot.x) * (1.0 - (-dt * 2.5).exp());
        craft.velocity = Vec3::new(if dt > 0.0 { (x - slot.x) / dt } else { 0.0 }, 0.0, -speed);
        slot.x = x;
        craft.position = DVec3::new(f64::from(x), f64::from(HOVER_M), z);

        if !slot.passed && slot.ahead > -REAR_M && ahead <= -REAR_M && !slot.leaving && slot.appear > 0.5 {
            slot.passed = true;
            self.overtakes += 1;
            self.pending += 1;
            self.last_overtake_at = frame.time;
        }
        slot.ahead = ahead;
        if ahead < -BEHIND_M {
            slot.leaving = true;
        }

        slot.appear = (slot.appear + if slot.leaving { -dt / FADE_OUT_S } else { dt / FADE_IN_S }).clamp(0.0, 1.0);
        let shown = smooth(slot.appear);
        craft.scale = (BASE_SCALE * shown).max(0.01);
        craft.trail = 0.35 * shown;
        if slot.leaving && slot.appear <= 0.0 {
            fleet.release(id);
            slot.craft = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::frame::TrafficShow;
    use cf_show::model::EventName as ShowEventName;

    const DT: f32 = 1.0 / 60.0;

    struct Rig {
        ground: GroundTraffic,
        fleet: Fleet,
        rng: Rand,
        car_z: f64,
        time: f64,
    }

    impl Rig {
        fn new() -> Self {
            Self {
                ground: GroundTraffic::default(),
                fleet: Fleet::new(),
                rng: Rand::new(9),
                car_z: 0.0,
                time: 0.0,
            }
        }

        fn run(&mut self, seconds: f32, level: u8, density: f32, altitude: f32, busy: bool) {
            let events: [ShowEventName; 0] = [];
            let show = TrafficShow {
                level,
                breathing: false,
                flashes: true,
                ground_traffic: density,
                event_rate: 1.0,
                events: &events,
                city_pulse: 0.0,
                titles: false,
                dj_seq: 0,
                level_seq: 0,
            };
            let mut t = 0.0;
            while t < seconds {
                // La voiture roule à 60 m/s vers −z.
                self.car_z -= 60.0 * f64::from(DT);
                self.time += f64::from(DT);
                let frame = TrafficFrame {
                    time: self.time,
                    car: DVec3::new(0.0, f64::from(altitude), self.car_z),
                    altitude,
                    show: Some(show),
                    ..TrafficFrame::default()
                };
                self.ground.update(&frame, busy, &mut self.fleet, &mut self.rng);
                self.fleet.update(DT, self.car_z, |_| true);
                t += DT;
            }
        }
    }

    #[test]
    fn au_niveau_zero_la_route_reste_vide() {
        let mut rig = Rig::new();
        rig.run(60.0, 0, 1.0, 0.0, false);
        assert_eq!(rig.ground.count(), 0);
    }

    #[test]
    fn des_vehicules_naissent_et_la_voiture_les_double() {
        let mut rig = Rig::new();
        rig.run(60.0, 3, 1.0, 0.0, false);
        assert!(rig.ground.overtakes > 0, "aucun dépassement en 60 s");
        assert!(rig.ground.count() <= MAX_CARS);
        assert!(rig.ground.take_overtake());
        assert!(rig.ground.last_overtake_at > 0.0);
    }

    #[test]
    fn la_voiture_qui_vole_vide_le_sol_en_douceur() {
        let mut rig = Rig::new();
        rig.run(40.0, 3, 1.0, 0.0, false);
        assert!(rig.ground.count() > 0);
        rig.run(0.5, 3, 1.0, 15.0, false);
        // Encore là une demi-seconde après (fondu de 1,1 s) ...
        assert!(rig.ground.count() > 0);
        rig.run(3.0, 3, 1.0, 15.0, false);
        assert_eq!(rig.ground.count(), 0);
        let mut active = 0;
        rig.fleet.for_each_active(|_| active += 1);
        assert_eq!(active, 0, "les véhicules sont rendus à la flotte");
    }

    #[test]
    fn une_structure_sur_la_route_vide_le_sol() {
        let mut rig = Rig::new();
        rig.run(40.0, 3, 1.0, 0.0, false);
        rig.run(4.0, 3, 1.0, 0.0, true);
        assert_eq!(rig.ground.count(), 0);
    }

    #[test]
    fn la_densite_borne_le_nombre_de_vehicules() {
        let mut rig = Rig::new();
        rig.run(120.0, 3, 0.2, 0.0, false);
        assert!(rig.ground.count() <= 1);
    }

    #[test]
    fn les_vues_du_pilote_sont_des_malus_devant_la_voiture() {
        let mut rig = Rig::new();
        rig.run(40.0, 3, 1.0, 0.0, false);
        let mut out = Vec::new();
        rig.ground.views(&rig.fleet, &mut out, rig.car_z);
        assert!(out.iter().all(|v| v.kind == PickupKind::Malus));
        assert!(out.iter().all(|v| v.z <= rig.car_z + 1e-6 || v.z.is_finite()));
        assert!(out.len() <= MAX_CARS);
    }
}
