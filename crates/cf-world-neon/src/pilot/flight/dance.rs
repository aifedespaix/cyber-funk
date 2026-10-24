//! Danse en hauteur (`DanceFlight` de `flight/pilot.ts`), hors spectacle quand une danse est choisie :
//! - elle décolle sur un temps fort quand le morceau monte (groove soutenu ou sommet) et se pose quand il retombe ;
//! - tant qu'elle vole, elle change d'étage sur la première mesure d'un groupe de quatre temps, vers l'étage de sa
//!   figure ; plus le morceau est intense, plus haut elle monte ;
//! - un drop lui fait faire une vrille ; toutes les huit mesures, un salto (qui change aussi d'étage) ;
//! - chaque kick fait rebondir la caisse sur ses répulseurs, d'autant plus que les basses sont fortes
//!   (voir [`Flight::kick`](super::Flight::kick)).

use cf_analysis::Section;

use super::auto::FlightCommand;
use super::machine::Flight;
use super::types::FlightPhase;
use crate::pilot::dance::{DanceStyle, dance_floor};
use crate::route::FLOOR_COUNT;

/// Ce que la danse lit à chaque image.
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "des drapeaux d'image indépendants, comme le `BeatState` de Sillage"
)]
pub struct DanceContext {
    /// Durée de l'image, s.
    pub dt: f64,
    /// Un kick vient d'avoir lieu.
    pub kick: bool,
    /// Nombre de kicks depuis le début.
    pub count: u32,
    /// Section du morceau.
    pub section: Section,
    /// Énergie du morceau, 0 → 1.
    pub intensity: f64,
    /// Fin de morceau.
    pub outro: bool,
    /// Un drop vient de déclencher un boost (ou la section est un pic).
    pub drop: bool,
    /// Une musique joue.
    pub playing: bool,
    /// Style de danse.
    pub style: DanceStyle,
    /// Temps de danse continu (voir [`DanceClock`](crate::pilot::dance::DanceClock)) : les figures s'y calent.
    pub dance_beat: f64,
}

/// La danse en hauteur.
#[derive(Debug, Clone)]
pub struct DanceFlight {
    last_action: i64,
    roll_side: i8,
    landed_at: f64,
    was_flying: bool,
    clock: f64,
    last_flip: f64,
}

impl Default for DanceFlight {
    fn default() -> Self {
        Self {
            last_action: -1,
            roll_side: 1,
            landed_at: -100.0,
            was_flying: false,
            clock: 0.0,
            last_flip: -100.0,
        }
    }
}

impl DanceFlight {
    /// Décide l'ordre de cette image, s'il y en a un.
    pub fn think(&mut self, flight: &Flight, c: &DanceContext) -> Option<FlightCommand> {
        self.clock += c.dt;
        if flight.flying() {
            self.was_flying = true;
        } else {
            if self.was_flying {
                self.landed_at = self.clock;
            }
            self.was_flying = false;
        }
        if matches!(flight.phase(), FlightPhase::Deploying | FlightPhase::Retracting) {
            return None;
        }

        let bar = (c.dance_beat / 4.0).floor() as i64;
        let downbeat = c.kick && c.count.is_multiple_of(4);
        let lively = c.section == Section::Peak || (c.section == Section::Groove && c.intensity > 0.45);

        if !c.playing || c.outro || (c.section == Section::Calm && c.intensity < 0.3) {
            // Le morceau retombe : on se pose, sur un temps fort pour rester dans le tempo.
            if flight.phase() == FlightPhase::Flying && (downbeat || !c.playing) && flight.ready() {
                return Some(FlightCommand::Down);
            }
            return None;
        }

        if flight.phase() == FlightPhase::Ground {
            // Décollage sur un temps fort, une fois le calme retombé depuis un moment.
            if lively && downbeat && self.clock - self.landed_at > 10.0 {
                return Some(FlightCommand::Up);
            }
            return None;
        }

        if !flight.ready() {
            return None;
        }

        // Drop : une vrille, du côté qui alterne.
        if c.drop {
            self.roll_side = -self.roll_side;
            self.last_action = bar;
            return Some(FlightCommand::Roll(self.roll_side));
        }

        if !downbeat || bar == self.last_action {
            return None;
        }
        self.last_action = bar;
        let top = i32::try_from(FLOOR_COUNT).unwrap_or(6);
        // Toutes les huit mesures : un salto, dans le sens qui reste dans les limites.
        if bar.rem_euclid(8) == 7 && self.clock - self.last_flip > 6.0 {
            self.last_flip = self.clock;
            let direction = if flight.floor() >= top {
                -1
            } else if flight.floor() <= 1 || bar.rem_euclid(16) == 7 {
                1
            } else {
                -1
            };
            return Some(FlightCommand::Flip(direction));
        }
        // Sinon : un pas vers l'étage de la figure, plafonné par l'intensité du morceau.
        let cap = (2 + (c.intensity * 4.0).round() as i32).clamp(2, top);
        let wanted = cap.min(dance_floor(c.style, bar));
        if wanted > flight.floor() {
            Some(FlightCommand::Up)
        } else if wanted < flight.floor() && flight.floor() > 1 {
            Some(FlightCommand::Down)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(kick: bool, count: u32, beat: f64) -> DanceContext {
        DanceContext {
            dt: 1.0 / 60.0,
            kick,
            count,
            section: Section::Peak,
            intensity: 0.9,
            outro: false,
            drop: false,
            playing: true,
            style: DanceStyle::Sweep,
            dance_beat: beat,
        }
    }

    #[test]
    fn decolle_sur_un_temps_fort_quand_le_morceau_monte() {
        let mut dance = DanceFlight::default();
        let flight = Flight::default();
        for _ in 0..60 {
            assert_eq!(
                dance.think(&flight, &ctx(false, 1, 0.0)),
                None,
                "pas de temps fort, pas de décollage"
            );
        }
        assert_eq!(dance.think(&flight, &ctx(true, 5, 0.0)), None, "kick hors temps fort");
        assert_eq!(dance.think(&flight, &ctx(true, 4, 0.0)), Some(FlightCommand::Up));
    }

    #[test]
    fn au_calme_elle_ne_decolle_pas() {
        let mut dance = DanceFlight::default();
        let flight = Flight::default();
        let mut calm = ctx(true, 4, 0.0);
        calm.section = Section::Calm;
        calm.intensity = 0.1;
        assert_eq!(dance.think(&flight, &calm), None);
    }
}
