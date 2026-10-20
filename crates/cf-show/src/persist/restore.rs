//! Reprise d'un set : un chef du spectacle neuf, replacé là où la sauvegarde l'a laissé.

use rand_core::{Rng, SeedableRng};

use super::{PersistError, ShowSave};
use crate::director::{MacroKnob, ShowDirector};
use crate::drop::DropTitles;
use crate::load::LoadedShow;

impl<R: Rng + SeedableRng + Send + Sync + 'static> ShowDirector<R> {
    /// Rouvre l'événement avec la graine de la sauvegarde, puis replace le créneau, le cue (chronomètre avancé de
    /// son temps écoulé, portes posées sans fondu), le temps du créneau, les macros et le DJ armé.
    ///
    /// La graine seule est sauvegardée, pas la position dans le flux du RNG : une reprise rejoue le hasard depuis
    /// le début, avec les mêmes règles (même graine, mêmes entrées, même spectacle), ce qui suffit à retrouver un
    /// spectacle cohérent. La fraîcheur (anti-répétition) repart de zéro.
    ///
    /// # Errors
    ///
    /// [`PersistError::WrongEvent`] si la sauvegarde vient d'un autre événement, [`PersistError::OutOfRange`] si son
    /// créneau ou son cue n'existe plus (événement modifié depuis), [`PersistError::Director`] à l'ouverture.
    pub fn resume_from(show: &LoadedShow, titles: DropTitles, save: &ShowSave) -> Result<Self, PersistError> {
        let name = &show.event().settings.name;
        if *name != save.event_name {
            return Err(PersistError::WrongEvent {
                saved: save.event_name.clone(),
                current: name.clone(),
            });
        }
        let mut director = Self::new(show, titles, save.seed)?;
        if !director.resume(save.slot, save.cue, save.cue_elapsed_s) {
            return Err(PersistError::OutOfRange {
                slot: save.slot,
                cue: save.cue,
            });
        }
        director.restore_session(save);
        Ok(director)
    }

    /// Ce que `resume` ne remet pas : temps du créneau, macros (`resume` les libère : l'entrée est une ouverture)
    /// et DJ armé.
    fn restore_session(&mut self, save: &ShowSave) {
        let state = self.state_mut();
        if save.elapsed_s.is_finite() {
            state.elapsed = save.elapsed_s.max(0.0);
        }
        // `update` recalcule `cue_elapsed` depuis le chronomètre ; on le publie dès maintenant pour qu'une lecture
        // avant la première image (interface de reprise) voie le bon temps.
        if save.cue_elapsed_s.is_finite() {
            state.cue_elapsed = save.cue_elapsed_s.max(0.0);
        }
        for (knob, saved) in state.macros.iter_mut().zip(&save.macros) {
            *knob = MacroKnob {
                value: saved.value.clamp(0.0, 1.0),
                engaged: saved.engaged,
            };
        }
        let slots = self.slot_count();
        let state = self.state_mut();
        state.armed_slot = save.armed_slot.filter(|&n| n < slots && n != state.slot);
    }
}
