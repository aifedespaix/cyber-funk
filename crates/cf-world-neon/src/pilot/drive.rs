//! Le conducteur : tout ce que `sillage-scene.ts` fait de la voiture à chaque image, sans le jeu. Allure, inertie,
//! volant, danse, vol : une entrée ([`PilotTick`]), une sortie ([`PilotFrame`]), aucun Bevy, aucune allocation en régime
//! établi. Le système Bevy ([`super::systems`]) n'a plus qu'à lire les ressources et à écrire la position.

use cf_analysis::Section;

use super::animation::CarAnimation;
use super::dance::{DanceClock, DanceStyle, dance_position};
use super::flight::{
    AutoContext, AutoFlight, DanceContext, DanceFlight, Flight, FlightEnv, FlightEvents, FlightPhase, Obstruction,
    bridge_obstructions,
};
use super::input::{BeatInput, PilotTraffic, ShowInput};
use super::pace::{CRUISE, Pace, PaceState};
use super::steer::{Autopilot, SteerInput};
use crate::route::{FULL_RATE, LIMIT};

/// Allure du pilote : adhérence sans laquelle le volant braque mais la voiture ne glisse pas de côté (part de la
/// croisière).
const GRIP_SPEED_RATIO: f64 = 0.6;
/// Vitesse de rattrapage de l'allure voulue : on prend de la vitesse mollement, on freine plus vite, on roule sur son
/// erreur quand la musique s'arrête (1/s).
const ACCEL_RATE: f64 = 1.7;
const BRAKE_RATE: f64 = 3.0;
const COAST_RATE: f64 = 1.6;
/// Part de la vitesse qui devient du chemin parcouru : la route défile un peu moins vite que le compteur.
const SCROLL: f64 = 0.85;
/// Structures retenues par image (arches et trafic).
const MAX_OBSTRUCTIONS: usize = 16;

/// Réglages du pilote (ressource Bevy).
#[derive(bevy::prelude::Resource, Debug, Clone, Copy, PartialEq)]
pub struct PilotSettings {
    /// Danse choisie : la voiture suit sa position latérale au lieu de la ligne de course, et danse en hauteur hors
    /// spectacle. `None` : ligne de course et envols du pilote.
    pub dance: Option<DanceStyle>,
    /// Plafond de vitesse (m/s), posé par la pré-intro du niveau 0 ; `None` : pas de plafond.
    pub speed_cap_mps: Option<f64>,
    /// « Réduire les animations » : pas de figures.
    pub calm: bool,
    /// Graine des décisions du pilote (file de la ligne de course, envols, figures).
    pub seed: u64,
    /// Profondeur de départ (z absolu, m) : 0 au départ du set, −400 000 pour vérifier la précision en fin de set.
    pub start_z_m: f64,
    /// Spectacle imposé (démos, captures, tests) : remplace celui de `CurrentShow`. `None` : celui du spectacle en
    /// cours, ou tout permis sans spectacle.
    pub show: Option<ShowInput>,
}

impl Default for PilotSettings {
    fn default() -> Self {
        Self {
            dance: None,
            speed_cap_mps: None,
            calm: false,
            seed: 0x5111_A6E,
            start_z_m: 0.0,
            show: None,
        }
    }
}

/// Les entrées d'une image.
#[derive(Debug, Clone, Copy)]
pub struct PilotTick<'a> {
    /// Durée réelle de l'image, s.
    pub dt: f64,
    /// L'audio de l'image.
    pub beat: BeatInput,
    /// Le spectacle de l'image, ou `None` hors spectacle.
    pub show: Option<ShowInput>,
    /// Réglages.
    pub settings: &'a PilotSettings,
    /// La circulation vue de la voiture.
    pub traffic: &'a PilotTraffic,
    /// Dépassements réussis depuis la dernière image.
    pub overtakes: u32,
}

/// Ce que le pilote décide à cette image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PilotFrame {
    /// Abscisse de la voiture, m.
    pub car_x: f64,
    /// Profondeur de la voiture (z absolu), m.
    pub car_z: f64,
    /// Vitesse, m/s (avant `SCROLL`).
    pub speed: f64,
    /// Angle du volant, −1 à 1 (`wheelAngle` : braque les roues et commande le clignotant).
    pub steer: f64,
    /// Freinage, 0 à 1.
    pub brake: f64,
    /// Un drop vient de déclencher un boost (une image).
    pub boosted: bool,
    /// Animation de vol.
    pub animation: CarAnimation,
    /// Faits du vol de l'image (figure, phase).
    pub events: FlightEvents,
}

/// Le conducteur automatique.
#[derive(Debug, Clone)]
pub struct Pilot {
    pace: Pace,
    autopilot: Autopilot,
    flight: Flight,
    auto_flight: AutoFlight,
    dance_flight: DanceFlight,
    dance_clock: DanceClock,
    obstructions: Vec<Obstruction>,
    car_x: f64,
    car_z: f64,
    speed: f64,
    accel: f64,
    brake: f64,
    wheel_angle: f64,
    refusals: u32,
    last: PilotFrame,
}

impl Pilot {
    /// Un pilote posé en `start_z_m`, ses décisions tirées de `seed`.
    #[must_use]
    pub fn new(seed: u64, start_z_m: f64) -> Self {
        let last = PilotFrame {
            car_x: 0.0,
            car_z: start_z_m,
            speed: 0.0,
            steer: 0.0,
            brake: 0.0,
            boosted: false,
            animation: CarAnimation::default(),
            events: FlightEvents::default(),
        };
        Self {
            pace: Pace::default(),
            autopilot: Autopilot::new((seed % 1000) as f64),
            flight: Flight::default(),
            auto_flight: AutoFlight::new(seed),
            dance_flight: DanceFlight::default(),
            dance_clock: DanceClock::default(),
            obstructions: Vec::with_capacity(MAX_OBSTRUCTIONS),
            car_x: 0.0,
            car_z: start_z_m,
            speed: 0.0,
            accel: 0.0,
            brake: 0.0,
            wheel_angle: 0.0,
            refusals: 0,
            last,
        }
    }

    /// La dernière image.
    #[must_use]
    pub fn frame(&self) -> &PilotFrame {
        &self.last
    }

    /// La machine de vol (lecture).
    #[must_use]
    pub fn flight(&self) -> &Flight {
        &self.flight
    }

    /// Changement de titre : on oublie l'allure du morceau précédent.
    pub fn reset_track(&mut self) {
        self.pace.reset();
    }

    /// Remet la voiture au sol, au centre (changement de voiture, de thème).
    pub fn reset(&mut self) {
        self.flight.reset();
        self.autopilot.reset(0.0);
        self.car_x = 0.0;
        self.speed = 0.0;
        self.accel = 0.0;
        self.brake = 0.0;
        self.wheel_angle = 0.0;
    }

    /// Avance d'une image.
    pub fn update(&mut self, t: &PilotTick<'_>) -> PilotFrame {
        self.flight.events = FlightEvents::default();
        let show = t.show.as_ref();
        let b = &t.beat;
        let playing = b.live;
        for _ in 0..t.overtakes {
            self.pace.overtook();
        }
        let paced = self.pace.update(b, t.dt, show);
        // Le monde ralentit avec le temps du spectacle (ralenti, arrêt sur image).
        let dt = t.dt * show.map_or(1.0, |s| s.time_scale);

        self.advance_speed(&paced, playing, t.settings.speed_cap_mps, dt);
        self.car_z -= self.speed * SCROLL * dt;

        let beat = self.dance_clock.update(dt, b.bpm, b.kick, playing);
        let preferred = t.settings.dance.map(|style| dance_position(style, beat) * LIMIT);
        let steer = self.steer(t, show, beat, preferred, dt);
        let rate = self.lateral_rate();
        self.car_x += (steer * LIMIT - self.car_x) * (1.0 - (-dt * rate).exp());
        // Le volant tourne tant que la voiture a de la route à parcourir vers sa position visée (l'écart restant, saturé à
        // 3 m) : il se redresse tout seul en butée ou quand elle arrive, et il glisse vers sa valeur au lieu de sauter.
        let wheel_target = ((steer * LIMIT - self.car_x) / 3.0).clamp(-1.0, 1.0);
        self.wheel_angle += (wheel_target - self.wheel_angle) * (1.0 - (-dt * 9.0).exp());

        let animation = self.fly(t, show, paced.boosted, beat, dt);
        self.last = PilotFrame {
            car_x: self.car_x,
            car_z: self.car_z,
            speed: self.speed,
            steer: self.wheel_angle,
            brake: self.brake,
            boosted: paced.boosted,
            animation,
            events: self.flight.events,
        };
        self.last
    }

    /// Inertie : on prend de la vitesse mollement, on freine plus vite, on roule sur son erreur quand la musique s'arrête.
    /// Pré-intro (niveau 0) : la voiture roule lentement, plafonnée, même avant que la musique ne soit là.
    fn advance_speed(&mut self, paced: &PaceState, playing: bool, cap: Option<f64>, dt: f64) {
        let target = match cap {
            Some(cap) => (if playing { paced.speed } else { cap }).min(cap),
            None if playing => paced.speed,
            None => 0.0,
        };
        let rate = if target >= self.speed {
            ACCEL_RATE + paced.surge * 3.0
        } else if playing {
            BRAKE_RATE
        } else {
            COAST_RATE
        };
        let before = self.speed;
        self.speed += (target - self.speed) * (1.0 - (-dt * rate).exp());
        let instant = if dt > 0.0 { (self.speed - before) / dt } else { 0.0 };
        self.accel += (instant - self.accel) * (1.0 - (-dt * 9.0).exp());
        let braking = ((-self.accel - 3.0) / 14.0).clamp(0.0, 1.0);
        self.brake += (braking - self.brake) * (1.0 - (-dt * if braking > self.brake { 14.0 } else { 4.0 }).exp());
    }

    /// Taux de rattrapage latéral réel : sans vitesse, le volant braque mais la voiture ne glisse pas ; très vite au-delà
    /// de la croisière, elle sous-vire.
    fn lateral_rate(&self) -> f64 {
        let grip = (self.speed / (CRUISE * GRIP_SPEED_RATIO)).min(1.0);
        let understeer = 1.0 - 0.4 * ((self.speed - CRUISE) / (CRUISE * 0.6)).clamp(0.0, 1.0);
        FULL_RATE * grip * grip * (3.0 - 2.0 * grip) * understeer
    }

    fn steer(
        &mut self,
        t: &PilotTick<'_>,
        show: Option<&ShowInput>,
        beat: f64,
        preferred: Option<f64>,
        dt: f64,
    ) -> f64 {
        // Niveau 0 (pré-intro) : la voiture file droit.
        if show.is_some_and(|s| s.level == 0) {
            return 0.0;
        }
        self.autopilot.steer(&SteerInput {
            car_x: self.car_x,
            car_z: self.car_z,
            altitude: self.last.animation.altitude.into(),
            ground: &t.traffic.ground,
            preferred,
            dt,
            lateral_rate: self.lateral_rate(),
            beat,
            energy: t.beat.intensity,
            show,
        })
    }

    /// Vol : les ordres (danse ou pilote), les étages, les esquives → la pose de la voiture.
    fn fly(&mut self, t: &PilotTick<'_>, show: Option<&ShowInput>, boosted: bool, beat: f64, dt: f64) -> CarAnimation {
        let b = &t.beat;
        self.obstructions.clear();
        bridge_obstructions(self.car_z, &mut self.obstructions);
        let room = MAX_OBSTRUCTIONS.saturating_sub(self.obstructions.len());
        self.obstructions.extend(t.traffic.obstructions.iter().take(room));
        let env = FlightEnv {
            dt,
            car_z: self.car_z,
            speed: self.speed,
            calm: t.settings.calm,
            obstructions: &self.obstructions,
            bands: &t.traffic.bands,
        };
        let command = match (t.settings.dance, show) {
            // La danse prime sur le pilote quand aucun spectacle ne tourne ; chaque kick fait rebondir la caisse.
            (Some(style), None) => {
                if b.kick && self.flight.phase() == FlightPhase::Flying {
                    self.flight.kick(0.1 + b.bass * 0.22);
                }
                self.dance_flight.think(
                    &self.flight,
                    &DanceContext {
                        dt,
                        kick: b.kick,
                        count: b.count,
                        section: b.section,
                        intensity: b.intensity,
                        outro: false,
                        drop: boosted || b.section == Section::Peak,
                        playing: b.live,
                        style,
                        dance_beat: beat,
                    },
                )
            }
            _ => self.auto_flight.think(
                &self.flight,
                &AutoContext {
                    dt,
                    playing: b.live,
                    show,
                },
            ),
        };
        if let Some(command) = command {
            command.apply(&mut self.flight, &env);
        }
        let frame = self.flight.update(&env);
        if self.flight.refusals != self.refusals {
            self.refusals = self.flight.refusals;
            // Un ordre refusé : la caisse tressaute, comme sous un coup de répulseur.
            self.flight.kick(0.12);
        }
        CarAnimation::from(&frame)
    }
}
