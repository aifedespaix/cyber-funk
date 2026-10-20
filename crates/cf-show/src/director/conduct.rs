//! La conduite : appel et entrée des cues, avancée automatique, passation de DJ, reprise.

use std::sync::Arc;

use rand_core::Rng;

use super::clock::{Beats, DOWNBEAT_MAX_WAIT, PendingCue, bar_seconds};
use super::event::{CueCause, ShowEvent, ShowToggle};
use super::looks::{MACROS, MacroKnob, Target, rest_position};
use super::orders::Order;
use super::prepared::{CueTrigger, PreparedCue};
use super::state::{Handover, HandoverPhase, ShowState};
use super::{ShowDirector, drop_input};
use crate::model::{CueEntry, SlotMode, Theme};

impl<R: Rng> ShowDirector<R> {
    /// Appelle un cue : son chronomètre part maintenant, il entrera selon sa règle d'entrée.
    pub(super) fn call(&mut self, slot: usize, cue: usize, cause: CueCause) {
        let Some(prepared) = self.prepared.slots.get(slot).and_then(|s| s.cues.get(cue)) else {
            return;
        };
        self.called.start(prepared.advance, self.frame.audio.beat);
        let entry = prepared.entry;
        self.state.pending = Some(PendingCue {
            slot,
            cue,
            entry,
            cause,
            waited: 0.0,
        });
        self.events.push(ShowEvent::CueCalled {
            slot,
            cue,
            entry,
            cause,
        });
    }

    /// Appelle un cue et le fait entrer tout de suite (`Back`, `GotoCue`, ouverture).
    pub(super) fn call_now(&mut self, slot: usize, cue: usize, cause: CueCause) {
        let Some(prepared) = self.prepared.slots.get(slot).and_then(|s| s.cues.get(cue)) else {
            return;
        };
        self.called.start(prepared.advance, self.frame.audio.beat);
        self.enter(slot, cue, cause);
    }

    /// Le cue appelé entre si sa règle d'entrée est remplie sur cette image.
    pub(super) fn step_pending(&mut self, beats: Beats) {
        if let Some(pending) = self.state.pending
            && pending.ready(beats)
        {
            self.enter(pending.slot, pending.cue, pending.cause);
        }
    }

    /// Le cue appelé entre tout de suite (second GO). Faux s'il n'y en a pas.
    pub(super) fn enter_pending_now(&mut self) -> bool {
        let Some(pending) = self.state.pending else {
            return false;
        };
        self.enter(pending.slot, pending.cue, pending.cause);
        true
    }

    /// Oublie le cue appelé s'il l'a été par une règle d'avancée (`Hold`, `SafeLook`) ; un appel de l'opérateur
    /// reste.
    pub(super) fn cancel_auto_call(&mut self) {
        if self.state.pending.is_some_and(|p| p.cause == CueCause::Auto) {
            self.state.pending = None;
        }
    }

    /// Avancée automatique (Auto, Semi) : la règle du cue en cours appelle le suivant, si personne ne l'a fait.
    pub(super) fn auto_advance(&mut self, beats: Beats) {
        let s = &self.state;
        if s.hold || s.safe_look || s.mode == SlotMode::Manual || s.pending.is_some() || s.handover.is_some() {
            return;
        }
        let (slot, next) = (s.slot, s.cue + 1);
        if next < self.prepared.slots[slot].cues.len() && self.clock.due(&self.frame.audio, beats.drop_now) {
            self.call(slot, next, CueCause::Auto);
        }
    }

    /// Fait entrer un cue : il devient le cue à l'écran, ses portes partent, ses ordres ponctuels sont lancés.
    pub(super) fn enter(&mut self, slot: usize, cue: usize, cause: CueCause) {
        if self.prepared.slots.get(slot).and_then(|s| s.cues.get(cue)).is_none() {
            return;
        }
        let (previous_slot, previous_cue) = (self.state.slot, self.state.cue);
        self.state.pending = None;
        std::mem::swap(&mut self.clock, &mut self.called);
        if slot != previous_slot {
            self.load_slot(slot);
        }
        let bar_s = bar_seconds(self.frame.audio.bpm);
        let prepared_slot = &self.prepared.slots[slot];
        let prepared = &prepared_slot.cues[cue];
        let previous = &self.prepared.slots[previous_slot].cues[previous_cue];
        let s = &mut self.state;
        s.cue = cue;
        s.cue_seq = s.cue_seq.wrapping_add(1);
        carry_macros(&mut s.macros, previous, prepared, cause == CueCause::Open);
        let fade_s = match prepared.entry {
            CueEntry::Fade { bars } => bar_s * f32::from(bars),
            CueEntry::Cut | CueEntry::OnDrop | CueEntry::OnPhrase => 0.0,
        };
        s.mixer.begin_entry(&prepared.look, fade_s);
        s.mixer.set_next(prepared_slot.cues.get(cue + 1).map(|c| &c.look));
        if let Some(theme) = &prepared.theme {
            // Une palette ne coupe jamais : au moins une mesure de fondu (un saut de palette est un saut de luminance).
            set_theme(s, theme, fade_s.max(bar_s));
        }
        self.events.push(ShowEvent::CueEntered {
            slot,
            cue,
            previous_slot,
            previous_cue,
            cause,
        });
        // Retour en arrière : on annule, on ne rejoue pas les cartons ni les ordres du cue.
        if cause != CueCause::Back {
            fire(&prepared.triggers, slot, s, &mut self.events, &mut self.force_drop);
        }
        self.variation.reset();
        if s.drop_now {
            // Entrée sur l'image du drop : le plan est refait avec les portes et le niveau du nouveau cue (sans
            // titre, l'entrée a les siens), comme `replan` dans Sillage.
            s.level = prepared.level;
            let next = prepared_slot.cues.get(cue + 1).map(|c| &c.look);
            s.mixer.refresh(&prepared.look, &s.macros, s.master, next);
            if !s.mixer.entering() && !s.safe_look {
                s.mixer.frame(Target::Still, 0.0);
            }
            let input = drop_input(s, &self.frame, &self.prepared, false);
            self.conductor.replan(&self.freshness, &input);
        }
    }

    /// Le créneau devient le créneau courant : son mode, et l'ordre au binaire de charger ses profils.
    fn load_slot(&mut self, slot: usize) {
        let s = &mut self.state;
        let previous = s.slot;
        s.slot = slot;
        s.slot_seq = s.slot_seq.wrapping_add(1);
        s.mode = self.prepared.slots[slot].mode;
        s.elapsed = 0.0;
        if s.armed_slot == Some(slot) {
            s.armed_slot = None;
        }
        self.events.push(ShowEvent::SlotLoaded { slot, previous });
    }

    /// Lance une passation vers `to` (sans effet si une passation est en cours ou si `to` n'existe pas).
    pub(super) fn start_handover(&mut self, to: usize) {
        let s = &mut self.state;
        if s.handover.is_some() || to == s.slot || to >= self.prepared.slots.len() {
            return;
        }
        // Le changement de cue en attente est abandonné : la passation passe avant.
        s.pending = None;
        s.handover = Some(Handover {
            from: s.slot,
            to,
            phase: HandoverPhase::Downbeat,
            elapsed: 0.0,
            fade_s: 0.0,
        });
        self.events.push(ShowEvent::Preload { slot: to });
    }

    /// Passation en cours : temps fort, puis fondu d'une mesure, puis appel du premier cue du créneau suivant.
    pub(super) fn step_handover(&mut self, beats: Beats) {
        let Some(mut handover) = self.state.handover else {
            return;
        };
        handover.elapsed += self.frame.dt;
        match handover.phase {
            HandoverPhase::Downbeat
                if beats.downbeat || beats.drop_now || !beats.live || handover.elapsed >= DOWNBEAT_MAX_WAIT =>
            {
                handover.phase = HandoverPhase::Fade;
                handover.elapsed = 0.0;
                handover.fade_s = bar_seconds(self.frame.audio.bpm);
                let theme = &self.prepared.slots[handover.to].theme;
                set_theme(&mut self.state, theme, handover.fade_s);
                self.events.push(ShowEvent::Handover {
                    from: handover.from,
                    to: handover.to,
                    fade_s: handover.fade_s,
                });
            }
            HandoverPhase::Fade if handover.elapsed >= handover.fade_s => {
                self.state.handover = None;
                self.call(handover.to, 0, CueCause::Handover);
                return;
            }
            HandoverPhase::Downbeat | HandoverPhase::Fade => {}
        }
        self.state.handover = Some(handover);
    }

    /// Quitte le repli sur le look de secours : le look du cue en cours revient (ses nombres glissent).
    pub(super) fn leave_safe_look(&mut self) {
        if self.state.safe_look {
            self.state.safe_look = false;
            self.events.push(ShowEvent::Toggle {
                what: ShowToggle::SafeLook,
                on: false,
            });
        }
    }

    /// Reprise (sauvegarde, J2-07) : ce cue de ce créneau, son chronomètre déjà avancé de `cue_elapsed` secondes,
    /// portes posées sans fondu. Faux si le créneau ou le cue n'existe pas.
    pub fn resume(&mut self, slot: usize, cue: usize, cue_elapsed: f64) -> bool {
        if self.prepared.slots.get(slot).and_then(|s| s.cues.get(cue)).is_none() {
            return false;
        }
        self.state.handover = None;
        self.state.pending = None;
        self.call_now(slot, cue, CueCause::Open);
        if cue_elapsed.is_finite() {
            self.clock.elapsed = cue_elapsed.max(0.0);
        }
        self.state.mixer.snap();
        self.heat_snap = true;
        true
    }
}

/// Les potards suivent le look : une macro de même nom au même rang garde sa position (le potard physique n'a pas
/// bougé) ; les autres se libèrent, à la position qui reproduit le look.
fn carry_macros(knobs: &mut [MacroKnob; MACROS], previous: &PreparedCue, next: &PreparedCue, reset: bool) {
    for (i, knob) in knobs.iter_mut().enumerate() {
        let kept = !reset
            && match (previous.look.macros.get(i), next.look.macros.get(i)) {
                (Some(a), Some(b)) => a.name == b.name,
                _ => false,
            };
        if !kept {
            *knob = MacroKnob {
                value: rest_position(&next.look, i),
                engaged: false,
            };
        }
    }
}

/// Nouveau thème, en fondu de `fade_s` secondes (sans effet si c'est déjà celui-là).
fn set_theme(state: &mut ShowState, theme: &Arc<Theme>, fade_s: f32) {
    if Arc::ptr_eq(theme, &state.theme) || **theme == *state.theme {
        return;
    }
    state.theme = Arc::clone(theme);
    state.theme_seq = state.theme_seq.wrapping_add(1);
    state.theme_fade_s = fade_s;
}

/// Lance les ordres ponctuels d'un cue qui entre.
fn fire(triggers: &[CueTrigger], slot: usize, s: &mut ShowState, events: &mut Vec<ShowEvent>, force_drop: &mut bool) {
    for trigger in triggers {
        match trigger {
            CueTrigger::Title(text) => events.push(ShowEvent::Title(Arc::clone(text))),
            CueTrigger::ShowDj => {
                s.orders.give(Order::ShowDj);
                s.dj_show_seq = s.dj_show_seq.wrapping_add(1);
                events.push(ShowEvent::ShowDj { slot });
            }
            CueTrigger::DjLogo => {
                s.dj_logo_seq = s.dj_logo_seq.wrapping_add(1);
                events.push(ShowEvent::DjLogo { slot });
            }
            CueTrigger::Ascension => s.orders.give(Order::Ascension),
            CueTrigger::Event(id) => s.orders.give_trigger(id.clone()),
            CueTrigger::ForceDrop => *force_drop = true,
        }
    }
}
