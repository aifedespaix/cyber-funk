//! Ce que le pilote lit chaque image : l'audio, le spectacle, la circulation. Des copies légères (`Copy`), extraites des
//! ressources Bevy par le système : toute la logique du pilote reste testable sans `App`.

use cf_analysis::{AudioFeatures, Section};
use cf_show::director::ShowState;

use super::flight::{Band, Obstruction};
use crate::route::PickupView;

/// L'audio de l'image (le `BeatState` de Sillage).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BeatInput {
    /// Une source livre du signal (`audio.live`) : sinon le monde ralentit en douceur.
    pub live: bool,
    /// Un kick vient d'avoir lieu (vrai une seule image).
    pub kick: bool,
    /// Volume global, 0..1.
    pub loud: f64,
    /// Énergie des basses, 0..1.
    pub bass: f64,
    /// Tempo détecté, battements par minute.
    pub bpm: Option<f64>,
    /// Énergie lissée du morceau, 0 (calme) → 1 (sommet) (`sections.intensity`).
    pub intensity: f64,
    /// Section du morceau.
    pub section: Section,
    /// Nombre de kicks depuis le début de l'analyse (jamais remis à zéro).
    pub count: u32,
}

impl Default for BeatInput {
    fn default() -> Self {
        Self::from(&AudioFeatures::NEUTRAL)
    }
}

impl From<&AudioFeatures> for BeatInput {
    fn from(f: &AudioFeatures) -> Self {
        Self {
            live: f.live,
            kick: f.kick,
            loud: f64::from(f.loud),
            bass: f64::from(f.bass),
            bpm: f.bpm.map(f64::from),
            intensity: f64::from(f.intensity),
            section: f.section,
            count: f.count,
        }
    }
}

/// Les portes du look que lit le pilote (`ShowGates` : `cruise`, `flightRate`, `maxFloor`, `flightOnDropOnly`, `tricks`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PilotGates {
    /// Multiplicateur d'allure.
    pub cruise: f64,
    /// Multiplicateur de la fréquence des envols, 0 (jamais) à 4.
    pub flight_rate: f64,
    /// Étage le plus haut permis, 0 (jamais de vol) à 6.
    pub max_floor: u8,
    /// Les envols n'ont lieu que sur un drop.
    pub flight_on_drop_only: bool,
    /// Figures permises.
    pub tricks: bool,
}

/// Le spectacle de l'image (`ShowState` de Sillage, ce que le pilote en lit). Absent (`None`) quand aucun spectacle ne
/// tourne : tout est alors permis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShowInput {
    /// Niveau d'intensité du look, 0 à 5.
    pub level: u8,
    /// Escalade lissée, 0 → 1.
    pub heat: f64,
    /// Mode respiration : on se pose, plans lents.
    pub breathing: bool,
    /// Le drop frappe maintenant (une image).
    pub drop_now: bool,
    /// Facteur de temps du monde (1 normal).
    pub time_scale: f64,
    /// Les portes du look.
    pub gates: PilotGates,
}

impl ShowInput {
    /// Extrait du `ShowState` de l'image.
    #[must_use]
    pub fn of(state: &ShowState) -> Self {
        let g = state.gates();
        Self {
            level: state.level,
            heat: f64::from(state.heat),
            breathing: state.breathing,
            drop_now: state.drop_now,
            time_scale: f64::from(state.time_scale),
            gates: PilotGates {
                cruise: f64::from(g.cruise),
                flight_rate: f64::from(g.flight_rate),
                max_floor: g.max_floor,
                flight_on_drop_only: g.flight_on_drop_only,
                tricks: g.tricks,
            },
        }
    }
}

/// La circulation que le pilote doit voir : écrite par le trafic (J5-08) avant le pilote, lue par lui. Tampons réutilisés
/// d'une image à l'autre.
#[derive(bevy::prelude::Resource, Debug, Clone, Default)]
pub struct PilotTraffic {
    /// Voitures au sol à dépasser, vues devant la voiture (`GroundTraffic.views`) : des malus pour le pilote.
    pub ground: Vec<PickupView>,
    /// Tranches d'altitude occupées par des véhicules volants proches (`TrafficDirector.airspace`).
    pub bands: Vec<Band>,
    /// Structures à esquiver en plus des arches (`TrafficDirector.obstructions`).
    pub obstructions: Vec<Obstruction>,
    /// Dépassements réussis depuis la dernière image : chacun donne un coup d'accélérateur au prochain kick.
    pub overtakes: u32,
}
