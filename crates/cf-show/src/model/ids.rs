//! Identifiants forts et petites valeurs typées du format de show.
//!
//! Toutes les références entre fichiers passent par un identifiant stable (un slug), jamais par un index : on peut
//! réordonner les cues, insérer un créneau ou renommer un look affiché sans casser une référence. La syntaxe est
//! vérifiée dès la lecture du fichier (l'erreur porte alors la ligne et le chemin du champ) ; l'existence de la
//! cible est vérifiée ensuite par `validate`.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Longueur maximale d'un identifiant, en caractères.
pub const MAX_ID_LEN: usize = 64;

/// Longueur maximale d'un chemin d'asset, en caractères.
pub const MAX_ASSET_PATH_LEN: usize = 255;

/// Une valeur typée refusée à la lecture : ce qu'on attendait, ce qu'on a lu, pourquoi.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} invalide « {value} » : {reason}")]
pub struct InvalidValue {
    /// Nature de la valeur (« identifiant de look », « couleur »…).
    pub kind: &'static str,
    /// Texte lu dans le fichier.
    pub value: String,
    /// Règle enfreinte, en clair.
    pub reason: &'static str,
}

/// Vérifie qu'une chaîne est un slug : `a-z`, `0-9` et tirets, sans tiret au début, à la fin ni doublé.
///
/// # Errors
///
/// Renvoie la règle enfreinte, en clair.
pub fn check_slug(text: &str) -> Result<(), &'static str> {
    if text.is_empty() {
        return Err("ne doit pas être vide");
    }
    if text.chars().count() > MAX_ID_LEN {
        return Err("64 caractères au plus");
    }
    if !text
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err("seuls a-z, 0-9 et « - » sont permis (slug en minuscules)");
    }
    if text.starts_with('-') || text.ends_with('-') || text.contains("--") {
        return Err("pas de tiret au début, à la fin ni doublé");
    }
    Ok(())
}

/// Déclare un identifiant fort : un slug vérifié à la construction, sérialisé comme une simple chaîne.
macro_rules! slug_id {
    ($(#[$meta:meta])* $name:ident, $kind:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            /// Construit l'identifiant après vérification de la syntaxe (slug, voir [`check_slug`]).
            ///
            /// # Errors
            ///
            /// Refuse une chaîne qui n'est pas un slug.
            pub fn new(text: impl Into<String>) -> Result<Self, InvalidValue> {
                let text = text.into();
                match check_slug(&text) {
                    Ok(()) => Ok(Self(text)),
                    Err(reason) => Err(InvalidValue { kind: $kind, value: text, reason }),
                }
            }

            /// Le slug, tel qu'écrit dans les fichiers.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = InvalidValue;

            fn try_from(text: String) -> Result<Self, Self::Error> {
                Self::new(text)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

slug_id!(
    /// Identifiant stable d'un look (`"niveau-3-envol"`). Pour un look de la bibliothèque, c'est aussi le nom du
    /// fichier sans extension (`assets/looks/niveau-3-envol.ron`).
    LookId,
    "identifiant de look"
);
slug_id!(
    /// Identifiant stable d'un créneau DJ dans un événement (`"krmn"`). Unique dans l'événement.
    SlotId,
    "identifiant de créneau"
);
slug_id!(
    /// Identifiant stable d'un cue dans la liste de son créneau (`"cosmos"`). Unique dans le créneau.
    CueId,
    "identifiant de cue"
);
slug_id!(
    /// Profil de contrôle (mapping MIDI/clavier) de `assets/controllers/<id>.ron`, défini par `cf-control`.
    ControlProfileId,
    "identifiant de profil de contrôle"
);
slug_id!(
    /// Monde 3D dans lequel un look se joue (`"neon"` : le monde de Sillage). Liste close validée par le rendu.
    WorldId,
    "identifiant de monde"
);
slug_id!(
    /// Rang de plan caméra de Sillage (`"common"`, `"uncommon"`, `"rare"`, `"epic"`). La liste close sera vérifiée
    /// par le réalisateur (J6) ; ici, seule la syntaxe l'est.
    ShotTier,
    "rang de plan"
);
slug_id!(
    /// Identifiant d'un plan du catalogue caméra de Sillage (`"ringworld"`, `"top-down"`…). Vérifié en J6.
    ShotId,
    "identifiant de plan"
);
slug_id!(
    /// Rang de cascade de Sillage (`"common"`, `"uncommon"`, `"rare"`). Liste close vérifiée en J7.
    StuntTier,
    "rang de cascade"
);
slug_id!(
    /// Cascade de Sillage (`"off-road"`, `"slalom"`, `"wall-ride"`…). Liste close vérifiée en J7.
    StuntKind,
    "cascade"
);
slug_id!(
    /// Événement du trafic de Sillage (`"chase"`, `"storm"`, `"city-pulse"`…). Liste close vérifiée en J7.
    EventName,
    "événement du trafic"
);

/// Couleur `#rrggbb`, ramenée en minuscules à la lecture (comme `normalizeHex` de Sillage, sans la forme courte).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct HexColor(String);

impl HexColor {
    /// Construit la couleur après vérification.
    ///
    /// # Errors
    ///
    /// Refuse tout ce qui n'est pas `#` suivi de six chiffres hexadécimaux.
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidValue> {
        let text = text.into();
        let valid = text.len() == 7 && text.starts_with('#') && text.bytes().skip(1).all(|b| b.is_ascii_hexdigit());
        if valid {
            Ok(Self(text.to_ascii_lowercase()))
        } else {
            Err(InvalidValue {
                kind: "couleur",
                value: text,
                reason: "format attendu : « #rrggbb »",
            })
        }
    }

    /// La couleur, `#rrggbb` en minuscules.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for HexColor {
    type Error = InvalidValue;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(text)
    }
}

impl From<HexColor> for String {
    fn from(color: HexColor) -> Self {
        color.0
    }
}

/// Heure d'horloge locale `"HH:MM"` (00:00 à 23:59), pour les horaires prévus et l'avancée « à une heure ».
///
/// Pas de date : une heure plus petite que la précédente s'entend le lendemain (un set qui passe minuit).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ClockTime {
    minutes: u16,
}

impl ClockTime {
    /// Construit une heure à partir des heures (0 à 23) et des minutes (0 à 59) ; `None` hors plage.
    #[must_use]
    pub fn from_hm(hours: u8, minutes: u8) -> Option<Self> {
        (hours < 24 && minutes < 60).then(|| Self {
            minutes: u16::from(hours) * 60 + u16::from(minutes),
        })
    }

    /// Minutes écoulées depuis minuit, 0 à 1439.
    #[must_use]
    pub fn minutes_since_midnight(self) -> u16 {
        self.minutes
    }
}

impl TryFrom<String> for ClockTime {
    type Error = InvalidValue;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let parsed = match text.split_once(':') {
            Some((h, m)) if h.len() == 2 && m.len() == 2 => match (h.parse::<u8>(), m.parse::<u8>()) {
                (Ok(h), Ok(m)) => Self::from_hm(h, m),
                _ => None,
            },
            _ => None,
        };
        parsed.ok_or(InvalidValue {
            kind: "heure",
            value: text,
            reason: "format attendu : « HH:MM », de 00:00 à 23:59",
        })
    }
}

impl From<ClockTime> for String {
    fn from(time: ClockTime) -> Self {
        time.to_string()
    }
}

impl fmt::Display for ClockTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.minutes / 60, self.minutes % 60)
    }
}

/// Chemin d'un fichier sous `assets/` (`"logos/krmn.png"`) : relatif, séparateur `/`, sans `..`.
///
/// Le fichier est préchargé à l'ouverture de l'événement (jamais pendant le show) ; son existence est vérifiée à ce
/// moment-là par le rendu, pas ici.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct AssetPath(String);

impl AssetPath {
    /// Construit le chemin après vérification.
    ///
    /// # Errors
    ///
    /// Refuse un chemin vide, absolu, avec `\`, `..` ou un segment vide, ou trop long.
    pub fn new(text: impl Into<String>) -> Result<Self, InvalidValue> {
        let text = text.into();
        let reason = if text.is_empty() {
            Some("ne doit pas être vide")
        } else if text.chars().count() > MAX_ASSET_PATH_LEN {
            Some("255 caractères au plus")
        } else if text.contains('\\') {
            Some("séparateur « / » uniquement")
        } else if text.starts_with('/') || text.contains(':') {
            Some("chemin relatif au dossier assets/ attendu")
        } else if text
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
        {
            Some("pas de segment vide, « . » ni « .. »")
        } else {
            None
        };
        match reason {
            None => Ok(Self(text)),
            Some(reason) => Err(InvalidValue {
                kind: "chemin d'asset",
                value: text,
                reason,
            }),
        }
    }

    /// Le chemin relatif à `assets/`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for AssetPath {
    type Error = InvalidValue;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        Self::new(text)
    }
}

impl From<AssetPath> for String {
    fn from(path: AssetPath) -> Self {
        path.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert!(LookId::new("niveau-3-envol").is_ok());
        assert!(LookId::new("a").is_ok());
        for bad in ["", "Majuscule", "a--b", "-a", "a-", "é", "a b", "a_b"] {
            assert!(LookId::new(bad).is_err(), "{bad:?} devrait être refusé");
        }
        assert!(LookId::new("a".repeat(MAX_ID_LEN)).is_ok());
        assert!(LookId::new("a".repeat(MAX_ID_LEN + 1)).is_err());
    }

    #[test]
    fn colors_are_lowercased() {
        assert_eq!(HexColor::new("#AbCdEf").map(String::from), Ok("#abcdef".to_owned()));
        for bad in ["abcdef", "#abc", "#abcdeg", "#abcdef0"] {
            assert!(HexColor::new(bad).is_err(), "{bad:?} devrait être refusé");
        }
    }

    #[test]
    fn clock_times() {
        let time = ClockTime::try_from("23:05".to_owned());
        assert_eq!(time.map(String::from), Ok("23:05".to_owned()));
        for bad in ["24:00", "7:30", "12:60", "1230", "ab:cd"] {
            assert!(
                ClockTime::try_from(bad.to_owned()).is_err(),
                "{bad:?} devrait être refusé"
            );
        }
    }

    #[test]
    fn asset_paths() {
        assert!(AssetPath::new("logos/krmn.png").is_ok());
        for bad in ["", "/etc/passwd", "../secret", "a//b", "a\\b", "C:/x", "a/./b"] {
            assert!(AssetPath::new(bad).is_err(), "{bad:?} devrait être refusé");
        }
    }
}
