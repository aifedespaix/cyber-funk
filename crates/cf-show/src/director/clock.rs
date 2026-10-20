//! Le temps de la conduite : chronomètre du cue (règles d'avancée) et attente d'entrée d'un cue appelé.
//!
//! Deux idées de Sillage (`director.ts`) :
//! * un changement dû attend le prochain drop ; passé `MAX_LATE` il part à la phrase suivante, et passé
//!   `MAX_LATE + NO_BEAT_FALLBACK` tout de suite (même sans musique) ;
//! * les échéances sont absolues : dans Sillage, attendre un drop ne décale pas les niveaux suivants. Ici le
//!   chronomètre d'un cue part donc quand il est **appelé** (GO ou échéance), pas quand son entrée se réalise.

use cf_analysis::AudioFeatures;

use super::event::CueCause;
use crate::model::{ClockTime, CueAdvance, CueEntry};
use crate::wait::MusicWait;

/// Un changement dû attend le prochain drop ; passé ce retard (s) il part à la phrase suivante (`MAX_LATE` de
/// Sillage).
pub const MAX_LATE: f32 = 90.0;
/// … et passé ce retard supplémentaire (s), sans attendre du tout (`NO_BEAT_FALLBACK` de Sillage). Sert aussi de
/// plafond à l'attente d'une phrase.
pub const NO_BEAT_FALLBACK: f32 = 30.0;
/// Une phrase : seize temps (`PHRASE_BEATS` de Sillage).
pub const PHRASE_BEATS: u32 = 16;
/// Un temps fort : un kick sur quatre (`DOWNBEAT_EVERY` de `dj-transition.ts`).
pub const DOWNBEAT_EVERY: u32 = 4;
/// On attend au plus ce temps (s) un temps fort avant de couper quand même (`MAX_WAIT` de `dj-transition.ts`).
pub const DOWNBEAT_MAX_WAIT: f32 = 4.0;
/// Tempo supposé quand l'analyse n'en a pas encore (s'accorde avec le fondu par défaut de 8 s pour seize temps de
/// `dj-transition.ts`).
pub const FALLBACK_BPM: f32 = 120.0;
const DAY_S: f64 = 86_400.0;

/// Durée d'une mesure (4 temps), en secondes, au tempo détecté ou à [`FALLBACK_BPM`].
#[must_use]
pub fn bar_seconds(bpm: Option<f32>) -> f32 {
    let bpm = bpm.filter(|b| b.is_finite() && *b > 0.0).unwrap_or(FALLBACK_BPM);
    4.0 * 60.0 / bpm
}

/// Ce qui se passe musicalement sur l'image : de quoi décider qu'une entrée est prête.
#[derive(Debug, Clone, Copy, Default)]
#[allow(clippy::struct_excessive_bools, reason = "quatre faits musicaux indépendants de la même image")]
pub(crate) struct Beats {
    /// Kick sur le premier temps d'une mesure.
    pub downbeat: bool,
    /// Une nouvelle phrase commence (le compte des kicks franchit un multiple de 16).
    pub phrase_start: bool,
    /// Une source livre du signal.
    pub live: bool,
    /// Le drop frappe sur cette image (réel ou forcé).
    pub drop_now: bool,
}

/// Un cue appelé qui attend son entrée.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PendingCue {
    /// Créneau du cue (un autre que le créneau courant à la fin d'une passation).
    pub slot: usize,
    /// Rang du cue dans son créneau.
    pub cue: usize,
    /// Comment il entre.
    pub entry: CueEntry,
    /// Qui l'a appelé.
    pub cause: CueCause,
    /// Temps (s) écoulé depuis l'appel.
    pub waited: f32,
}

impl PendingCue {
    /// L'entrée peut se faire sur cette image.
    pub(crate) fn ready(&self, beats: Beats) -> bool {
        match self.entry {
            CueEntry::Fade { .. } => true,
            CueEntry::Cut => beats.downbeat || beats.drop_now || !beats.live || self.waited >= DOWNBEAT_MAX_WAIT,
            CueEntry::OnDrop => {
                beats.drop_now
                    || (self.waited > MAX_LATE && beats.phrase_start)
                    || self.waited > MAX_LATE + NO_BEAT_FALLBACK
            }
            CueEntry::OnPhrase => beats.phrase_start || !beats.live || self.waited > NO_BEAT_FALLBACK,
        }
    }
}

/// Le chronomètre du cue en cours (ou du cue appelé, pendant son attente d'entrée).
#[derive(Debug, Clone, Default)]
pub(crate) struct CueClock {
    /// Secondes depuis l'appel.
    pub elapsed: f64,
    advance: Option<CueAdvance>,
    /// Compte continu des kicks (`AudioFeatures::beat`) à l'appel.
    beat_at_call: f64,
    /// Échéance (s depuis l'appel) des avancées `After` et `At`, `None` tant qu'une heure `At` n'est pas résolue.
    due_s: Option<f64>,
    wait: MusicWait,
    music: bool,
}

impl CueClock {
    /// Démarre le chronomètre d'un cue appelé maintenant.
    pub(crate) fn start(&mut self, advance: CueAdvance, beat: f64) {
        self.elapsed = 0.0;
        self.advance = Some(advance);
        self.beat_at_call = beat;
        self.due_s = match advance {
            CueAdvance::After { seconds } => Some(seconds),
            _ => None,
        };
        self.wait.reset();
        self.music = false;
    }

    /// Avance d'une image.
    pub(crate) fn tick(&mut self, dt: f32, audio: &AudioFeatures, local_time_s: f64) {
        self.elapsed += f64::from(dt);
        // Remise à zéro du compte des kicks côté analyse : on repart de là.
        if audio.beat < self.beat_at_call {
            self.beat_at_call = audio.beat;
        }
        if let (Some(CueAdvance::At { time }), None) = (self.advance, self.due_s) {
            self.due_s = Some(self.elapsed + seconds_until(time, local_time_s));
        }
        self.music = self.wait.update(dt, audio.live, audio.intensity);
    }

    /// Temps musicaux écoulés depuis l'appel.
    fn beats(&self, audio: &AudioFeatures) -> f64 {
        (audio.beat - self.beat_at_call).max(0.0)
    }

    /// La règle d'avancée du cue est remplie sur cette image.
    pub(crate) fn due(&self, audio: &AudioFeatures, drop_now: bool) -> bool {
        match self.advance {
            None | Some(CueAdvance::Manual) => false,
            Some(CueAdvance::OnDrop) => drop_now,
            Some(CueAdvance::OnMusic) => self.music,
            Some(CueAdvance::Bars { bars }) => self.beats(audio) >= f64::from(bars) * 4.0,
            Some(CueAdvance::After { .. } | CueAdvance::At { .. }) => self.due_s.is_some_and(|due| self.elapsed >= due),
        }
    }

    /// Avancement vers l'échéance, 0 → 1, pour les règles qui en ont une (mesures, durée, heure).
    pub(crate) fn progress(&self, audio: &AudioFeatures) -> Option<f32> {
        let ratio = match self.advance? {
            CueAdvance::Bars { bars } => self.beats(audio) / (f64::from(bars) * 4.0),
            CueAdvance::After { .. } | CueAdvance::At { .. } => {
                let due = self.due_s?;
                if due > 0.0 { self.elapsed / due } else { 1.0 }
            }
            CueAdvance::Manual | CueAdvance::OnDrop | CueAdvance::OnMusic => return None,
        };
        Some(ratio.clamp(0.0, 1.0) as f32)
    }

    /// Secondes avant l'échéance (0 si elle est dépassée), pour les règles qui en ont une. En mesures, estimé au
    /// tempo détecté (`None` sans tempo).
    pub(crate) fn remaining(&self, audio: &AudioFeatures) -> Option<f32> {
        match self.advance? {
            CueAdvance::Bars { bars } => {
                let bpm = audio.bpm.filter(|b| b.is_finite() && *b > 0.0)?;
                let left = (f64::from(bars) * 4.0 - self.beats(audio)).max(0.0);
                Some((left * 60.0 / f64::from(bpm)) as f32)
            }
            CueAdvance::After { .. } | CueAdvance::At { .. } => Some((self.due_s? - self.elapsed).max(0.0) as f32),
            CueAdvance::Manual | CueAdvance::OnDrop | CueAdvance::OnMusic => None,
        }
    }
}

/// Secondes jusqu'à la prochaine occurrence de `time` à partir de l'heure locale `local_s` (0 : c'est maintenant).
/// Une heure locale non finie compte pour une journée entière : l'échéance ne part jamais par erreur.
fn seconds_until(time: ClockTime, local_s: f64) -> f64 {
    if !local_s.is_finite() {
        return DAY_S;
    }
    (f64::from(time.minutes_since_midnight()) * 60.0 - local_s).rem_euclid(DAY_S)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audio(beat: f64, bpm: Option<f32>) -> AudioFeatures {
        AudioFeatures {
            beat,
            bpm,
            live: true,
            ..AudioFeatures::NEUTRAL
        }
    }

    #[test]
    fn at_resolves_to_the_next_occurrence_of_the_hour() {
        let one_am = ClockTime::from_hm(1, 0).unwrap_or_else(|| unreachable!());
        assert!((seconds_until(one_am, 23.0 * 3600.0) - 7200.0).abs() < 1e-9);
        assert!((seconds_until(one_am, 3600.0)).abs() < 1e-9);
        assert!((seconds_until(one_am, 3601.0) - (DAY_S - 1.0)).abs() < 1e-9);
        assert!((seconds_until(one_am, f64::NAN) - DAY_S).abs() < 1e-9);
    }

    #[test]
    fn bars_count_beats_since_the_call_and_estimate_the_rest() {
        let mut clock = CueClock::default();
        clock.start(CueAdvance::Bars { bars: 2 }, 10.0);
        let a = audio(14.0, Some(120.0));
        clock.tick(0.5, &a, 0.0);
        assert!(!clock.due(&a, false));
        assert_eq!(clock.progress(&a), Some(0.5));
        assert!(clock.remaining(&a).is_some_and(|s| (s - 2.0).abs() < 1e-6));
        assert!(clock.due(&audio(18.0, None), false));
        assert_eq!(clock.remaining(&audio(18.0, None)), None, "sans tempo, pas d'estimation");
    }

    #[test]
    fn entries_wait_like_sillage() {
        let mut p = PendingCue {
            slot: 0,
            cue: 1,
            entry: CueEntry::OnDrop,
            cause: CueCause::Auto,
            waited: 10.0,
        };
        let quiet = Beats {
            live: true,
            ..Beats::default()
        };
        let phrase = Beats {
            phrase_start: true,
            ..quiet
        };
        assert!(!p.ready(phrase), "avant MAX_LATE, seule la frappe du drop compte");
        assert!(p.ready(Beats { drop_now: true, ..quiet }));
        p.waited = MAX_LATE + 1.0;
        assert!(!p.ready(quiet) && p.ready(phrase), "après MAX_LATE : la phrase suivante");
        p.waited = MAX_LATE + NO_BEAT_FALLBACK + 0.1;
        assert!(p.ready(quiet), "puis tout de suite");
        p.entry = CueEntry::Cut;
        p.waited = 0.0;
        assert!(!p.ready(quiet) && p.ready(Beats { downbeat: true, ..quiet }));
        assert!(p.ready(Beats::default()), "sans musique, la coupe n'attend pas");
    }
}
