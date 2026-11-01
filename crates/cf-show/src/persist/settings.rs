//! Réglages du logiciel (périphériques, qualité, voiture, panneaux), relus au relancement.
//!
//! Distincts de la sauvegarde du spectacle : ils ne décrivent pas le show mais la machine et l'opérateur. La lecture est
//! tolérante par construction : un fichier absent, tronqué, d'une autre version ou dont un champ est invalide ne doit
//! jamais empêcher de démarrer (skill `live-safety`).

use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize};

use super::{PersistError, io::write_atomic};

/// Version du format. Un champ ajouté plus tard reste lisible grâce à `#[serde(default)]`.
pub const SETTINGS_VERSION: u32 = 1;

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
    /// Contient aussi, par réutilisation, les entrées `debug-open:*` et les curseurs des façades `tuning:*` (cf-render).
    #[serde(deserialize_with = "lenient")]
    pub collapsed_panels: Vec<String>,
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
