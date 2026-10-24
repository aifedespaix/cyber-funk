//! Les types du vol : phases, figures, refus, ce que la scène fournit et ce que la voiture affiche.

use super::obstacles::{Band, DodgeSide, Obstruction};

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
