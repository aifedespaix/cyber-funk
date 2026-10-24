//! Le pilote automatique de la verticale (`AutoFlight` de `flight/pilot.ts`) : quand décoller, quel étage, quelle
//! figure. Il passe par les mêmes ordres que Sillage donnait au joueur : s'ils sont refusés (véhicule sur l'étage,
//! esquive en cours), il réessaie un peu plus tard.
//!
//! Sillage cherche d'abord les bonus qui flottent aux étages (`bestFloor`) ; c'est le mode Jeu, il n'y en a pas en
//! cinéma : seul reste le petit vol que le pilote s'offre de temps en temps, réglé par les portes du look.

use super::machine::{Flight, FlightEnv, FlightPhase, TrickKind};
use crate::pilot::input::ShowInput;
use crate::pilot::rand::Rand;
use crate::route::FLOOR_COUNT;

/// Un ordre de la verticale (`FlightCommand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightCommand {
    /// Monte d'un étage (ou décolle).
    Up,
    /// Descend d'un étage (ou se pose).
    Down,
    /// Tonneau : −1 à gauche, 1 à droite.
    Roll(i8),
    /// Salto : 1 arrière (monte), −1 avant (descend).
    Flip(i8),
}

impl FlightCommand {
    /// Joue l'ordre sur la machine de vol.
    pub fn apply(self, flight: &mut Flight, env: &FlightEnv<'_>) {
        match self {
            Self::Up => {
                flight.up(env);
            }
            Self::Down => {
                flight.down(env);
            }
            Self::Roll(direction) => {
                flight.do_trick(
                    if direction < 0 {
                        TrickKind::RollLeft
                    } else {
                        TrickKind::RollRight
                    },
                    env,
                );
            }
            Self::Flip(direction) => {
                flight.do_trick(
                    if direction > 0 {
                        TrickKind::Backflip
                    } else {
                        TrickKind::Frontflip
                    },
                    env,
                );
            }
        }
    }
}

/// Ce que le pilote de la verticale lit à chaque image.
#[derive(Debug, Clone, Copy)]
pub struct AutoContext<'a> {
    /// Durée de l'image, s.
    pub dt: f64,
    /// Une musique joue (source vivante).
    pub playing: bool,
    /// Le spectacle (étage maximal, fréquence, drops, figures, chaleur) ; absent : tout est permis.
    pub show: Option<&'a ShowInput>,
}

/// Le pilote de la verticale.
#[derive(Debug, Clone)]
pub struct AutoFlight {
    rand: Rand,
    wait: f64,
    airtime: f64,
    flight_length: f64,
    cooldown: f64,
}

impl AutoFlight {
    /// Un pilote de graine `seed` : mêmes décisions pour les mêmes entrées.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        let mut rand = Rand::new(seed);
        let wait = rand.range(14.0, 26.0);
        let flight_length = rand.range(20.0, 34.0);
        Self {
            rand,
            wait,
            airtime: 0.0,
            flight_length,
            cooldown: 0.0,
        }
    }

    /// Décide l'ordre de cette image, s'il y en a un.
    pub fn think(&mut self, flight: &Flight, c: &AutoContext<'_>) -> Option<FlightCommand> {
        self.cooldown = (self.cooldown - c.dt).max(0.0);
        if matches!(flight.phase(), FlightPhase::Deploying | FlightPhase::Retracting) {
            return None;
        }

        // Plus de musique : on se pose.
        if !c.playing {
            if flight.phase() == FlightPhase::Flying && self.cooldown <= 0.0 {
                self.cooldown = 1.1;
                return Some(FlightCommand::Down);
            }
            return None;
        }

        // Spectacle : les portes du niveau décident si l'on vole, jusqu'où et sur quels temps.
        let ceiling = c.show.map_or(i32::try_from(FLOOR_COUNT).unwrap_or(6), ceiling);
        if let Some(show) = c.show
            && (show.breathing || ceiling < 1 || flight.floor() > ceiling)
        {
            // Respiration, niveau sans vol, ou plafond rabaissé : on se pose (ou on descend d'un étage) et on ne
            // redécolle pas.
            self.airtime = 0.0;
            if flight.flying() && flight.ready() && self.cooldown <= 0.0 {
                self.cooldown = 1.1;
                return Some(FlightCommand::Down);
            }
            return None;
        }

        if flight.phase() == FlightPhase::Ground {
            // Plus la fréquence du niveau est basse, plus l'attente s'étire ; sur un drop seulement, on attend aussi
            // le drop.
            self.wait -= c.dt * c.show.map_or(1.0, |s| s.gates.flight_rate);
            if self.wait <= 0.0 && may_take_off(c.show) {
                self.wait = self.rand.range(26.0, 48.0);
                self.flight_length = self.rand.range(18.0, 34.0) * c.show.map_or(1.0, |s| 0.6 + 1.2 * s.heat);
                self.airtime = 0.0;
                self.cooldown = 0.5;
                return Some(FlightCommand::Up);
            }
            return None;
        }

        // En vol : on y reste un moment, on varie, puis on redescend.
        self.airtime += c.dt;
        if self.cooldown > 0.0 || !flight.ready() {
            return None;
        }
        self.cooldown = self.rand.range(2.6, 5.2) * c.show.map_or(1.0, |s| 1.2 - 0.4 * s.heat);
        if self.airtime > self.flight_length {
            self.cooldown = 1.2;
            return Some(FlightCommand::Down);
        }
        let roll = self.rand.unit();
        let tricks = c.show.is_none_or(|s| s.gates.tricks);
        if roll < 0.34 && tricks {
            Some(self.trick(flight, ceiling))
        } else if roll < 0.74 {
            let down = (self.rand.coin() && flight.floor() > 1) || flight.floor() >= ceiling;
            Some(if down { FlightCommand::Down } else { FlightCommand::Up })
        } else {
            None
        }
    }

    /// Une figure. Un salto change d'étage : arrière monte, avant descend ; on choisit celui qui reste dans les limites.
    ///
    /// Écart avec Sillage (voir « Écarts » de la PR) : les limites sont celles du plafond du moment, pas seulement du
    /// dernier étage. Là-bas un salto arrière au plafond le dépasse d'un étage, ce que la porte `max_floor` interdit.
    fn trick(&mut self, flight: &Flight, ceiling: i32) -> FlightCommand {
        let roll = if self.rand.coin() { -1 } else { 1 };
        if self.rand.unit() < 0.5 {
            return FlightCommand::Roll(roll);
        }
        let floor = flight.floor();
        if floor >= ceiling {
            // Au plafond : salto avant pour descendre, ou tonneau s'il n'y a pas d'étage dessous.
            if floor > 1 {
                FlightCommand::Flip(-1)
            } else {
                FlightCommand::Roll(roll)
            }
        } else if floor <= 1 {
            FlightCommand::Flip(1)
        } else {
            FlightCommand::Flip(if self.rand.coin() { 1 } else { -1 })
        }
    }
}

/// Étage le plus haut du moment : la porte du niveau, plus ou moins bas selon la chaleur du set. 0 : pas de vol.
#[must_use]
pub fn ceiling(show: &ShowInput) -> i32 {
    let max = i32::from(show.gates.max_floor).min(i32::try_from(FLOOR_COUNT).unwrap_or(6));
    if max < 1 || show.gates.flight_rate <= 0.0 {
        return 0;
    }
    ((1.0 + f64::from(max) * (0.4 + 0.6 * show.heat)).floor() as i32).clamp(1, max)
}

/// Le décollage est-il permis cette image ? Sur un drop seulement, selon le niveau.
fn may_take_off(show: Option<&ShowInput>) -> bool {
    show.is_none_or(|s| !s.gates.flight_on_drop_only || s.drop_now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pilot::input::PilotGates;

    fn show(max_floor: u8, flight_rate: f64, heat: f64) -> ShowInput {
        ShowInput {
            level: 5,
            heat,
            breathing: false,
            drop_now: false,
            time_scale: 1.0,
            gates: PilotGates {
                cruise: 1.0,
                flight_rate,
                max_floor,
                flight_on_drop_only: false,
                tricks: true,
            },
        }
    }

    #[test]
    fn le_plafond_suit_la_porte_et_la_chaleur() {
        assert_eq!(ceiling(&show(0, 1.0, 1.0)), 0, "niveau sans vol");
        assert_eq!(ceiling(&show(6, 0.0, 1.0)), 0, "fréquence nulle");
        assert_eq!(ceiling(&show(6, 1.0, 0.0)), 3, "1 + 6 × 0,4 = 3,4");
        assert_eq!(ceiling(&show(6, 1.0, 1.0)), 6);
        assert_eq!(ceiling(&show(2, 1.0, 1.0)), 2, "borné par la porte");
    }

    #[test]
    fn le_decollage_sur_drop_attend_le_drop() {
        let mut s = show(6, 4.0, 1.0);
        s.gates.flight_on_drop_only = true;
        assert!(!may_take_off(Some(&s)));
        s.drop_now = true;
        assert!(may_take_off(Some(&s)));
        assert!(may_take_off(None));
    }
}
