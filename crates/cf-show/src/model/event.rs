//! L'événement (fichier `.cfshow` / `assets/shows/*.ron`) et ses créneaux DJ.
//!
//! Remplace les réglages du mode spectacle de Sillage (`ShowSettings` de `engine/show/defaults.ts`, `ShowDj`) :
//! une soirée préparée à l'avance, rejouable à l'identique, avec un créneau par DJ.

use serde::{Deserialize, Serialize};

use super::cue::Cue;
use super::ids::{AssetPath, ClockTime, ControlProfileId, LookId, SlotId};
use super::look::{Look, Theme};

/// Nombre maximal de créneaux dans un événement (`MAX_DJS` de Sillage).
pub const MAX_SLOTS: usize = 24;
/// Nombre maximal de cues dans un créneau.
pub const MAX_CUES: usize = 256;
/// Longueur maximale du nom de l'événement, en caractères (`MAX_EVENT_NAME` de Sillage).
pub const MAX_EVENT_NAME: usize = 60;
/// Longueur maximale du nom d'un DJ, en caractères (`MAX_DJ_NAME` de Sillage).
pub const MAX_DJ_NAME: usize = 40;
/// Longueur maximale du nom d'un écran, en caractères.
pub const MAX_MONITOR_NAME: usize = 120;
/// Plus grand décalage de latence accepté, en millisecondes (dans un sens ou dans l'autre).
pub const MAX_LATENCY_OFFSET_MS: f32 = 500.0;
/// Plage de tempo acceptée pour un profil audio, en BPM.
pub const BPM_LIMITS: (f32, f32) = (40.0, 250.0);

/// Une soirée : réglages globaux, looks propres à l'événement, créneaux DJ dans l'ordre de passage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// Version du format ; vaut [`super::FORMAT_VERSION`] une fois le fichier chargé (les anciennes versions sont
    /// migrées à la lecture).
    pub version: u32,
    /// Réglages globaux de la soirée.
    pub settings: EventSettings,
    /// Looks propres à l'événement. Ils s'ajoutent à la bibliothèque `assets/looks/` et priment sur un look de même
    /// identifiant. Absent : aucun.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub looks: Vec<Look>,
    /// Créneaux DJ dans l'ordre de passage, 1 à [`MAX_SLOTS`].
    pub slots: Vec<DjSlot>,
}

/// Réglages globaux d'un événement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSettings {
    /// Nom de l'événement, affichable sur les enseignes, 0 à [`MAX_EVENT_NAME`] caractères (vide : aucun).
    pub name: String,
    /// Thème de couleurs de la soirée, quand ni le DJ ni le look n'en imposent un.
    pub theme: Theme,
    /// Politique de flashs au lancement (basculable en direct par l'opérateur).
    pub flashes: FlashPolicy,
    /// Décalage appliqué à l'analyse pour compenser la latence de la chaîne son → image, en millisecondes,
    /// -500 à 500 (positif : l'image attend).
    pub latency_offset_ms: f32,
    /// Écran de la sortie projecteur.
    pub projector: MonitorSelect,
    /// Look de secours, toujours préchargé : action `SafeLook`, et repli si un look ne peut pas être préparé.
    pub safe_look: LookId,
    /// Enseigne géante du DJ dans le monde (réglage `djSign` de Sillage).
    pub dj_sign: bool,
    /// Nom du DJ sur les publicités du monde (réglage `djOnAds` de Sillage).
    pub dj_on_ads: bool,
    /// Profils de contrôle communs à toute la soirée (`assets/controllers/<id>.ron`), fusionnés dans l'ordre.
    /// Absent : aucun (le clavier par défaut reste actif).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<ControlProfileId>,
}

/// Politique de flashs. Dans tous les cas, le limiteur photosensible (WCAG 2.3.1) s'applique en dernier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlashPolicy {
    /// Flashs permis, plafonnés par le limiteur (réglage `flashes: true` de Sillage).
    Limited,
    /// Aucun flash (réglage `flashes: false` de Sillage).
    Off,
}

/// Choix de l'écran de la sortie projecteur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonitorSelect {
    /// Le premier écran secondaire s'il y en a un, sinon l'écran principal.
    Auto,
    /// L'écran de ce rang dans la liste du système (0 : le premier).
    Index(u8),
    /// L'écran de ce nom (tel que le système le rapporte), 1 à [`MAX_MONITOR_NAME`] caractères.
    Name(String),
}

/// Le créneau d'un DJ : qui il est, comment l'écouter, comment il pilote, et sa conduite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DjSlot {
    /// Identifiant stable, unique dans l'événement.
    pub id: SlotId,
    /// Identité affichée du DJ.
    pub dj: DjIdentity,
    /// Ce qu'on attend de sa musique.
    pub audio: AudioProfile,
    /// Qui décide des changements de cue pendant ce créneau.
    pub mode: SlotMode,
    /// Profils de contrôle propres au créneau (son contrôleur, sa table), fusionnés après ceux de l'événement.
    /// Absent : aucun.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<ControlProfileId>,
    /// La conduite : 1 à [`MAX_CUES`] cues, joués dans l'ordre.
    pub cues: Vec<Cue>,
}

/// Identité d'un DJ (`ShowDj` de Sillage, plus logo et horaires).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DjIdentity {
    /// Nom affiché sur l'enseigne et les cartons, 0 à [`MAX_DJ_NAME`] caractères (vide : pas de nom affiché).
    pub name: String,
    /// Logo (image sous `assets/`), affiché en avant-plan. Absent : le nom seul.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<AssetPath>,
    /// Thème de couleurs que le DJ impose. Absent : celui de l'événement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Theme>,
    /// Horaires prévus (information pour l'opérateur ; rien n'avance tout seul sur ces heures).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<Schedule>,
}

/// Horaires prévus d'un créneau, heures locales. Une fin plus petite que le début s'entend le lendemain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    /// Début prévu, `"HH:MM"`.
    pub start: ClockTime,
    /// Fin prévue, `"HH:MM"`, différente du début.
    pub end: ClockTime,
}

/// Ce qu'on attend de la musique d'un DJ : de quoi régler l'analyse avant même la première note.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioProfile {
    /// Tempo le plus lent attendu, en BPM, 40 à 250, plus petit que `bpm_max`.
    pub bpm_min: f32,
    /// Tempo le plus rapide attendu, en BPM, 40 à 250, plus grand que `bpm_min`.
    pub bpm_max: f32,
    /// Sensibilité de la détection des kicks et des sections, 0 à 1 (0,5 : réglage neutre ; plus haut : détecte
    /// plus facilement).
    pub sensitivity: f32,
    /// Source de tempo préférée pour ce créneau.
    pub tempo_source: TempoSource,
}

/// Source de tempo préférée. Si elle disparaît, le système se replie sur la détection audio sans coupure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TempoSource {
    /// Arbitrage automatique : la meilleure source disponible (MIDI Clock, puis Link, puis audio).
    Auto,
    /// Détection sur le signal audio (micro ou entrée ligne).
    Audio,
    /// MIDI Clock envoyée par le logiciel de mix ou la table.
    MidiClock,
    /// Ableton Link sur le réseau local.
    Link,
    /// Tempo tapé par l'opérateur.
    Tap,
}

/// Qui décide des changements de cue (voir `docs/ARCHITECTURE.md` §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlotMode {
    /// Les règles d'avancée des cues seules : le comportement de Sillage.
    Auto,
    /// GO de l'opérateur ; si personne n'appuie, les règles d'avancée prennent le relais (mode par défaut).
    Semi,
    /// Uniquement l'opérateur (ou les notes MIDI du DJ).
    Manual,
}
