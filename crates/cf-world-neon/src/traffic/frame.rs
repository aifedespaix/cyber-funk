//! Ce que le trafic lit à chaque image : la voiture, l'audio, le spectacle. Des copies légères, extraites des ressources
//! Bevy par le système : toute la logique du trafic reste testable sans `App`.
//!
//! C'est le `TrafficContext` de Sillage (`context.ts`) moins ce qui est dans les ressources du trafic (flotte, brillances,
//! murs de lumière) : voir [`EventCtx`](super::EventCtx) pour ce que reçoit un événement.

use bevy::color::LinearRgba;
use bevy::math::DVec3;
use cf_analysis::{AudioFeatures, Section};
use cf_show::director::ShowState;
use cf_show::model::EventName as ShowEventName;

use super::names::EventName;

/// Marge (m) entre l'altitude d'un véhicule et celle de la voiture : au-delà, il ne passe jamais « sur le même étage »
/// (`FLOOR_MARGIN`).
pub const FLOOR_MARGIN_M: f32 = 6.5;

/// L'audio de l'image que lit le trafic (`BeatState`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrafficBeat {
    /// Un kick vient d'avoir lieu (vrai une seule image).
    pub kick: bool,
    /// Énergie des basses, 0..1.
    pub bass: f32,
    /// Énergie des médiums, 0..1.
    pub mid: f32,
    /// Énergie des aigus, 0..1.
    pub high: f32,
    /// Volume global, 0..1.
    pub loud: f32,
    /// Pouls du dernier kick, 1 → 0.
    pub pulse: f32,
    /// Nombre de kicks depuis le début de l'analyse.
    pub count: u32,
    /// Tempo détecté, battements par minute.
    pub bpm: Option<f32>,
}

impl TrafficBeat {
    /// Le silence : ni kick, ni énergie.
    pub const SILENT: Self = Self {
        kick: false,
        bass: 0.0,
        mid: 0.0,
        high: 0.0,
        loud: 0.0,
        pulse: 0.0,
        count: 0,
        bpm: None,
    };
}

impl From<&AudioFeatures> for TrafficBeat {
    fn from(f: &AudioFeatures) -> Self {
        Self {
            kick: f.kick,
            bass: f.bass,
            mid: f.mid,
            high: f.high,
            loud: f.loud,
            pulse: f.pulse,
            count: f.count,
            bpm: f.bpm,
        }
    }
}

/// La forme du morceau (`Drive` de `landmarks/types.ts`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrafficDrive {
    /// Le drop frappe maintenant (une image).
    pub drop: bool,
    /// Énergie lissée du morceau, 0 (calme) → 1 (sommet).
    pub intensity: f32,
    /// Section du morceau.
    pub section: Section,
}

impl TrafficDrive {
    /// Le départ de Sillage : `groove`, intensité 0,4.
    pub const START: Self = Self {
        drop: false,
        intensity: 0.4,
        section: Section::Groove,
    };
}

/// Le spectacle de l'image quand il est actif (`ShowState` de Sillage, ce que le trafic en lit). Hors spectacle, le
/// trafic n'a pas de portes : tout est permis sauf les événements du spectacle seul.
#[derive(Debug, Clone, Copy)]
pub struct TrafficShow<'a> {
    /// Niveau d'intensité du look, 0 à 5.
    pub level: u8,
    /// Mode respiration : seulement du lent et du doux.
    pub breathing: bool,
    /// Les flashs sont permis par le look (le limiteur s'applique ensuite quoi qu'il arrive).
    pub flashes: bool,
    /// Densité du trafic au sol, 0 à 1 (`gates.ground_traffic`).
    pub ground_traffic: f32,
    /// Multiplicateur de la fréquence des événements, 0 à 4 (`gates.event_rate`).
    pub event_rate: f32,
    /// Liste blanche des événements (`gates.events`).
    pub events: &'a [ShowEventName],
    /// Pulsation de la ville, 0 à 1 (`gates.fx.city_pulse`).
    pub city_pulse: f32,
    /// Il y a un nom de DJ ou d'événement à afficher : le titre holographique peut être tiré.
    pub titles: bool,
    /// Change à chaque changement de DJ (créneau).
    pub dj_seq: u32,
    /// Change à chaque changement de niveau.
    pub level_seq: u32,
}

impl<'a> TrafficShow<'a> {
    /// Extrait du `ShowState` de l'image. `level_seq` est compté par l'appelant (le `ShowState` ne le porte pas).
    #[must_use]
    pub fn of(state: &'a ShowState, level_seq: u32, titles: bool) -> Self {
        let gates = state.gates();
        Self {
            level: state.level,
            breathing: state.breathing,
            flashes: state.flashes,
            ground_traffic: gates.ground_traffic,
            event_rate: gates.event_rate,
            events: &gates.events,
            city_pulse: gates.fx.city_pulse,
            titles,
            dj_seq: state.slot_seq,
            level_seq,
        }
    }

    /// La liste blanche du look contient-elle cet événement ?
    #[must_use]
    pub fn allows(&self, name: EventName) -> bool {
        self.events.iter().any(|e| e.as_str() == name.as_str())
    }
}

/// Tout ce que le trafic lit à l'image : la voiture, l'audio, le spectacle.
#[derive(Debug, Clone, Copy)]
pub struct TrafficFrame<'a> {
    /// Durée de l'image, s.
    pub dt: f32,
    /// Horloge de la scène, s.
    pub time: f64,
    /// Position absolue de la voiture, altitude comprise.
    pub car: DVec3,
    /// Altitude de la voiture, m, et celle de l'étage qu'elle vise : au sol, les deux valent 0. Les véhicules évitent
    /// ces hauteurs.
    pub altitude: f32,
    /// Altitude de l'étage visé, m.
    pub target: f32,
    /// Vitesse de la voiture, m/s.
    pub speed: f32,
    /// L'audio.
    pub beat: TrafficBeat,
    /// La forme du morceau.
    pub drive: TrafficDrive,
    /// Horloge de danse en temps musicaux : une mesure (quatre temps) commence quand `floor(dance_beat / 4)` change.
    pub dance_beat: f64,
    /// Le monde avance (pas de pause, un signal arrive).
    pub playing: bool,
    /// Mode « réduire les animations » : lent et grand seulement.
    pub calm: bool,
    /// Le spectacle, s'il est actif.
    pub show: Option<TrafficShow<'a>>,
    /// Le cycle de teintes prismatiques de la palette, pour colorer les véhicules.
    pub cycle: [LinearRgba; 6],
    /// Densité du trafic voulue par la qualité adaptative, 0 → 1 (`QualityLevers::particle_density`).
    pub density: f32,
}

impl TrafficFrame<'_> {
    /// Une teinte du cycle (l'indice boucle) (`TrafficDirector.tint`).
    #[must_use]
    pub fn tint(&self, index: i64) -> LinearRgba {
        self.cycle[index.unsigned_abs() as usize % self.cycle.len()]
    }

    /// Vrai si une hauteur `y` est libre de la voiture : ni son altitude actuelle, ni celle de l'étage qu'elle vise
    /// (`isFree`).
    #[must_use]
    pub fn is_free(&self, y: f32, margin: f32) -> bool {
        if self.altitude < 1.5 && self.target < 1.5 {
            return true;
        }
        (y - self.altitude).abs() > margin && (y - self.target).abs() > margin
    }
}

impl Default for TrafficFrame<'_> {
    fn default() -> Self {
        Self {
            dt: 1.0 / 60.0,
            time: 0.0,
            car: DVec3::ZERO,
            altitude: 0.0,
            target: 0.0,
            speed: 0.0,
            beat: TrafficBeat::SILENT,
            drive: TrafficDrive::START,
            dance_beat: 0.0,
            playing: true,
            calm: false,
            show: None,
            cycle: [LinearRgba::WHITE; 6],
            density: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn au_sol_toutes_les_hauteurs_sont_libres() {
        let frame = TrafficFrame::default();
        assert!(frame.is_free(0.0, FLOOR_MARGIN_M));
        assert!(frame.is_free(15.0, FLOOR_MARGIN_M));
    }

    #[test]
    fn en_vol_l_etage_de_la_voiture_et_celui_qu_elle_vise_sont_pris() {
        let frame = TrafficFrame {
            altitude: 15.0,
            target: 21.0,
            ..TrafficFrame::default()
        };
        assert!(!frame.is_free(15.0, FLOOR_MARGIN_M));
        assert!(!frame.is_free(21.0, FLOOR_MARGIN_M));
        assert!(frame.is_free(38.5, FLOOR_MARGIN_M));
    }

    #[test]
    fn l_indice_de_teinte_boucle_dans_les_deux_sens() {
        let mut cycle = [LinearRgba::BLACK; 6];
        cycle[1] = LinearRgba::WHITE;
        let frame = TrafficFrame {
            cycle,
            ..TrafficFrame::default()
        };
        assert_eq!(frame.tint(1), LinearRgba::WHITE);
        assert_eq!(frame.tint(7), LinearRgba::WHITE);
        assert_eq!(frame.tint(-1), LinearRgba::WHITE);
        assert_eq!(frame.tint(0), LinearRgba::BLACK);
    }

    #[test]
    fn la_liste_blanche_se_lit_par_slug() {
        let events = [
            ShowEventName::new("chase").expect("slug"),
            ShowEventName::new("city-pulse").expect("slug"),
        ];
        let show = TrafficShow {
            level: 3,
            breathing: false,
            flashes: true,
            ground_traffic: 0.5,
            event_rate: 1.0,
            events: &events,
            city_pulse: 0.6,
            titles: false,
            dj_seq: 0,
            level_seq: 0,
        };
        assert!(show.allows(EventName::Chase));
        assert!(show.allows(EventName::CityPulse));
        assert!(!show.allows(EventName::Storm));
    }
}
