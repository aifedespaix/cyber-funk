//! Réglages du pont audio, et choix des backends (le vrai matériel, ou la sortie nulle et le signal synthétique des
//! tests).

use std::path::PathBuf;

use cf_audio::capture::{DeviceSelector, SyntheticBus};

use super::types::AudioSource;

/// D'où vient le signal de capture.
#[derive(Debug, Clone, Default)]
pub enum InputBackendChoice {
    /// Les périphériques d'entrée du système (`cpal`).
    #[default]
    Cpal,
    /// Un banc de périphériques simulés : pour tester toute la chaîne sans carte son.
    Synthetic(SyntheticBus),
    /// Aucune entrée : micro et ligne ne produisent rien.
    Disabled,
}

/// Où part le son du lecteur de démo.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum OutputChoice {
    /// La sortie par défaut du système.
    #[default]
    Default,
    /// Une sortie nulle cadencée comme une carte son (tests, machines sans audio) ; `speed` > 1 accélère la lecture.
    Null {
        /// Multiple de la vitesse réelle.
        speed: f64,
    },
    /// Pas de lecteur : la source Démo est indisponible.
    Disabled,
}

/// Réglages du plugin [`CfAudioPlugin`](super::CfAudioPlugin).
#[derive(Debug, Clone)]
pub struct AudioConfig {
    /// Entrée de capture.
    pub input: InputBackendChoice,
    /// Sortie du lecteur de démo.
    pub output: OutputChoice,
    /// Dossier des pistes (et son sous-dossier `local/`).
    pub music_dir: PathBuf,
    /// Source écoutée au démarrage. Sans lecteur, le plugin se replie sur le micro et le dit dans `notes`.
    pub initial_source: AudioSource,
    /// Lancer la lecture dès que la source Démo est choisie au démarrage (le choix de la source Démo en cours de
    /// route lance toujours la lecture).
    pub autoplay: bool,
    /// Périphérique du micro.
    pub mic_device: DeviceSelector,
    /// Périphérique de l'entrée ligne.
    pub line_device: DeviceSelector,
    /// Décalage visuel initial (ms), voir [`VisualLatency`](super::VisualLatency).
    pub latency_ms: f32,
    /// Graine du tirage au hasard de la playlist.
    pub playlist_seed: u64,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input: InputBackendChoice::default(),
            output: OutputChoice::default(),
            music_dir: PathBuf::from("assets/music"),
            initial_source: AudioSource::Demo,
            autoplay: true,
            mic_device: DeviceSelector::Default,
            line_device: DeviceSelector::Default,
            latency_ms: 0.0,
            playlist_seed: 0x00C1_BE2F_0000_0001,
        }
    }
}
