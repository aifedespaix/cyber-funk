//! Arbitrage des sources de tempo et grille musicale publiée (temps, mesure, phrase).
//!
//! ```text
//! MidiClock (cf-control) ─┐
//! Link (J3-04) ───────────┤  TempoReading      TempoArbiter::update(now)
//! AudioBeatGrid ──────────┼────────────────▶   priorité Clock > Link > Audio > Tap (ou la source préférée du créneau),
//! TapTempo (cf-control) ──┘   offer(source)    bascule sans saut de phase, repli après 2 s sans nouvelle
//!                                                 └─▶ publish(&mut AudioFeatures) : bpm, tempo_source, confidence,
//!                                                     phase, bar, phrase
//! ```
//!
//! Tout est en temps de l'application (s, horloge monotone commune, la même que les `time_s` des événements MIDI) et
//! injecté par l'appelant : l'arbitre ne lit aucune horloge. Il tourne côté application, une fois par image, après
//! la lecture du dernier `AudioFeatures` : c'est là que se rencontrent l'audio (déjà ramené en temps d'application,
//! décalage de latence compris) et les contrôles.
//!
//! Les réactions à l'énergie (basses, médiums, aigus, sections) viennent toujours de l'audio : l'arbitre ne touche
//! qu'aux champs de tempo et de grille.

mod arbiter;
mod audio_grid;

pub use arbiter::{CATCH_UP_BEATS, EXTERNAL_LOSS_S, TempoArbiter};
pub use audio_grid::AudioBeatGrid;

use crate::features::TempoSource;

/// Temps par mesure (4/4).
pub const BEATS_PER_BAR: f64 = 4.0;

/// Temps par phrase : seize, comme `PHRASE_BEATS` de Sillage (`cinema/cues.ts`).
pub const BEATS_PER_PHRASE: f64 = 16.0;

/// Ce qu'une source dit du tempo, à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoReading {
    /// Instant de la mesure (s, horloge de l'application). L'arbitre extrapole depuis cet instant ; pour l'horloge MIDI
    /// et Link, c'est aussi son âge qui décide de la perte.
    pub time_s: f64,
    /// Position à `time_s`, en temps (noires).
    pub beat: f64,
    /// Tempo (BPM).
    pub bpm: f64,
    /// La source connaît le temps 1 : ses multiples de 4 sont des débuts de mesure.
    pub bar_aligned: bool,
    /// Confiance 0..1.
    pub confidence: f32,
}

impl TempoReading {
    /// Position extrapolée à `now_s`, au tempo de la lecture.
    #[must_use]
    pub fn beat_at(&self, now_s: f64) -> f64 {
        self.beat + (now_s - self.time_s) * self.bpm / 60.0
    }
}

/// Rang de priorité par défaut (0 = le plus sûr).
pub(crate) const PRIORITY: [TempoSource; 4] = [
    TempoSource::Clock,
    TempoSource::Link,
    TempoSource::Audio,
    TempoSource::Tap,
];

pub(crate) fn slot(source: TempoSource) -> usize {
    match source {
        TempoSource::Clock => 0,
        TempoSource::Link => 1,
        TempoSource::Audio => 2,
        TempoSource::Tap => 3,
    }
}
