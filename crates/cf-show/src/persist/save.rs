//! La sauvegarde d'état : ce qu'il faut pour reprendre un set après un plantage ou une fermeture.

use serde::{Deserialize, Serialize};

use super::PersistError;
use crate::director::{MACROS, ShowState};
use crate::load::{LoadError, check_version, deserialize};

/// Version du format de la sauvegarde (indépendante de celle des événements).
pub const SAVE_VERSION: u32 = 1;
/// Fenêtre de reprise (s) : 15 min, comme `RESUME_WINDOW_MS` de Sillage.
pub const RESUME_WINDOW_S: f64 = 15.0 * 60.0;
/// Intervalle de sauvegarde (s) : 5 s (10 s dans Sillage) pour qu'un plantage coûte moins de temps de set.
pub const SAVE_EVERY_S: f32 = 5.0;

/// Position d'un potard de macro.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedKnob {
    /// Position 0..1.
    pub value: f32,
    /// Le potard a été touché depuis l'entrée du look.
    pub engaged: bool,
}

/// État sauvegardé d'un set en cours.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShowSave {
    /// Version du format, en tête du fichier.
    pub version: u32,
    /// Nom de l'événement : une sauvegarde d'un autre événement ne se reprend pas.
    pub event_name: String,
    /// Heure murale de la sauvegarde, en secondes Unix.
    pub saved_at_s: f64,
    /// Graine de tout l'aléatoire du spectacle.
    pub seed: u64,
    /// Rang du créneau DJ en cours.
    pub slot: usize,
    /// Rang du cue à l'écran dans son créneau.
    pub cue: usize,
    /// Secondes depuis l'appel du cue (son chronomètre).
    pub cue_elapsed_s: f64,
    /// Secondes depuis le chargement du créneau.
    pub elapsed_s: f64,
    /// Créneau armé pour le prochain `NextDj`.
    pub armed_slot: Option<usize>,
    /// Potards de macro.
    pub macros: Vec<SavedKnob>,
}

impl ShowSave {
    /// Photographie l'état du spectacle (pure, sans I/O ; une petite allocation, toutes les 5 s seulement).
    #[must_use]
    pub fn capture(event_name: &str, state: &ShowState, seed: u64, wall_now_s: f64) -> Self {
        Self {
            version: SAVE_VERSION,
            event_name: event_name.to_owned(),
            saved_at_s: wall_now_s,
            seed,
            slot: state.slot,
            cue: state.cue,
            cue_elapsed_s: state.cue_elapsed,
            elapsed_s: state.elapsed,
            armed_slot: state.armed_slot,
            macros: state
                .macros
                .iter()
                .map(|k| SavedKnob {
                    value: k.value,
                    engaged: k.engaged,
                })
                .collect(),
        }
    }

    /// Vrai si la sauvegarde a moins de [`RESUME_WINDOW_S`] et n'est pas datée du futur (âge négatif : horloge
    /// reculée, on ne s'y fie pas).
    #[must_use]
    pub fn is_resumable(&self, wall_now_s: f64) -> bool {
        let age = wall_now_s - self.saved_at_s;
        age.is_finite() && (0.0..=RESUME_WINDOW_S).contains(&age)
    }

    /// Écrit la sauvegarde en RON.
    ///
    /// # Errors
    ///
    /// Échec du sérialiseur (ne devrait pas arriver).
    pub fn to_ron(&self) -> Result<String, ron::Error> {
        let pretty = ron::ser::PrettyConfig::new().struct_names(true);
        ron::ser::to_string_pretty(self, pretty)
    }

    /// Relit une sauvegarde ; `origin` nomme la source dans les messages. Rejette les valeurs impossibles (temps
    /// négatif ou non fini, potard hors 0..1, trop de potards) comme `readSave` de Sillage.
    ///
    /// # Errors
    ///
    /// [`PersistError::Corrupt`] : RON invalide, version inconnue ou valeurs hors plage.
    pub fn from_ron(text: &str, origin: &str) -> Result<Self, PersistError> {
        let save: Self = deserialize(text, origin).map_err(PersistError::Corrupt)?;
        // Seule la version 1 existe : une migration s'ajouterait ici, comme pour les événements.
        if check_version(Some(save.version), origin).map_err(PersistError::Corrupt)? != SAVE_VERSION
            || save.version != SAVE_VERSION
        {
            return Err(PersistError::Corrupt(LoadError::UnknownVersion {
                origin: origin.to_owned(),
                found: save.version,
            }));
        }
        let times_ok = [save.saved_at_s, save.cue_elapsed_s, save.elapsed_s]
            .iter()
            .all(|t| t.is_finite() && *t >= 0.0);
        let knobs_ok = save.macros.len() <= MACROS && save.macros.iter().all(|k| (0.0..=1.0).contains(&k.value));
        if times_ok && knobs_ok {
            Ok(save)
        } else {
            Err(PersistError::Corrupt(LoadError::Syntax {
                origin: origin.to_owned(),
                field: None,
                line: 1,
                column: 1,
                message: "valeurs hors plage (temps, potards)".to_owned(),
            }))
        }
    }
}

/// Compte à rebours de la sauvegarde périodique (`SavePacer` de Sillage).
#[derive(Debug, Clone, Copy)]
pub struct SavePacer {
    left_s: f32,
}

impl Default for SavePacer {
    fn default() -> Self {
        Self { left_s: SAVE_EVERY_S }
    }
}

impl SavePacer {
    /// Repart de zéro (changement d'événement, reprise).
    pub fn reset(&mut self) {
        self.left_s = SAVE_EVERY_S;
    }

    /// Vrai quand il est temps d'enregistrer (une fois toutes les [`SAVE_EVERY_S`] secondes de `dt` cumulé).
    pub fn tick(&mut self, dt: f32) -> bool {
        self.left_s -= if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        if self.left_s > 0.0 {
            return false;
        }
        self.left_s = SAVE_EVERY_S;
        true
    }
}
