//! L'instantané d'état : ce que le client de la sonde lit pour savoir où en est le show et le rythme.
//!
//! Chaque source est optionnelle : une application de test sans moteur de routage ni chef du spectacle répond avec des
//! valeurs de repos au lieu de refuser la requête.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use cf_cinema::CinemaDirector;
use cf_probe_proto::StateSnapshot;
use cf_render::audio::FilteredSignals;
use cf_render::{CurrentShow, DiagnosticsSnapshot, RunPhase, ShowSheet};

/// Les ressources lues pour bâtir l'instantané (toutes en lecture seule).
#[derive(SystemParam)]
pub struct SnapshotSources<'w> {
    time: Option<Res<'w, Time<Virtual>>>,
    signals: Option<Res<'w, FilteredSignals>>,
    phase: Option<Res<'w, RunPhase>>,
    show: Option<Res<'w, CurrentShow>>,
    sheet: Option<Res<'w, ShowSheet>>,
    director: Option<Res<'w, CinemaDirector>>,
    diagnostics: Option<Res<'w, DiagnosticsSnapshot>>,
}

impl SnapshotSources<'_> {
    /// Bâtit l'instantané de l'image courante.
    pub fn snapshot(&self) -> StateSnapshot {
        let signals = self.signals.as_deref();
        StateSnapshot {
            sim_time_s: self.time.as_ref().map_or(0.0, |time| time.elapsed_secs_f64()),
            // Sans séquenceur, `RunPhase` n'existe pas : le show joue (même repli que `RunPhase::default`).
            phase: format!("{:?}", self.phase.as_deref().copied().unwrap_or_default().phase),
            cue: self.cue_label(),
            shot: self
                .director
                .as_deref()
                .and_then(CinemaDirector::shot_id)
                .map(str::to_owned),
            beat_phase: signals.map_or(0.0, |s| s.beat_phase),
            bar_phase: signals.map_or(0.0, |s| s.bar_phase),
            grid_beat: signals.map_or(0.0, |s| s.grid_beat),
            grid_trust: signals.map_or(0.0, |s| s.grid_trust),
            kick_count: signals.map_or(0, |s| s.kick_count),
            kick_now: signals.is_some_and(|s| s.kick_now),
            // Sans diagnostics (application de test) : 0, jamais une valeur inventée.
            fps: self.diagnostics.as_deref().map_or(0.0, |d| d.fps),
        }
    }

    /// « DJ · cue » comme dans le journal de session ; sans feuille de conduite, la référence `slotN/cueM`.
    fn cue_label(&self) -> Option<String> {
        let show = self.show.as_deref()?;
        let named = self.sheet.as_deref().and_then(|sheet| {
            let slot = sheet.slots.get(show.slot)?;
            Some(format!("{} · {}", slot.dj, slot.cues.get(show.cue)?.name))
        });
        Some(named.unwrap_or_else(|| format!("slot{}/cue{}", show.slot, show.cue)))
    }
}
