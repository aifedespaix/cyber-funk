//! Ce que la scène donne au chef du spectacle à chaque image (`ShowFrame` de Sillage, plus les actions et l'heure).

use cf_analysis::AudioFeatures;
use cf_control::Action;

/// Les entrées d'une image. Le temps est un paramètre : rien n'est lu sur une horloge.
#[derive(Debug, Clone, Copy)]
pub struct ShowInput<'a> {
    /// Durée de l'image, en secondes (temps réel, jamais le temps du monde ralenti). Négatif ou non fini : 0.
    pub dt: f32,
    /// Horloge du show, en secondes, monotone (sert à la fraîcheur).
    pub now: f64,
    /// Heure locale, en secondes depuis minuit (0 à 86 400). Ne sert qu'aux avancées `At` (« à 01:00 »).
    pub local_time_s: f64,
    /// Dernier état de l'analyse audio.
    pub audio: &'a AudioFeatures,
    /// Le drop réel de la musique, une seule image.
    pub real_drop: bool,
    /// Actions reçues depuis l'image précédente, dans l'ordre (clavier, MIDI, OSC, interface). Les actions de tempo
    /// (`TapTempo`, `Resync`, `NudgePhase`) et d'application (`Playback`, `OperatorOverlay`) ne concernent pas le
    /// spectacle et sont ignorées ici.
    pub actions: &'a [Action],
}
