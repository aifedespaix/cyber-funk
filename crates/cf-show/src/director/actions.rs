//! Les `Action` de `cf-control`, résolues contre l'état du spectacle (seul détenteur de l'état : une bascule dit
//! « allume / éteins / inverse », un niveau « mets à / décale de », et c'est ici qu'on applique).

use std::sync::Arc;

use cf_control::{Action, CueIndex, DjIndex, Layer};
use rand_core::Rng;

use super::ShowDirector;
use super::event::{CueCause, ShowEvent, ShowToggle};
use super::orders::Order;

impl<R: Rng> ShowDirector<R> {
    /// Applique une action reçue sur cette image.
    pub(super) fn apply(&mut self, action: &Action) {
        match action {
            Action::Go => self.go(),
            Action::Back => self.back(),
            Action::GotoCue(CueIndex(index)) => {
                let (slot, index) = (self.state.slot, usize::from(*index));
                if index < self.prepared.slots[slot].cues.len() {
                    self.leave_safe_look();
                    self.state.pending = None;
                    self.call_now(slot, index, CueCause::Goto);
                }
            }
            Action::Hold(switch) => {
                let on = switch.apply(self.state.hold);
                if self.toggle(ShowToggle::Hold, on) && on {
                    self.cancel_auto_call();
                }
            }
            Action::NextDj => {
                let to = self.state.armed_slot.unwrap_or(self.state.slot + 1);
                self.start_handover(to);
            }
            Action::PrevDj => {
                if let Some(to) = self.state.slot.checked_sub(1) {
                    self.start_handover(to);
                }
            }
            Action::ArmDj(DjIndex(index)) => {
                let index = usize::from(*index);
                if index < self.prepared.slots.len() && index != self.state.slot {
                    self.state.armed_slot = Some(index);
                    self.events.push(ShowEvent::Preload { slot: index });
                }
            }
            Action::ForceDrop => self.force_drop = true,
            Action::SkipShot => self.state.orders.give(Order::SkipShot),
            Action::Ascension => self.state.orders.give(Order::Ascension),
            Action::RandomEvent => self.state.orders.give(Order::RandomEvent),
            Action::ShowDj => {
                self.state.orders.give(Order::ShowDj);
                self.state.dj_show_seq = self.state.dj_show_seq.wrapping_add(1);
                self.events.push(ShowEvent::ShowDj { slot: self.state.slot });
            }
            Action::Title(id) => {
                // Un titre du fichier des cartons, dans la langue de l'événement ; sinon le texte tel qu'envoyé.
                let text = self
                    .titles
                    .text(&id.0, self.prepared.language)
                    .unwrap_or_else(|| Arc::clone(&id.0));
                self.events.push(ShowEvent::Title(text));
            }
            Action::Trigger(id) => self.state.orders.give_trigger(id.clone()),
            Action::Macro { index, value } => {
                let i = usize::from(index.0);
                let look = &self.prepared.slots[self.state.slot].cues[self.state.cue].look;
                if let (Some(knob), true) = (self.state.macros.get_mut(i), i < look.macros.len()) {
                    knob.value = value.apply(knob.value);
                    knob.engaged = true;
                }
            }
            Action::Master(value) => self.master.set_target(value.apply(self.master.target())),
            Action::LayerFade { layer, value } => {
                let fade = &mut self.layer_fades[layer_index(*layer)];
                fade.set_target(value.apply(fade.target()));
            }
            Action::Blackout(switch) => {
                let on = switch.apply(self.state.blackout_on);
                self.toggle(ShowToggle::Blackout, on);
            }
            Action::SafeLook => {
                if self.toggle(ShowToggle::SafeLook, true) {
                    self.cancel_auto_call();
                }
            }
            Action::FlashKill(switch) => {
                let killed = switch.apply(!self.state.flashes);
                self.toggle(ShowToggle::Flashes, !killed);
            }
            Action::Breathing(switch) => {
                let on = switch.apply(self.state.breathing);
                self.toggle(ShowToggle::Breathing, on);
            }
            // Tempo : l'arbitre de tempo (cf-analysis) ; application : le binaire et la fenêtre opérateur.
            Action::TapTempo
            | Action::Resync
            | Action::NudgePhase { .. }
            | Action::Playback(_)
            | Action::OperatorOverlay(_) => {}
        }
    }

    /// GO : quitte le repli s'il est engagé ; sinon fait entrer tout de suite un cue appelé qui attend (second GO) ;
    /// sinon appelle le cue suivant. Sans effet pendant le temps fort et le fondu d'une passation.
    fn go(&mut self) {
        if self.state.safe_look {
            self.leave_safe_look();
            return;
        }
        if self.enter_pending_now() || self.state.handover.is_some() {
            return;
        }
        let (slot, next) = (self.state.slot, self.state.cue + 1);
        if next < self.prepared.slots[slot].cues.len() {
            self.call(slot, next, CueCause::Go);
        }
    }

    /// BACK : annule d'abord un cue appelé qui attend son entrée (un GO de trop) ; sinon revient au cue précédent,
    /// tout de suite, sans rejouer ses ordres ponctuels.
    fn back(&mut self) {
        self.leave_safe_look();
        if self.state.pending.take().is_some() {
            return;
        }
        if let Some(previous) = self.state.cue.checked_sub(1) {
            self.call_now(self.state.slot, previous, CueCause::Back);
        }
    }

    /// Pose une bascule ; publie l'événement et rend vrai si elle a changé.
    fn toggle(&mut self, what: ShowToggle, on: bool) -> bool {
        let s = &mut self.state;
        let field = match what {
            ShowToggle::Hold => &mut s.hold,
            ShowToggle::Blackout => &mut s.blackout_on,
            ShowToggle::SafeLook => &mut s.safe_look,
            ShowToggle::Flashes => &mut s.flashes,
            ShowToggle::Breathing => &mut s.breathing,
        };
        if *field == on {
            return false;
        }
        *field = on;
        self.events.push(ShowEvent::Toggle { what, on });
        true
    }
}

fn layer_index(layer: Layer) -> usize {
    match layer {
        Layer::Background => 0,
        Layer::World => 1,
        Layer::Foreground => 2,
        Layer::Overlay => 3,
    }
}
