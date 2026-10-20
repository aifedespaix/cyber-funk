//! La conduite préparée à l'ouverture : chaque cue avec son look résolu (surcharges appliquées), son niveau, son
//! thème et ses déclencheurs déjà prêts à servir. Tout ce qui alloue est fait ici, une fois ; pendant le show, le chef
//! du spectacle ne fait que lire ces tables.

use std::sync::Arc;

use cf_control::TriggerId;
use thiserror::Error;

use crate::drop::{MAX_LEVEL, ShowLevel, TitleLanguage};
use crate::load::LoadedShow;
use crate::model::look::MAX_FLOOR;
use crate::model::{CueAdvance, CueEntry, Look, LookId, PaletteRef, ShowGates, SlotMode, Theme, Trigger};

/// Le show ne peut pas être préparé (ne devrait pas arriver avec un [`LoadedShow`] validé).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DirectorError {
    /// L'événement n'a aucun créneau.
    #[error("l'événement n'a aucun créneau DJ")]
    NoSlot,
    /// Un créneau n'a aucun cue.
    #[error("le créneau {slot} n'a aucun cue")]
    EmptySlot {
        /// Rang du créneau.
        slot: usize,
    },
    /// Un look référencé est introuvable.
    #[error("look « {look} » introuvable")]
    MissingLook {
        /// Identifiant du look.
        look: LookId,
    },
}

/// Niveau d'intensité d'un look (`ShowLevel` de Sillage, 0 à 5), déduit de ce que ses portes permettent.
///
/// Reproduit exactement les six looks `niveau-*` (test `levels_of_the_six_sillage_looks`) et classe un look
/// quelconque par ce qu'il autorise : l'Ascension n'existe qu'au niveau 5, tous les étages de vol à partir du 4, le
/// vol haut au 3, un premier envol au 2, des événements du trafic au 1 ; rien de tout cela : la pré-intro.
#[must_use]
pub fn look_level(gates: &ShowGates) -> ShowLevel {
    if gates.ascension {
        MAX_LEVEL
    } else if gates.max_floor >= MAX_FLOOR {
        4
    } else if gates.max_floor >= 4 {
        3
    } else if gates.max_floor >= 1 {
        2
    } else {
        u8::from(!gates.events.is_empty())
    }
}

/// Ordre ponctuel d'un cue, prêt à servir (textes et identifiants partagés : les lancer n'alloue rien).
#[derive(Debug, Clone)]
pub(crate) enum CueTrigger {
    Title(Arc<str>),
    ShowDj,
    DjLogo,
    Ascension,
    Event(TriggerId),
    ForceDrop,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedCue {
    pub look: Look,
    pub level: ShowLevel,
    /// Thème à prendre à l'entrée ; `None` : garder la palette à l'écran (`PaletteRef::Inherit`).
    pub theme: Option<Arc<Theme>>,
    pub entry: CueEntry,
    pub advance: CueAdvance,
    pub triggers: Vec<CueTrigger>,
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedSlot {
    pub mode: SlotMode,
    /// Thème du DJ, sinon celui de l'événement : pris à la passation.
    pub theme: Arc<Theme>,
    pub cues: Vec<PreparedCue>,
}

#[derive(Debug, Clone)]
pub(crate) struct Prepared {
    pub slots: Vec<PreparedSlot>,
    pub safe: Look,
    pub safe_level: ShowLevel,
    pub language: TitleLanguage,
}

impl Prepared {
    pub(crate) fn new(show: &LoadedShow) -> Result<Self, DirectorError> {
        let event = show.event();
        let settings = &event.settings;
        let event_theme = Arc::new(settings.theme.clone());
        let safe = show
            .look(&settings.safe_look)
            .map(|resolved| resolved.look.clone())
            .ok_or_else(|| DirectorError::MissingLook {
                look: settings.safe_look.clone(),
            })?;
        if event.slots.is_empty() {
            return Err(DirectorError::NoSlot);
        }
        let mut slots = Vec::with_capacity(event.slots.len());
        for (index, slot) in event.slots.iter().enumerate() {
            if slot.cues.is_empty() {
                return Err(DirectorError::EmptySlot { slot: index });
            }
            let slot_theme = slot
                .dj
                .theme
                .as_ref()
                .map_or_else(|| Arc::clone(&event_theme), |theme| Arc::new(theme.clone()));
            let mut cues = Vec::with_capacity(slot.cues.len());
            for cue in &slot.cues {
                let look = show
                    .cue_look(cue)
                    .ok_or_else(|| DirectorError::MissingLook { look: cue.look.clone() })?;
                let theme = match &look.palette {
                    PaletteRef::Inherit => None,
                    PaletteRef::Dj => Some(Arc::clone(&slot_theme)),
                    PaletteRef::Preset(preset) => Some(Arc::new(Theme::Preset(*preset))),
                    PaletteRef::Custom(custom) => Some(Arc::new(Theme::Custom(custom.clone()))),
                };
                cues.push(PreparedCue {
                    level: look_level(&look.gates),
                    look,
                    theme,
                    entry: cue.entry,
                    advance: cue.advance,
                    triggers: cue.triggers.iter().map(prepare_trigger).collect(),
                });
            }
            slots.push(PreparedSlot {
                mode: slot.mode,
                theme: slot_theme,
                cues,
            });
        }
        Ok(Self {
            safe_level: look_level(&safe.gates),
            safe,
            slots,
            language: settings.title_language,
        })
    }
}

fn prepare_trigger(trigger: &Trigger) -> CueTrigger {
    match trigger {
        Trigger::Title(text) => CueTrigger::Title(Arc::from(text.as_str())),
        Trigger::ShowDj => CueTrigger::ShowDj,
        Trigger::DjLogo => CueTrigger::DjLogo,
        Trigger::Ascension => CueTrigger::Ascension,
        Trigger::Event(name) => CueTrigger::Event(TriggerId::new(name.as_str())),
        Trigger::ForceDrop => CueTrigger::ForceDrop,
    }
}
