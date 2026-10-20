//! `ShowState` : ce que tous les systèmes lisent, mis à jour sur place une fois par image par le seul chef du
//! spectacle (contrat de `engine/show/types.ts` de Sillage, étendu aux créneaux et aux cues).
//!
//! Correspondance avec Sillage : `level` (le niveau) devient `cue` (le rang du cue) et `level` désigne désormais le
//! niveau d'intensité du look à l'écran ; `actProgress` → `cue_progress`, `nextLevelIn` → `next_cue_in`,
//! `djIndex` → `slot`, `levelSeq` → `cue_seq`, `djSeq` → `slot_seq`, `djTheme` → `theme`, `take()` →
//! `ShowDirector::take` (avec `orders` pour lire sans consommer), `drop` → `ShowDirector::drop_plan`,
//! `freshness` → `ShowDirector::freshness`, `requestTimeScale` → `ShowDirector::request_time_scale`.
//! Disparaissent : `active` (le mode cinéma est le seul mode), `duration` et `progress` (un créneau n'a pas de durée
//! fixe, la conduite est écrite en cues), et les réglages statiques (`eventName`, `djSign`, `djOnAds`, nom du DJ) qui
//! se lisent dans l'événement chargé.

use std::sync::Arc;

use super::clock::PendingCue;
use super::looks::{LookMixer, MACROS, MacroKnob};
use super::orders::Orders;
use crate::drop::ShowLevel;
use crate::model::{LayerMix, ShowGates, SlotMode, Theme};

/// Étape d'une passation de DJ.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoverPhase {
    /// On attend un temps fort pour lancer le fondu (au plus `DOWNBEAT_MAX_WAIT`).
    Downbeat,
    /// Fondu de palette d'une mesure, logo et enseigne du DJ suivant.
    Fade,
}

/// Une passation en cours, de l'appui sur `NextDj` jusqu'à l'appel du premier cue du créneau suivant (qui attend
/// ensuite sa propre entrée, comme tout cue appelé : voir `ShowState::pending`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Handover {
    /// Créneau qui joue.
    pub from: usize,
    /// Créneau qui arrive.
    pub to: usize,
    /// Étape.
    pub phase: HandoverPhase,
    /// Secondes passées dans l'étape.
    pub elapsed: f32,
    /// Durée du fondu (une mesure), connue au début du fondu.
    pub fade_s: f32,
}

impl Handover {
    /// Avancement du fondu, 0 → 1 (0 pendant l'attente du temps fort).
    #[must_use]
    pub fn progress(&self) -> f32 {
        match self.phase {
            HandoverPhase::Downbeat => 0.0,
            HandoverPhase::Fade if self.fade_s > 0.0 => (self.elapsed / self.fade_s).clamp(0.0, 1.0),
            HandoverPhase::Fade => 1.0,
        }
    }
}

/// L'état du spectacle, en lecture seule pour tous les systèmes.
#[derive(Debug, Clone)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "bascules indépendantes de l'opérateur, comme `flashes` et `breathing` dans Sillage"
)]
pub struct ShowState {
    // --- Créneau ---
    /// Rang du créneau DJ en cours (celui dont le cue est à l'écran).
    pub slot: usize,
    /// Monte à chaque changement de créneau.
    pub slot_seq: u32,
    /// Qui décide des changements de cue pendant ce créneau.
    pub mode: SlotMode,
    /// Créneau armé : le prochain `NextDj` y mène.
    pub armed_slot: Option<usize>,
    /// Passation en cours.
    pub handover: Option<Handover>,

    // --- Cue ---
    /// Rang du cue à l'écran dans son créneau.
    pub cue: usize,
    /// Monte à chaque entrée de cue : de quoi réagir sans écouter les événements.
    pub cue_seq: u32,
    /// Cue appelé qui attend son entrée (drop, phrase, temps fort), ou `None`.
    pub pending: Option<PendingCue>,
    /// Secondes depuis l'appel du cue en cours (son chronomètre).
    pub cue_elapsed: f64,
    /// Avancement vers l'échéance de la règle d'avancée, 0 → 1, si elle en a une (mesures, durée, heure). `None`
    /// pendant l'attente d'entrée d'un cue appelé.
    pub cue_progress: Option<f32>,
    /// Secondes avant que le cue suivant ne soit dû (0 si l'échéance est passée). `None` en mode Manuel, sans cue
    /// suivant, sans échéance connue, ou pendant l'attente d'entrée d'un cue appelé.
    pub next_cue_in: Option<f32>,
    /// Secondes depuis le chargement du créneau.
    pub elapsed: f64,
    /// L'avancée automatique est figée (`Hold`) ; GO reste possible.
    pub hold: bool,

    // --- Intensité ---
    /// Niveau d'intensité du look à l'écran, 0 à 5 (`ShowLevel` de Sillage).
    pub level: ShowLevel,
    /// Escalade lissée, 0 → 1 : sert à doser (probabilités, intensités).
    pub heat: f32,
    /// Potards de macro du look en cours.
    pub macros: [MacroKnob; MACROS],
    /// Intensité globale, 0..1, déjà glissée (voir `fade::OPERATOR_FADE`).
    pub master: f32,
    /// Opacité effective des couches : celle du look (glissée) multipliée par les fondus de l'opérateur.
    pub layers: LayerMix,
    /// Rotation de teinte de la palette, en degrés (glissée).
    pub hue_shift_deg: f32,
    /// Thème de couleurs à afficher.
    pub theme: Arc<Theme>,
    /// Monte à chaque changement de thème.
    pub theme_seq: u32,
    /// Durée (s) du fondu de palette vers `theme` (jamais une coupe : une palette qui saute est un saut de
    /// luminance).
    pub theme_fade_s: f32,

    // --- Sécurité ---
    /// Noir sur le projecteur, 0 (image) → 1 (noir), glissé : jamais plus vite que `MIN_FADE` pour une course
    /// complète.
    pub blackout: f32,
    /// Noir demandé.
    pub blackout_on: bool,
    /// Repli sur le look de secours.
    pub safe_look: bool,
    /// Flashs permis (`FlashKill` les coupe). Le limiteur photosensible s'applique en dernier dans tous les cas.
    pub flashes: bool,
    /// Mode respiration : moins d'effets, plans lents, aucun flash.
    pub breathing: bool,

    // --- Image ---
    /// Vrai pendant une seule image : le drop frappe maintenant.
    pub drop_now: bool,
    /// Facteur de temps du monde (1 normal).
    pub time_scale: f32,
    /// Ordres ponctuels en attente.
    pub orders: Orders,
    /// Monte à chaque rappel du DJ (`djShowSeq` de Sillage).
    pub dj_show_seq: u32,
    /// Monte à chaque logo du DJ demandé.
    pub dj_logo_seq: u32,

    pub(crate) mixer: LookMixer,
}

impl ShowState {
    /// Ce que le look à l'écran autorise. Les listes ne changent qu'avec le look ; les nombres glissent.
    #[must_use]
    pub fn gates(&self) -> &ShowGates {
        self.mixer.gates()
    }
}
