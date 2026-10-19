//! État partagé entre les trois acteurs du lecteur, sans verrou :
//!
//! - le **contrôle** ([`super::Player`]) écrit lecture/pause, volume, sourdine ;
//! - le **décodeur** (thread [`super::engine`]) pousse les échantillons et des [`Marker`] ;
//! - le **rendu** ([`super::Renderer`], callback temps réel) publie position, piste jouée et compteurs.
//!
//! Tout passe par des atomiques `Relaxed` : chaque valeur est indépendante, aucune n'en protège une autre (la
//! synchronisation des données audio, elle, est assurée par les `rtrb`).

use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};

use crate::playlist::PlayMode;

/// Valeur de [`Shared::track`] quand aucune piste ne joue encore.
pub(crate) const NO_TRACK: u32 = u32::MAX;

/// Volume général par défaut (Sillage : `volume = 0.8`).
pub(crate) const DEFAULT_VOLUME: f32 = 0.8;

/// Repère inséré par le décodeur dans le flux : « à partir de la trame `at_frame` du ring, c'est telle piste, à telle
/// position ». Il voyage dans un `rtrb` séparé du signal (le signal reste un ring de `f32` nus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Marker {
    /// Rang (en trames stéréo, depuis le démarrage) de la première trame concernée dans le ring audio. Toujours
    /// poussé **après** les trames qui le précèdent : quand le rendu voit le repère, ces trames sont dans le ring.
    pub at_frame: u64,
    /// Indice de la piste dans la playlist.
    pub track: u32,
    /// Position (trames à la fréquence de sortie) de la première trame dans la piste : 0, ou la cible d'un `seek`.
    pub start_frame: u64,
    /// Durée de la piste (trames à la fréquence de sortie), 0 si inconnue.
    pub duration_frames: u64,
    /// `true` : rupture voulue (suivant, précédent, seek). Le rendu fond en sortie, jette ce qui précède, puis fond en
    /// entrée. `false` : enchaînement naturel, sans trou ni fondu.
    pub flush: bool,
}

/// Commande du contrôle vers le décodeur (canal borné, jamais lue par le callback).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Command {
    Next,
    Prev,
    Select(isize),
    /// Position cible en secondes depuis le début de la piste jouée.
    Seek(f64),
    SetMode(PlayMode),
    Shutdown,
}

/// Atomiques partagés (voir le module).
#[derive(Debug)]
pub(crate) struct Shared {
    // Écrits par le contrôle.
    pub playing: AtomicBool,
    volume_bits: AtomicU32,
    pub muted: AtomicBool,
    mode: AtomicU8,
    /// La voie d'analyse a été prise : le rendu y pousse le signal.
    pub tap_enabled: AtomicBool,

    // Écrits par le décodeur.
    /// Rien à décoder (liste vide ou toutes les pistes illisibles) : un ring vide n'est alors pas une sous-alimentation.
    pub engine_idle: AtomicBool,

    // Écrits par le rendu.
    pub track: AtomicU32,
    pub position_frames: AtomicU64,
    pub duration_frames: AtomicU64,
    /// Trames rendues vers la sortie depuis le démarrage (horloge audio du lecteur).
    pub rendered_frames: AtomicU64,
    pub underruns: AtomicU64,
    pub tap_overflows: AtomicU64,
    /// Erreurs signalées par le pilote de sortie (xrun du périphérique, périphérique perdu…).
    pub device_errors: AtomicU64,
}

impl Shared {
    pub fn new() -> Self {
        Self {
            playing: AtomicBool::new(false),
            volume_bits: AtomicU32::new(DEFAULT_VOLUME.to_bits()),
            muted: AtomicBool::new(false),
            mode: AtomicU8::new(mode_to_u8(PlayMode::default())),
            tap_enabled: AtomicBool::new(false),
            engine_idle: AtomicBool::new(false),
            track: AtomicU32::new(NO_TRACK),
            position_frames: AtomicU64::new(0),
            duration_frames: AtomicU64::new(0),
            rendered_frames: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            tap_overflows: AtomicU64::new(0),
            device_errors: AtomicU64::new(0),
        }
    }

    /// Volume général 0..1 (avant la courbe quadratique).
    pub fn volume(&self) -> f32 {
        f32::from_bits(self.volume_bits.load(Ordering::Relaxed))
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume_bits.store(volume.to_bits(), Ordering::Relaxed);
    }

    /// Gain linéaire visé par le rendu : 0 en sourdine, sinon `volume²` (Sillage : « courbe quadratique, plus
    /// naturelle à l'oreille qu'une rampe linéaire »).
    pub fn target_gain(&self) -> f32 {
        if self.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            let volume = self.volume();
            volume * volume
        }
    }

    pub fn mode(&self) -> PlayMode {
        match self.mode.load(Ordering::Relaxed) {
            1 => PlayMode::Sequence,
            2 => PlayMode::Loop,
            _ => PlayMode::Random,
        }
    }

    pub fn set_mode(&self, mode: PlayMode) {
        self.mode.store(mode_to_u8(mode), Ordering::Relaxed);
    }
}

fn mode_to_u8(mode: PlayMode) -> u8 {
    match mode {
        PlayMode::Random => 0,
        PlayMode::Sequence => 1,
        PlayMode::Loop => 2,
    }
}
