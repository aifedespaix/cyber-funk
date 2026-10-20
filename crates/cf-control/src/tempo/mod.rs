//! Sources de tempo pilotées par les contrôles : MIDI Clock ([`MidiClock`]) et tap tempo ([`TapTempo`]).
//!
//! Chacune produit une [`TempoEstimate`] : un tempo, et une position (en temps) ancrée sur un instant. L'arbitrage
//! entre sources (Clock / Link > audio > tap, bascule sans saut de phase, repli après 2 s de perte) vit dans
//! `cf-analysis` (`TempoArbiter`), qui publie le résultat dans `AudioFeatures` ; `cf-control` ne dépend pas de
//! `cf-analysis`, le pont de l'application recopie l'estimation champ pour champ.
//!
//! Les actions de tempo (`TapTempo`, `Resync`, `NudgePhase`) sont routées par le pont : `TapTempo` vers
//! [`TapTempo::tap`], les deux autres vers l'arbitre.

mod clock;
mod fit;
mod tap;

pub use clock::{MidiClock, PORT_TAKEOVER_S, PPQN, Transport};
pub use tap::{TAP_RESET_S, TapTempo};

/// Tempo et position estimés par une source, à un instant donné.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TempoEstimate {
    /// Instant de l'ancre (s, horloge monotone de l'application) : où la source a mesuré `beat`. Pour l'horloge, la
    /// dernière impulsion débarrassée de sa gigue ; pour le tap, la dernière frappe.
    pub time_s: f64,
    /// Position à `time_s`, en temps (noires). Les multiples de 4 sont des temps 1 quand `bar_aligned` est vrai.
    pub beat: f64,
    /// Tempo (BPM).
    pub bpm: f64,
    /// La source connaît le temps 1 de la mesure (horloge après Start ou Song Position, tap).
    pub bar_aligned: bool,
    /// Confiance 0..1.
    pub confidence: f32,
}
