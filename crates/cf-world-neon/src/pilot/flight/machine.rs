//! Le vol de la voiture (`flight/flight.ts`) : étages, décollage et atterrissage, figures, esquives des structures qui
//! enjambent la route.
//!
//! La machine décide, la voiture joue : [`FlightFrame`] dit quoi afficher. Tout ordre peut être refusé ([`Refusal`]) :
//! pendant une transition, une figure, ou quand un véhicule volant occupe l'étage visé.

use super::obstacles::{Band, CLEARANCE, Dodge, DodgeSide, Obstruction, overlaps, resolve_dodge};
use crate::route::{FLOOR_COUNT, floor_altitude};

/// Durée du déploiement et de l'atterrissage des roues (s) : la même est envoyée à la voiture.
pub const HOVER_SECONDS: f64 = 1.8;
/// Le déploiement est presque fini : la voiture commence à monter vers son premier étage.
const CLIMB_AT: f64 = 0.88;
/// Sous cette altitude, en descendant, on commence à replier les roues.
const LAND_ALTITUDE: f64 = 0.7;
/// Entre deux figures : un court temps mort, pour ne pas les enchaîner par accident.
const TRICK_COOLDOWN: f64 = 0.35;
/// Combien de temps un ordre d'étage attend qu'une figure ou un véhicule s'écarte avant d'être refusé.
const PENDING_SECONDS: f64 = 1.6;

/// Phase du vol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlightPhase {
    /// Roues sur la chaussée.
    #[default]
    Ground,
    /// Les roues se déploient en répulseurs.
    Deploying,
    /// En vol, aux étages.
    Flying,
    /// Les roues se replient, la voiture se pose.
    Retracting,
}

/// Une figure en vol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrickKind {
    /// Tonneau à gauche.
    RollLeft,
    /// Tonneau à droite.
    RollRight,
    /// Salto arrière (monte d'un étage).
    Backflip,
    /// Salto avant (descend d'un étage).
    Frontflip,
}

impl TrickKind {
    /// Durée de la figure, s.
    #[must_use]
    pub fn seconds(self) -> f64 {
        match self {
            Self::RollLeft | Self::RollRight => 0.95,
            Self::Backflip | Self::Frontflip => 1.2,
        }
    }
}

/// Pourquoi un ordre est refusé : de quoi faire réagir l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Déploiement ou repli des roues en cours.
    Transition,
    /// Une figure est en cours.
    Trick,
    /// Un véhicule occupe l'étage visé.
    Traffic,
    /// « Réduire les animations » : pas de figures.
    Calm,
    /// Dernier étage atteint.
    Limit,
}

/// Ce que la scène fournit à chaque image.
#[derive(Debug, Clone, Copy)]
pub struct FlightEnv<'a> {
    /// Durée de l'image, s.
    pub dt: f64,
    /// Profondeur de la voiture, m.
    pub car_z: f64,
    /// Vitesse du monde (m/s) : elle règle l'anticipation des esquives.
    pub speed: f64,
    /// « Réduire les animations » : pas de figures.
    pub calm: bool,
    /// Structures à esquiver (immeubles-arches, route croisée…).
    pub obstructions: &'a [Obstruction],
    /// Tranches d'altitude occupées par des véhicules proches.
    pub bands: &'a [Band],
}

/// Ce que la voiture doit afficher à cette image (`FlightFrame`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlightFrame {
    /// Phase du vol.
    pub phase: FlightPhase,
    /// Altitude de la voiture au-dessus de la route (m), esquives et figures comprises.
    pub altitude: f64,
    /// Altitude visée par l'étage (sans esquive) : elle règle le trafic aérien.
    pub target: f64,
    /// Mode volant demandé à la voiture (roues en répulseurs).
    pub hover: bool,
    /// Avancement du déploiement (ou du repli) des roues, 0 → 1.
    pub hover_amount: f64,
    /// Vitesse verticale lissée, m/s.
    pub climb: f64,
    /// Inclinaison due à la montée et à la descente (rad ; + = nez en l'air).
    pub pitch: f64,
    /// Roulis dû à l'esquive (rad ; + = roulis à droite).
    pub roll: f64,
    /// Rotation de tonneau en cours (rad, de 0 à ±2π).
    pub trick_roll: f64,
    /// Rotation de salto en cours (rad, de 0 à ±2π).
    pub trick_pitch: f64,
    /// Rebond de la caisse sur ses répulseurs (m), poussé par la musique.
    pub bounce: f64,
    /// Esquive en cours.
    pub dodge: DodgeSide,
    /// Étage visé : 0 = sol.
    pub floor: u8,
    /// La figure en cours.
    pub trick: Option<TrickKind>,
}

/// Faits de l'image que la scène relit (son, caméra) : remplacent les rappels `onTrick`, `onTrickStart` et `onPhase`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FlightEvents {
    /// Une figure vient de partir.
    pub trick_started: Option<TrickKind>,
    /// Une figure vient de s'achever.
    pub trick_done: Option<TrickKind>,
    /// La phase vient de changer.
    pub phase_changed: Option<FlightPhase>,
}

fn smoother(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

#[derive(Debug, Clone, Copy)]
struct Trick {
    kind: TrickKind,
    t: f64,
}

#[derive(Debug, Clone, Copy)]
struct Pending {
    floor: i32,
    reason: Refusal,
    ttl: f64,
}

/// Le vol : l'état, les ordres, la pose.
#[derive(Debug, Clone, Default)]
pub struct Flight {
    phase: FlightPhase,
    floor: i32,
    /// Dernier refus et son compteur : l'interface s'en sert pour réagir.
    pub refusal: Option<Refusal>,
    /// Nombre de refus depuis le début.
    pub refusals: u32,
    /// Faits de l'image (à vider par l'appelant avant `update`).
    pub events: FlightEvents,
    altitude: f64,
    velocity: f64,
    hover_progress: f64,
    trick: Option<Trick>,
    trick_cooldown: f64,
    dodge_offset: f64,
    dodge_velocity: f64,
    dodge_side: DodgeSide,
    dodge_busy: bool,
    pending: Option<Pending>,
    last_altitude: f64,
    climb: f64,
    bank: f64,
    bounce: f64,
    bounce_velocity: f64,
}

impl Flight {
    /// Phase du vol.
    #[must_use]
    pub fn phase(&self) -> FlightPhase {
        self.phase
    }

    /// Étage visé : 0 = sol, 1 à `FLOOR_COUNT`.
    #[must_use]
    pub fn floor(&self) -> i32 {
        self.floor
    }

    /// Vrai si la voiture n'est pas au sol.
    #[must_use]
    pub fn flying(&self) -> bool {
        self.phase != FlightPhase::Ground
    }

    /// Vrai quand les étages sont atteignables : roues repliées, pas de figure en cours.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.phase == FlightPhase::Flying && self.trick.is_none()
    }

    /// Avancement du déploiement (ou du repli) des roues, 0 → 1.
    #[must_use]
    pub fn hover_amount(&self) -> f64 {
        self.hover_progress
    }

    /// Une figure est en cours.
    #[must_use]
    pub fn tricking(&self) -> bool {
        self.trick.is_some()
    }

    /// Altitude actuelle, esquive comprise.
    #[must_use]
    pub fn altitude_now(&self) -> f64 {
        self.altitude + self.dodge_offset
    }

    /// Monte d'un étage (ou décolle depuis le sol). Renvoie vrai si l'ordre est accepté.
    pub fn up(&mut self, env: &FlightEnv<'_>) -> bool {
        match self.phase {
            FlightPhase::Ground | FlightPhase::Retracting => self.take_off(),
            FlightPhase::Deploying => self.refuse(Refusal::Transition),
            FlightPhase::Flying => self.move_to(self.floor + 1, env),
        }
    }

    /// Descend d'un étage ; du premier étage, c'est l'atterrissage.
    pub fn down(&mut self, env: &FlightEnv<'_>) -> bool {
        match self.phase {
            FlightPhase::Ground | FlightPhase::Retracting => false,
            FlightPhase::Deploying => self.land(),
            FlightPhase::Flying => self.move_to(self.floor - 1, env),
        }
    }

    /// Lance une figure. Un salto ajoute ou retire un étage en passant (arrière : haut ; avant : bas, sans jamais
    /// atterrir).
    pub fn do_trick(&mut self, kind: TrickKind, env: &FlightEnv<'_>) -> bool {
        if env.calm {
            return self.refuse(Refusal::Calm);
        }
        if self.phase != FlightPhase::Flying {
            return self.refuse(Refusal::Transition);
        }
        if self.trick.is_some() || self.trick_cooldown > 0.0 {
            return self.refuse(Refusal::Trick);
        }
        self.trick = Some(Trick { kind, t: 0.0 });
        self.events.trick_started = Some(kind);
        let count = i32::try_from(FLOOR_COUNT).unwrap_or(6);
        if kind == TrickKind::Backflip && self.floor < count && self.can_enter(self.floor + 1, env) {
            self.floor += 1;
        } else if kind == TrickKind::Frontflip && self.floor > 1 && self.can_enter(self.floor - 1, env) {
            self.floor -= 1;
        }
        true
    }

    /// Un coup sur les répulseurs : la caisse rebondit (danse, kicks). `strength` en mètres.
    pub fn kick(&mut self, strength: f64) {
        self.bounce_velocity += strength * 9.0;
    }

    /// Avance d'une image et renvoie la pose.
    pub fn update(&mut self, env: &FlightEnv<'_>) -> FlightFrame {
        let dt = env.dt.min(0.05);
        self.trick_cooldown = (self.trick_cooldown - dt).max(0.0);

        match self.phase {
            FlightPhase::Deploying => {
                self.hover_progress = (self.hover_progress + dt / HOVER_SECONDS).min(1.0);
                if self.hover_progress >= 1.0 {
                    self.set_phase(FlightPhase::Flying);
                }
            }
            FlightPhase::Retracting => {
                self.hover_progress = (self.hover_progress - dt / HOVER_SECONDS).max(0.0);
                if self.hover_progress <= 0.0 && self.altitude < 0.05 {
                    self.altitude = 0.0;
                    self.velocity = 0.0;
                    self.set_phase(FlightPhase::Ground);
                }
            }
            // Atterrissage : la descente jusqu'au sol, puis les roues se replient.
            FlightPhase::Flying => {
                if self.floor == 0 && self.altitude < LAND_ALTITUDE {
                    self.set_phase(FlightPhase::Retracting);
                }
            }
            FlightPhase::Ground => {}
        }

        self.retry(env, dt);
        self.spring(dt);
        self.dodge(env, dt);
        let (trick_roll, trick_pitch, hop) = self.step_trick(dt);

        // Un rebond de caisse, ressort sous-amorti : il se calme seul.
        self.bounce_velocity += (-160.0 * self.bounce - 9.0 * self.bounce_velocity) * dt;
        self.bounce += self.bounce_velocity * dt;
        self.bounce = self.bounce.clamp(-0.2, 1.1);

        let altitude = (self.altitude + self.dodge_offset + hop).max(0.0);
        let rate = if dt > 0.0 { (altitude - self.last_altitude) / dt } else { 0.0 };
        self.last_altitude = altitude;
        self.climb += (rate - self.climb) * (1.0 - (-dt * 7.0).exp());
        // Le nez suit la montée ; la caisse se couche dans l'esquive.
        self.bank += (-self.dodge_velocity * 0.026 - self.bank) * (1.0 - (-dt * 8.0).exp());

        FlightFrame {
            phase: self.phase,
            altitude,
            target: floor_altitude(self.floor),
            hover: matches!(self.phase, FlightPhase::Deploying | FlightPhase::Flying),
            hover_amount: self.hover_progress,
            climb: self.climb,
            pitch: (self.climb * 0.022).clamp(-0.3, 0.3),
            roll: self.bank.clamp(-0.35, 0.35),
            trick_roll,
            trick_pitch,
            bounce: self.bounce,
            dodge: if self.dodge_busy { self.dodge_side } else { DodgeSide::None },
            floor: u8::try_from(self.floor).unwrap_or(0),
            trick: self.trick.map(|t| t.kind),
        }
    }

    /// Remet tout au sol (changement de voiture, de thème…).
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    fn take_off(&mut self) -> bool {
        self.floor = 1;
        if self.phase == FlightPhase::Ground {
            self.hover_progress = 0.0;
        }
        self.set_phase(FlightPhase::Deploying);
        true
    }

    fn land(&mut self) -> bool {
        self.floor = 0;
        // Depuis le vol : la voiture descend d'abord, les roues se replient quand elle est presque au sol (voir `update`).
        // Pendant le déploiement : on annule, le repli repart de la progression actuelle.
        if self.phase == FlightPhase::Deploying {
            self.set_phase(FlightPhase::Retracting);
        }
        true
    }

    /// Un ordre d'étage. L'esquive n'empêche jamais de bouger (elle se recalcule depuis le nouvel étage) ; une figure ou un
    /// véhicule sur l'étage visé retardent seulement l'ordre : il est mis en attente un instant et joué dès que c'est libre.
    fn move_to(&mut self, floor: i32, env: &FlightEnv<'_>) -> bool {
        if floor > i32::try_from(FLOOR_COUNT).unwrap_or(6) {
            return self.refuse(Refusal::Limit);
        }
        if floor < 0 {
            return false;
        }
        if floor == 0 {
            self.pending = None;
            return self.land();
        }
        if let Some(reason) = self.blocker(floor, env) {
            self.pending = Some(Pending {
                floor,
                reason,
                ttl: PENDING_SECONDS,
            });
            return false;
        }
        self.pending = None;
        self.floor = floor;
        true
    }

    fn blocker(&self, floor: i32, env: &FlightEnv<'_>) -> Option<Refusal> {
        if self.trick.is_some() {
            return Some(Refusal::Trick);
        }
        if !self.can_enter(floor, env) {
            return Some(Refusal::Traffic);
        }
        None
    }

    /// Rejoue l'ordre mis en attente ; au bout de `PENDING_SECONDS`, il est refusé pour de bon.
    fn retry(&mut self, env: &FlightEnv<'_>, dt: f64) {
        let Some(mut wait) = self.pending else { return };
        if self.phase != FlightPhase::Flying {
            self.pending = None;
            return;
        }
        if self.blocker(wait.floor, env).is_none() {
            self.floor = wait.floor;
            self.pending = None;
            return;
        }
        wait.ttl -= dt;
        if wait.ttl <= 0.0 {
            self.pending = None;
            self.refuse(wait.reason);
        } else {
            self.pending = Some(wait);
        }
    }

    /// L'étage est-il libre ? Aucun véhicule ne l'occupe à portée.
    fn can_enter(&self, floor: i32, env: &FlightEnv<'_>) -> bool {
        let altitude = floor_altitude(floor);
        !env.bands.iter().any(|&b| overlaps(altitude, b, CLEARANCE + 0.8))
    }

    fn refuse(&mut self, reason: Refusal) -> bool {
        self.refusal = Some(reason);
        self.refusals += 1;
        false
    }

    fn set_phase(&mut self, phase: FlightPhase) {
        if self.phase == phase {
            return;
        }
        self.phase = phase;
        self.events.phase_changed = Some(phase);
    }

    /// Ressort amorti vers l'altitude de l'étage : on ne monte qu'une fois les roues presque repliées.
    fn spring(&mut self, dt: f64) {
        let climbing = matches!(self.phase, FlightPhase::Flying | FlightPhase::Retracting)
            || (self.phase == FlightPhase::Deploying && self.hover_progress >= CLIMB_AT);
        let target = if climbing { floor_altitude(self.floor) } else { 0.0 };
        let omega = 4.6;
        self.velocity += (omega * omega * (target - self.altitude) - 2.0 * 0.9 * omega * self.velocity) * dt;
        self.velocity = self.velocity.clamp(-11.0, 11.0);
        self.altitude = (self.altitude + self.velocity * dt).max(0.0);
    }

    /// Esquive : pour chaque structure à venir, la part d'esquive (0 → 1 avec la distance) est appliquée sur l'altitude
    /// visée. Un calcul, pas une collision : par dessus ou par dessous, selon ce qui est le plus court.
    fn dodge(&mut self, env: &FlightEnv<'_>, dt: f64) {
        let mut plan = Dodge::default();
        if matches!(self.phase, FlightPhase::Flying | FlightPhase::Deploying) {
            plan = resolve_dodge(env.obstructions, floor_altitude(self.floor), env.car_z, env.speed);
            if plan.busy {
                self.dodge_side = plan.side;
            }
        }
        self.dodge_busy = plan.busy;
        // Le décalage suit sa consigne par un ressort : l'esquive est fluide même quand la consigne saute (deux obstacles
        // d'affilée).
        let omega = 7.0;
        self.dodge_velocity +=
            (omega * omega * (plan.offset - self.dodge_offset) - 2.0 * 0.95 * omega * self.dodge_velocity) * dt;
        self.dodge_offset += self.dodge_velocity * dt;
        if !plan.busy && self.dodge_offset.abs() < 0.01 && self.dodge_velocity.abs() < 0.02 {
            self.dodge_offset = 0.0;
            self.dodge_velocity = 0.0;
        }
    }

    /// Avance la figure en cours : rotation de tonneau, de salto, saut de la caisse.
    fn step_trick(&mut self, dt: f64) -> (f64, f64, f64) {
        let Some(mut trick) = self.trick else {
            return (0.0, 0.0, 0.0);
        };
        trick.t += dt / trick.kind.seconds();
        let done = trick.t >= 1.0;
        let progress = trick.t.min(1.0);
        let turn = smoother(progress) * std::f64::consts::TAU;
        let wave = (progress * std::f64::consts::PI).sin();
        let (roll, pitch, hop) = match trick.kind {
            TrickKind::RollLeft => (-turn, 0.0, wave * 0.25),
            TrickKind::RollRight => (turn, 0.0, wave * 0.25),
            TrickKind::Backflip => (0.0, turn, wave * 0.9),
            TrickKind::Frontflip => (0.0, -turn, wave * 0.9),
        };
        if done {
            self.trick = None;
            self.trick_cooldown = TRICK_COOLDOWN;
            self.events.trick_done = Some(trick.kind);
            return (0.0, 0.0, 0.0);
        }
        self.trick = Some(trick);
        (roll, pitch, hop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EMPTY: FlightEnv<'static> = FlightEnv {
        dt: 1.0 / 60.0,
        car_z: -1000.0,
        speed: 40.0,
        calm: false,
        obstructions: &[],
        bands: &[],
    };

    fn run(flight: &mut Flight, seconds: f64, env: &FlightEnv<'_>) -> FlightFrame {
        let mut frame = FlightFrame::default();
        for _ in 0..(seconds * 60.0) as usize {
            frame = flight.update(env);
        }
        frame
    }

    #[test]
    fn decolle_monte_a_l_etage_puis_se_pose() {
        let mut f = Flight::default();
        assert!(f.up(&EMPTY));
        assert_eq!(f.phase(), FlightPhase::Deploying);
        let frame = run(&mut f, 6.0, &EMPTY);
        assert_eq!(frame.phase, FlightPhase::Flying);
        assert!((frame.altitude - 3.5).abs() < 0.1, "{}", frame.altitude);
        assert!(f.up(&EMPTY));
        let frame = run(&mut f, 4.0, &EMPTY);
        assert!((frame.altitude - 8.0).abs() < 0.2, "{}", frame.altitude);
        assert!(f.down(&EMPTY) && f.down(&EMPTY));
        let frame = run(&mut f, 8.0, &EMPTY);
        assert_eq!(frame.phase, FlightPhase::Ground);
        assert!(frame.altitude < 0.06);
    }

    #[test]
    fn une_figure_dure_sa_duree_et_un_salto_change_d_etage() {
        let mut f = Flight::default();
        f.up(&EMPTY);
        run(&mut f, 6.0, &EMPTY);
        f.up(&EMPTY);
        run(&mut f, 3.0, &EMPTY);
        assert_eq!(f.floor(), 2);
        assert!(f.do_trick(TrickKind::Backflip, &EMPTY));
        assert_eq!(f.floor(), 3, "le salto arrière monte");
        assert!(!f.do_trick(TrickKind::RollLeft, &EMPTY), "une figure à la fois");
        let mid = run(&mut f, 0.6, &EMPTY);
        assert!(mid.trick_pitch.abs() > 0.5 && mid.trick == Some(TrickKind::Backflip));
        let end = run(&mut f, 0.8, &EMPTY);
        assert!(end.trick.is_none() && end.trick_pitch.abs() < f64::EPSILON);
        assert_eq!(f.events.trick_done, Some(TrickKind::Backflip));
    }

    #[test]
    fn pas_de_figure_en_mode_calme() {
        let mut f = Flight::default();
        f.up(&EMPTY);
        run(&mut f, 6.0, &EMPTY);
        let calm = FlightEnv { calm: true, ..EMPTY };
        assert!(!f.do_trick(TrickKind::RollRight, &calm));
        assert_eq!(f.refusal, Some(Refusal::Calm));
    }

    #[test]
    fn un_vehicule_sur_l_etage_retarde_l_ordre_puis_le_refuse() {
        let mut f = Flight::default();
        f.up(&EMPTY);
        run(&mut f, 6.0, &EMPTY);
        let band = [Band { lo: 6.0, hi: 10.0 }];
        let busy = FlightEnv { bands: &band, ..EMPTY };
        assert!(!f.up(&busy), "étage 2 occupé");
        run(&mut f, 2.0, &busy);
        assert_eq!(f.floor(), 1);
        assert_eq!(f.refusal, Some(Refusal::Traffic));
    }

    #[test]
    fn l_etage_cinq_esquive_le_tablier_de_l_arche() {
        let span = crate::route::bridge_span();
        let deck = [Obstruction {
            z_near: -1000.0 + span.half_depth,
            z_far: -1000.0 - span.half_depth,
            lo: span.lo,
            hi: span.hi,
        }];
        let mut f = Flight::default();
        f.up(&EMPTY);
        run(&mut f, 6.0, &EMPTY);
        for _ in 0..4 {
            f.up(&EMPTY);
            run(&mut f, 2.5, &EMPTY);
        }
        assert_eq!(f.floor(), 5);
        let env = FlightEnv {
            obstructions: &deck,
            car_z: -1000.0,
            ..EMPTY
        };
        let frame = run(&mut f, 2.0, &env);
        assert!(frame.altitude > 33.0, "passe au-dessus du tablier : {}", frame.altitude);
        assert_eq!(frame.dodge, DodgeSide::Over);
    }
}
