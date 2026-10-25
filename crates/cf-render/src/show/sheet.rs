//! La feuille de conduite : les noms et les règles du show, copiés une fois à l'ouverture pour l'interface.
//!
//! `ShowState` ne porte que des rangs (créneau 1, cue 3) : pour écrire « KRMN · Démesure », l'opérateur a besoin des
//! noms de l'événement. `LoadedShow` n'est pas gardé en ressource (il appartient au chef du spectacle et au
//! préchargement) ; la feuille en est un extrait en lecture seule, sans Bevy dans ses types de données.

use bevy::prelude::*;
use cf_show::load::LoadedShow;
use cf_show::model::{CueAdvance, CueEntry};

/// Un cue tel que la liste de l'opérateur l'affiche.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetCue {
    /// Nom du cue.
    pub name: String,
    /// Comment on y entre.
    pub entry: CueEntry,
    /// Quand le suivant est dû.
    pub advance: CueAdvance,
    /// Noms des macros du look du cue (surcharges comprises), dans l'ordre des potards ; vide si le look est inconnu.
    pub macros: Vec<String>,
}

/// Un créneau DJ.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetSlot {
    /// Nom du DJ (vide : sans nom).
    pub dj: String,
    /// Horaires prévus en minutes depuis minuit (début, fin), s'il y en a.
    pub schedule: Option<(u16, u16)>,
    /// La conduite.
    pub cues: Vec<SheetCue>,
}

/// Les noms du show en cours, lus par la fenêtre opérateur. Écrite une fois par `ShowPlugin`.
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub struct ShowSheet {
    /// Nom de l'événement (vide : aucun).
    pub event_name: String,
    /// L'enseigne géante du DJ est permise (`EventSettings::dj_sign`) : lue par `cf-world-neon` (J7-02).
    pub dj_sign: bool,
    /// Créneaux dans l'ordre de passage.
    pub slots: Vec<SheetSlot>,
}

impl ShowSheet {
    /// Extrait la feuille d'un événement chargé.
    #[must_use]
    pub fn from_show(show: &LoadedShow) -> Self {
        let event = show.event();
        Self {
            event_name: event.settings.name.clone(),
            dj_sign: event.settings.dj_sign,
            slots: event
                .slots
                .iter()
                .map(|slot| SheetSlot {
                    dj: slot.dj.name.clone(),
                    schedule: slot
                        .dj
                        .schedule
                        .map(|s| (s.start.minutes_since_midnight(), s.end.minutes_since_midnight())),
                    cues: slot
                        .cues
                        .iter()
                        .map(|cue| SheetCue {
                            name: cue.name.clone(),
                            entry: cue.entry,
                            advance: cue.advance,
                            macros: show
                                .cue_look(cue)
                                .map(|look| look.macros.into_iter().map(|m| m.name).collect())
                                .unwrap_or_default(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// Le créneau de ce rang.
    #[must_use]
    pub fn slot(&self, index: usize) -> Option<&SheetSlot> {
        self.slots.get(index)
    }
}
