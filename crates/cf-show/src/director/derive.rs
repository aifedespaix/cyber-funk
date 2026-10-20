//! Ce que l'image publie une fois la conduite à jour : avancement, chaleur, portes, couches et teinte.

use rand_core::Rng;

use super::looks::Target;
use super::{HEAT_SECONDS, ShowDirector, VARIATION_AFTER_S};
use crate::model::{LayerMix, SlotMode};

impl<R: Rng> ShowDirector<R> {
    /// Avancement, chaleur, portes, couches et teinte de l'image.
    pub(super) fn derive(&mut self, dt: f32) {
        let s = &mut self.state;
        let audio = &self.frame.audio;
        let slot = &self.prepared.slots[s.slot];
        let cue = &slot.cues[s.cue];
        let next = slot.cues.get(s.cue + 1);
        let manual = s.mode == SlotMode::Manual;
        let waiting = s.pending.is_some();
        s.cue_elapsed = self.clock.elapsed;
        s.cue_progress = self.clock.progress(audio);
        s.next_cue_in = if waiting || manual || next.is_none() {
            None
        } else {
            self.clock.remaining(audio)
        };
        // Glissement d'acte de Sillage : vers le look du cue suivant au fil de l'échéance (règles à échéance seules).
        let act = s.cue_progress.filter(|_| !manual && !s.safe_look && next.is_some());
        s.level = if s.safe_look {
            self.prepared.safe_level
        } else {
            cue.level
        };
        // Chaleur de Sillage : (niveau − 1 + avancement dans l'acte) / 5, lissée.
        let target_heat = if s.level == 0 {
            0.0
        } else {
            (f32::from(s.level) - 1.0 + act.unwrap_or(0.0)) / 5.0
        };
        let k = if self.heat_snap {
            1.0
        } else {
            1.0 - (-dt / HEAT_SECONDS).exp()
        };
        s.heat += (target_heat - s.heat) * k;
        self.heat_snap = false;

        let after_set = !manual
            && !s.safe_look
            && !waiting
            && next.is_none()
            && s.cue > 0
            && self.clock.elapsed >= VARIATION_AFTER_S;
        let varied = self.variation.update(f64::from(dt), after_set) && self.variation.uses_lower();
        s.mixer.refresh(&cue.look, &s.macros, s.master, next.map(|c| &c.look));
        let target = if s.safe_look {
            Target::Safe(&self.prepared.safe)
        } else if let Some(progress) = act {
            Target::Act(progress)
        } else if varied {
            Target::Variation(&slot.cues[s.cue - 1].look.gates, self.variation.mix())
        } else {
            Target::Still
        };
        s.mixer.frame(target, dt);
        let l = s.mixer.layers();
        let [bg, world, fg, overlay] = &self.layer_fades;
        s.layers = LayerMix {
            background: l.background * bg.value(),
            world: l.world * world.value(),
            foreground: l.foreground * fg.value(),
            overlay: l.overlay * overlay.value(),
        };
        s.hue_shift_deg = s.mixer.hue();
    }
}
