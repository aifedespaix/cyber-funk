//! Réglages du logiciel (périphériques, qualité, voiture, panneaux), relus au relancement.
//!
//! Distincts de la sauvegarde du spectacle : ils ne décrivent pas le show mais la machine et l'opérateur. La lecture est
//! tolérante par construction : un fichier absent, tronqué, d'une autre version ou dont un champ est invalide ne doit
//! jamais empêcher de démarrer (skill `live-safety`).

use std::collections::BTreeSet;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use super::{PersistError, io::write_atomic};

/// Version du format. Un champ ajouté plus tard reste lisible grâce à `#[serde(default)]`.
///
/// Version 2 : ajout du bloc [`OperatorPrefs`]. Pas de migration : un fichier de version 1 se lit avec les défauts.
pub const SETTINGS_VERSION: u32 = 2;

/// La voiture que l'opérateur a choisie.
///
/// Écrite en texte (`"dirty"`, `"cyber"`) : une variante inconnue d'une autre version se lit comme la voiture par
/// défaut au lieu de faire échouer la lecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum CarChoice {
    /// La « dirty car » (volant à gauche), la voiture actuelle.
    #[default]
    Dirty,
    /// La `CyberCar` d'origine de Sillage.
    Cyber,
}

impl From<String> for CarChoice {
    fn from(text: String) -> Self {
        match text.as_str() {
            "cyber" => Self::Cyber,
            _ => Self::Dirty,
        }
    }
}

impl From<CarChoice> for String {
    fn from(choice: CarChoice) -> Self {
        match choice {
            CarChoice::Dirty => "dirty".into(),
            CarChoice::Cyber => "cyber".into(),
        }
    }
}

/// Ce que la fenêtre « Debug monde » (F4, `cf-operator`) retient de sa présentation.
///
/// Seulement la présentation : le gel de l'automatique, le quartier forcé et les choix en cours du panneau ne sont
/// jamais sauvegardés (outil de debug, chaque session repart de l'automatique). On retient les exceptions à l'état
/// par défaut : les sections sont ouvertes par défaut, les familles d'événements fermées.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DebugPanelPrefs {
    /// Sections repliées par l'opérateur (identifiants stables choisis par `cf-operator`, ex. `"districts"`).
    #[serde(deserialize_with = "lenient")]
    pub closed_sections: BTreeSet<String>,
    /// Familles d'événements ouvertes (nom de famille du catalogue, ex. `"Trafic"`).
    #[serde(deserialize_with = "lenient")]
    pub open_families: BTreeSet<String>,
}

/// Préférences de la fenêtre opérateur (onglets, événements épinglés, écran de sortie, profil actif).
///
/// Chaque champ est lu de façon tolérante : une valeur de mauvais type ne fait perdre que ce champ-là.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OperatorPrefs {
    /// Dernier onglet ouvert (`"settings"`, `"stats"` ou `"debug"`).
    #[serde(deserialize_with = "lenient")]
    pub last_tab: String,
    /// Événements épinglés, par identifiant (`WorldEvent::id`).
    #[serde(deserialize_with = "lenient")]
    pub pinned_events: BTreeSet<String>,
    /// Écran de sortie du projecteur (nom de l'écran), `None` : écran par défaut.
    #[serde(deserialize_with = "lenient")]
    pub output_monitor: Option<String>,
    /// Sortie projecteur en plein écran.
    #[serde(deserialize_with = "lenient")]
    pub fullscreen: bool,
    /// Profil de contrôle actif (fichier d'événement de l'opérateur).
    #[serde(deserialize_with = "lenient")]
    pub active_profile: Option<String>,
    /// Dernier fichier d'événement ouvert ou enregistré par l'opérateur.
    #[serde(deserialize_with = "lenient")]
    pub last_event_path: Option<String>,
}

impl Default for OperatorPrefs {
    fn default() -> Self {
        Self {
            last_tab: "settings".into(),
            pinned_events: BTreeSet::new(),
            output_monitor: None,
            fullscreen: true,
            active_profile: None,
            last_event_path: None,
        }
    }
}

/// Ce que le logiciel retient d'un lancement à l'autre.
///
/// Les périphériques sont gardés par leur nom : un nom absent ce jour-là reste écrit (la prise reviendra), l'appelant
/// retombe seulement sur la source par défaut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// Version du format.
    #[serde(deserialize_with = "lenient")]
    pub version: u32,
    /// Dernière source audio (`"Microphone"`, `"Line"`, `"Demo"`).
    #[serde(deserialize_with = "lenient")]
    pub audio_source: Option<String>,
    /// Périphérique du micro.
    #[serde(deserialize_with = "lenient")]
    pub mic_device: Option<String>,
    /// Périphérique de l'entrée ligne.
    #[serde(deserialize_with = "lenient")]
    pub line_device: Option<String>,
    /// Ports MIDI d'entrée actifs.
    #[serde(deserialize_with = "lenient")]
    pub midi_inputs: Vec<String>,
    /// Niveau de qualité fixé par l'opérateur (`None` : automatique).
    #[serde(deserialize_with = "lenient")]
    pub quality: Option<String>,
    /// Voiture choisie.
    #[serde(deserialize_with = "lenient")]
    pub car: CarChoice,
    /// Panneaux de la fenêtre opérateur dont l'état est retenu (familles de l'onglet debug ouvertes, par exemple).
    /// Contient aussi, par réutilisation, les curseurs des façades `tuning:*` (cf-render) et, dans les fichiers écrits
    /// avant [`DebugPanelPrefs`], les familles ouvertes `debug-open:*` (migrées par `cf-operator` au premier geste).
    #[serde(deserialize_with = "lenient")]
    pub collapsed_panels: Vec<String>,
    /// Présentation de la fenêtre « Debug monde ». Absent des fichiers écrits avant son ajout : valeur par défaut, et
    /// les anciennes entrées `debug-open:*` de `collapsed_panels` restent comprises (`cf-operator`).
    #[serde(deserialize_with = "lenient")]
    pub debug_panel: DebugPanelPrefs,
    /// Préférences de la fenêtre opérateur. Absent des fichiers de version 1 : valeurs par défaut.
    #[serde(deserialize_with = "lenient")]
    pub operator: OperatorPrefs,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            audio_source: None,
            mic_device: None,
            line_device: None,
            midi_inputs: Vec::new(),
            quality: None,
            car: CarChoice::default(),
            collapsed_panels: Vec::new(),
            debug_panel: DebugPanelPrefs::default(),
            operator: OperatorPrefs::default(),
        }
    }
}

/// Un champ invalide (mauvais type, variante inconnue) prend sa valeur par défaut au lieu de faire perdre le fichier.
fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = ron::Value::deserialize(deserializer)?;
    Ok(value.into_rust::<T>().unwrap_or_default())
}

impl AppSettings {
    /// Relit les réglages. Ne renvoie jamais d'erreur : fichier absent, illisible ou invalide donnent les défauts.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        Self::load_reporting(path).0
    }

    /// Comme [`load`](Self::load), avec en plus de quoi journaliser : `Some(message)` quand le fichier existait mais
    /// n'a pas pu être lu (un fichier absent n'est pas une anomalie).
    #[must_use]
    pub fn load_reporting(path: &Path) -> (Self, Option<String>) {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (Self::default(), None),
            Err(e) => {
                return (
                    Self::default(),
                    Some(format!("{} : lecture impossible : {e}", path.display())),
                );
            }
        };
        match ron::from_str::<Self>(&text) {
            Ok(settings) => (settings, None),
            Err(e) => (
                Self::default(),
                Some(format!(
                    "{} : réglages illisibles, valeurs par défaut : {e}",
                    path.display()
                )),
            ),
        }
    }

    /// Écrit les réglages atomiquement et crée le dossier parent au besoin.
    ///
    /// # Errors
    ///
    /// [`PersistError::Serialize`] ou [`PersistError::Io`] ; l'ancien fichier reste alors intact.
    pub fn save(&self, path: &Path) -> Result<(), PersistError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| PersistError::Io {
                path: parent.to_path_buf(),
                action: "création du dossier",
                source,
            })?;
        }
        let text = ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default())?;
        write_atomic(path, text.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn v1_file_still_loads_with_default_operator_prefs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.ron");
        // Fichier écrit par la version 1 : aucun bloc `operator`.
        fs::write(&path, "(version: 1, car: \"cyber\", mic_device: Some(\"Micro USB\"))").unwrap();
        let loaded = AppSettings::load(&path);
        assert_eq!(loaded.operator, OperatorPrefs::default());
        assert_eq!(loaded.car, CarChoice::Cyber);
        assert_eq!(loaded.mic_device.as_deref(), Some("Micro USB"));
    }

    #[test]
    fn operator_prefs_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.ron");
        let settings = AppSettings {
            operator: OperatorPrefs {
                last_tab: "debug".into(),
                pinned_events: BTreeSet::from(["alpha".to_string(), "beta".to_string()]),
                output_monitor: Some("DP-1".into()),
                fullscreen: false,
                active_profile: Some("Soirée".into()),
                last_event_path: Some("assets/shows/demo.ron".into()),
            },
            ..AppSettings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(AppSettings::load(&path), settings);
    }

    #[test]
    fn invalid_operator_field_falls_back_to_default_without_losing_others() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.ron");
        fs::write(
            &path,
            "(car: \"cyber\", operator: (pinned_events: 12, last_tab: \"stats\"))",
        )
        .unwrap();
        let loaded = AppSettings::load(&path);
        assert_eq!(loaded.car, CarChoice::Cyber);
        assert!(loaded.operator.pinned_events.is_empty());
        assert_eq!(loaded.operator.last_tab, "stats");
    }
}
