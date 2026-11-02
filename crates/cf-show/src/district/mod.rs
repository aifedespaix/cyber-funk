//! Gouverneur de quartiers : décide, d'après la musique, quand et vers quel quartier la ville change.
//!
//! Sans Bevy ni I/O : le monde (`cf-world-neon`) fournit un [`GovernorSnapshot`] par image et applique la
//! [`Decision`] rendue. Le temps est celui de l'audio, en secondes, pour que les décisions soient reproductibles.

mod governor;
#[cfg(test)]
mod tests;

pub use governor::{
    AFTER_DROP_BEATS, AFTER_DROP_BPM_RANGE, AFTER_DROP_FALLBACK_S, Governor, MAX_DWELL_S, MIN_DWELL_S, TRANSITION_MAX_S,
};

use cf_analysis::Section;

/// Type de quartier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DistrictKind {
    /// Centre néon (quartier d'origine de Sillage).
    Neon,
    /// Banlieue.
    Suburb,
    /// Zone résidentielle.
    Residential,
}

impl DistrictKind {
    /// Les trois types, dans l'ordre des indices de `variants_per_kind`.
    pub const ALL: [DistrictKind; 3] = [Self::Neon, Self::Suburb, Self::Residential];

    /// Libellé affiché dans le panneau F4.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Neon => "Centre néon",
            Self::Suburb => "Banlieue",
            Self::Residential => "Résidentiel",
        }
    }

    /// Identifiant stable (déclencheurs `district:<slug>`, fichiers RON).
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::Neon => "neon",
            Self::Suburb => "suburb",
            Self::Residential => "residential",
        }
    }

    /// Indice dans [`Self::ALL`].
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Neon => 0,
            Self::Suburb => 1,
            Self::Residential => 2,
        }
    }
}

/// Un quartier précis : un type et l'index de sa variante d'étalonnage dans l'ensemble du type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DistrictId {
    /// Type de quartier.
    pub kind: DistrictKind,
    /// Index de la variante d'étalonnage.
    pub variant: u8,
}

/// Façon de passer d'un quartier à l'autre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    /// Fondu progressif de la palette le long de la route.
    Gradient,
    /// Brouillard.
    Fog,
    /// Zone tampon.
    Buffer,
    /// Pont.
    Bridge,
    /// Rétrécissement.
    Shrink,
    /// Noir complet.
    Blackout,
    /// Échangeur.
    Interchange,
}

impl TransitionKind {
    /// Vrai si la transition existe réellement ; les autres retombent sur le dégradé.
    #[must_use]
    pub fn implemented(self) -> bool {
        matches!(self, Self::Gradient)
    }

    /// Libellé affiché dans le panneau F4.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Gradient => "Dégradé",
            Self::Fog => "Brouillard",
            Self::Buffer => "Zone tampon",
            Self::Bridge => "Pont",
            Self::Shrink => "Rétrécissement",
            Self::Blackout => "Noir",
            Self::Interchange => "Échangeur",
        }
    }
}

/// Ce que le gouverneur voit à chaque image.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy)]
pub struct GovernorSnapshot {
    /// Temps audio en secondes.
    pub now_s: f64,
    /// Section courante du morceau.
    pub section: Section,
    /// Vrai pendant l'image où la section vient de changer.
    pub section_changed: bool,
    /// Intensité globale, 0..1.
    pub intensity: f32,
    /// Énergie des basses, 0..1.
    pub bass: f32,
    /// Énergie des aigus, 0..1.
    pub high: f32,
    /// Tempo connu, en battements par minute.
    pub bpm: Option<f32>,
    /// Un drop se prépare.
    pub drop_imminent: bool,
    /// Le drop tombe pendant cette image.
    pub drop_now: bool,
    /// Un événement marquant vient de se terminer (une seule image).
    pub event_ended: bool,
    /// Un show est chargé.
    pub show_present: bool,
}

/// Raison pour laquelle le gouverneur ne change rien.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StayReason {
    /// Aucun show.
    NoShow,
    /// Pas assez de temps passé dans le quartier.
    TooSoon,
    /// Section `Peak`.
    PeakSection,
    /// Drop imminent.
    DropImminent,
    /// Fenêtre qui suit un drop.
    AfterDrop,
    /// Aucune fenêtre de changement ouverte.
    NoWindow,
    /// Transition en cours.
    Transitioning,
    /// Automatique gelé.
    Frozen,
}

impl StayReason {
    /// Libellé affiché dans le panneau F4.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::NoShow => "bloqué : aucun show",
            Self::TooSoon => "bloqué : temps minimum non atteint",
            Self::PeakSection => "bloqué : section Peak",
            Self::DropImminent => "bloqué : drop imminent",
            Self::AfterDrop => "bloqué : juste après un drop",
            Self::NoWindow => "en attente d'une fenêtre ouverte",
            Self::Transitioning => "transition en cours",
            Self::Frozen => "automatique gelé",
        }
    }
}

/// Décision rendue à chaque image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Rien ne change.
    Stay {
        /// Pourquoi.
        reason: StayReason,
    },
    /// Passer vers un nouveau quartier.
    Change {
        /// Quartier visé.
        to: DistrictId,
        /// Transition à jouer.
        transition: TransitionKind,
    },
}
