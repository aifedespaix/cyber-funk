//! Le cue : une ligne de la conduite d'un créneau (modèle des listes de cues du spectacle vivant).
//!
//! Un cue ne scénarise pas l'image : il choisit un look (les portes), dit comment on y entre, ce qu'on déclenche à
//! l'entrée et quand on passe au suivant. Le chef du spectacle (J2-05) interprète ces règles selon le mode du créneau.

use serde::{Deserialize, Serialize};

use super::ids::{ClockTime, CueId, EventName, LookId};
use super::overrides::LookOverrides;

/// Longueur maximale du nom affiché d'un cue, en caractères.
pub const MAX_CUE_NAME: usize = 60;
/// Longueur maximale d'un carton-titre, en caractères (`MAX_EVENT_NAME` de Sillage).
pub const MAX_TITLE: usize = 60;
/// Fondu d'entrée le plus long, en mesures.
pub const MAX_FADE_BARS: u8 = 32;
/// Avancée « après N mesures » la plus longue, en mesures (≈ 32 min à 128 BPM).
pub const MAX_ADVANCE_BARS: u16 = 1024;
/// Avancée « après une durée » la plus longue, en secondes (12 h : la durée maximale d'un set dans Sillage).
pub const MAX_ADVANCE_SECONDS: f64 = 43_200.0;

/// Une ligne de la conduite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cue {
    /// Identifiant stable, unique dans le créneau (sauvegarde, reprise, interface).
    pub id: CueId,
    /// Nom affiché dans la liste de cues, 1 à [`MAX_CUE_NAME`] caractères.
    pub name: String,
    /// Look joué : un look de l'événement ou de la bibliothèque (ceux de l'événement priment).
    pub look: LookId,
    /// Surcharges du look pour ce cue seulement. Absent : le look tel quel.
    #[serde(default, skip_serializing_if = "LookOverrides::is_empty")]
    pub overrides: LookOverrides,
    /// Comment on entre dans ce cue une fois qu'il est dû.
    pub entry: CueEntry,
    /// Ordres ponctuels lancés à l'entrée du cue, dans l'ordre. Absent : aucun.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
    /// Quand le cue suivant devient dû (en mode Manuel, seul GO compte).
    pub advance: CueAdvance,
}

/// Comment on entre dans un cue, une fois qu'il est dû (par GO ou par la règle d'avancée du cue précédent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CueEntry {
    /// Coupe franche, sur le prochain temps fort.
    Cut,
    /// Fondu des nombres des portes, des couches et de la palette ; les listes basculent à mi-chemin.
    Fade {
        /// Durée du fondu, en mesures : 1 à [`MAX_FADE_BARS`].
        bars: u8,
    },
    /// Attend le prochain drop pour couper (avec le délai maximal de Sillage, puis la phrase suivante).
    OnDrop,
    /// Attend le début de la prochaine phrase (16 temps) pour couper.
    OnPhrase,
}

/// Quand le cue suivant devient dû. Le chronomètre part à l'entrée du cue.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CueAdvance {
    /// Seulement sur GO de l'opérateur (ou une note MIDI du DJ).
    Manual,
    /// Après un nombre de mesures depuis l'entrée du cue.
    Bars {
        /// Nombre de mesures (4 temps) : 1 à [`MAX_ADVANCE_BARS`].
        bars: u16,
    },
    /// Au prochain drop détecté (ou forcé).
    OnDrop,
    /// À une heure d'horloge locale (la prochaine occurrence de cette heure).
    At {
        /// Heure locale `"HH:MM"`.
        time: ClockTime,
    },
    /// Après une durée depuis l'entrée du cue.
    After {
        /// Durée, en secondes : plus de 0 et au plus [`MAX_ADVANCE_SECONDS`].
        seconds: f64,
    },
}

/// Ordre ponctuel lancé à l'entrée d'un cue (mêmes ordres que les pads de la fenêtre opérateur).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    /// Carton-titre en surcouche, 1 à [`MAX_TITLE`] caractères.
    Title(String),
    /// Plan sur l'enseigne géante du DJ (`showDj` de Sillage).
    ShowDj,
    /// Logo du DJ en avant-plan.
    DjLogo,
    /// Lance l'Ascension (si les portes du look la permettent).
    Ascension,
    /// Lance un événement du trafic (`"storm"`, `"fireworks"`…), s'il est dans la liste blanche du look.
    Event(EventName),
    /// Drop forcé, sans montée (`forceDrop` de Sillage).
    ForceDrop,
}
