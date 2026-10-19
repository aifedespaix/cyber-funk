//! Lecteur des pistes de démo (portage de la partie lecture de `engine/audio.ts`).
//!
//! ```text
//!   Player (contrôle) ──Command──▶ thread décodeur (symphonia + rubato) ──rtrb f32 + rtrb Marker──▶ Renderer
//!        ▲                                                                                         (callback)
//!        └────────────── atomiques : piste, position, durée, sous-alimentations ◀───────────────────────┘
//!                                                                     Renderer ──rtrb f32 mono──▶ AnalysisTap
//! ```
//!
//! - Le volume et la sourdine ne s'appliquent qu'au signal envoyé à la sortie, jamais à la voie d'analyse (Sillage :
//!   « l'analyseur lit le signal avant le volume »).
//! - La position est un compteur de trames tenu par le callback : elle correspond à ce qui part vers la carte son,
//!   pas à ce que le décodeur a lu d'avance.
//! - La sortie est injectable ([`OutputBackend`]) : tout se teste sans carte son avec [`NullOutput`] ou
//!   [`ManualOutput`].

mod engine;
pub mod error;
pub mod output;
mod render;
mod resample;
mod shared;
mod source;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;

use crossbeam_channel::Sender;

pub use error::{PlayerError, Result};
pub use output::{ActiveOutput, CpalOutput, ManualDriver, ManualOutput, NullOutput, OutputBackend};
pub use render::{DeviceErrorReporter, Renderer};

use self::engine::Engine;
use self::shared::{Command, NO_TRACK, Shared};
use crate::playlist::{PlayMode, Playlist, Track};

/// Avance du décodeur sur la sortie (s). Couvre l'ouverture du fichier suivant à la fin d'une piste ; un saut ou un
/// seek ne l'attend pas (le rendu jette l'ancien signal).
pub const RING_SECONDS: f32 = 0.5;

/// Capacité de la voie d'analyse (s) : le thread d'analyse lit par pas de ~10 ms, une seconde laisse de la marge.
pub const TAP_SECONDS: f32 = 1.0;

/// Capacité de la file de repères : un par piste ou par saut, 64 suffit même en martelant « suivant ».
const MARKER_CAPACITY: usize = 64;

/// Capacité du canal de commandes : au-delà, une commande est perdue (et journalisée) plutôt que de bloquer
/// l'appelant (le thread de l'interface).
const COMMAND_CAPACITY: usize = 64;

/// Réglages du lecteur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerConfig {
    /// Mode de lecture initial.
    pub mode: PlayMode,
    /// Volume initial 0..1 (Sillage : 0,8).
    pub volume: f32,
    /// Sourdine initiale.
    pub muted: bool,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            mode: PlayMode::default(),
            volume: shared::DEFAULT_VOLUME,
            muted: false,
        }
    }
}

/// Instantané observable du lecteur (pour l'interface opérateur).
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    /// Lecture demandée (le son peut encore être en fondu ou en attente de signal).
    pub playing: bool,
    /// Indice de la piste entendue (`None` avant la première piste ou si la liste est vide).
    pub track: Option<usize>,
    /// Position dans la piste entendue (s), tirée du compteur de trames du callback.
    pub position_s: f64,
    /// Position exacte en trames à la fréquence de sortie.
    pub position_frames: u64,
    /// Durée de la piste (s), si le fichier la donne.
    pub duration_s: Option<f64>,
    /// Volume 0..1.
    pub volume: f32,
    /// Sourdine.
    pub muted: bool,
    /// Mode de lecture.
    pub mode: PlayMode,
    /// Trames rendues vers la sortie depuis l'ouverture (horloge audio du lecteur).
    pub rendered_frames: u64,
    /// Sous-alimentations : le callback a manqué de signal alors que le décodeur avait encore à faire.
    pub underruns: u64,
    /// Échantillons perdus par la voie d'analyse (ring plein).
    pub tap_overflows: u64,
    /// Erreurs signalées par le pilote de sortie (xrun du périphérique, perte…).
    pub device_errors: u64,
}

/// Voie d'analyse : le signal joué, en mono, avant volume et sourdine, à la fréquence de la sortie. Un échantillon
/// par trame rendue, silences de pause compris : l'échantillon `n` correspond à la trame de sortie `n`, tant que le
/// lecteur suit (sinon [`PlayerState::tap_overflows`] augmente).
#[derive(Debug)]
pub struct AnalysisTap {
    /// Échantillons mono.
    pub consumer: rtrb::Consumer<f32>,
    /// Fréquence d'échantillonnage (Hz).
    pub sample_rate: u32,
}

/// Lecteur de démos. Toutes les méthodes sont non bloquantes (atomiques ou canal borné) : on peut les appeler depuis
/// un système Bevy.
#[derive(Debug)]
pub struct Player {
    commands: Sender<Command>,
    shared: Arc<Shared>,
    tracks: Arc<[Track]>,
    sample_rate: u32,
    tap: Option<rtrb::Consumer<f32>>,
    engine: Option<JoinHandle<()>>,
    /// Sortie démarrée, gardée pour la durée de vie du lecteur (arrêtée en premier dans `Drop`).
    output: Option<Box<dyn ActiveOutput>>,
}

impl Player {
    /// Lance le décodeur sur `playlist` et démarre `output`. La lecture commence en pause, sur le titre courant de la
    /// playlist, déjà décodé d'avance : [`Player::play`] est immédiat.
    ///
    /// # Errors
    ///
    /// Rend une erreur si le thread du décodeur ne démarre pas ou si la sortie ne s'ouvre pas.
    pub fn new(mut playlist: Playlist, config: PlayerConfig, output: Box<dyn OutputBackend>) -> Result<Self> {
        let sample_rate = output.sample_rate().max(1);
        let ring_frames = (RING_SECONDS * sample_rate as f32) as usize;
        let (audio_tx, audio_rx) = rtrb::RingBuffer::new(ring_frames * 2);
        let (marker_tx, marker_rx) = rtrb::RingBuffer::new(MARKER_CAPACITY);
        let (tap_tx, tap_rx) = rtrb::RingBuffer::new((TAP_SECONDS * sample_rate as f32) as usize);
        let (command_tx, command_rx) = crossbeam_channel::bounded(COMMAND_CAPACITY);

        let shared = Arc::new(Shared::new());
        shared.set_volume(config.volume.clamp(0.0, 1.0));
        shared.muted.store(config.muted, Ordering::Relaxed);
        shared.set_mode(config.mode);
        playlist.set_mode(config.mode);
        let tracks = Arc::clone(playlist.tracks());

        let renderer = Renderer::new(audio_rx, marker_rx, tap_tx, Arc::clone(&shared), sample_rate);
        let engine = Engine {
            playlist,
            rate_out: sample_rate,
            audio: audio_tx,
            markers: marker_tx,
            commands: command_rx,
            shared: Arc::clone(&shared),
        }
        .spawn()?;
        let mut player = Self {
            commands: command_tx,
            shared,
            tracks,
            sample_rate,
            tap: Some(tap_rx),
            engine: Some(engine),
            output: None,
        };
        // En cas d'échec, `player` est lâché ici et son `Drop` arrête proprement le décodeur.
        player.output = Some(output.start(renderer)?);
        Ok(player)
    }

    /// Lance ou reprend la lecture (fondu d'entrée de 5 ms).
    pub fn play(&self) {
        self.shared.playing.store(true, Ordering::Relaxed);
    }

    /// Met en pause (fondu de sortie de 5 ms). La voie d'analyse reçoit alors du silence.
    pub fn pause(&self) {
        self.shared.playing.store(false, Ordering::Relaxed);
    }

    /// Bascule lecture/pause.
    pub fn toggle(&self) {
        self.shared.playing.fetch_xor(true, Ordering::Relaxed);
    }

    /// Titre suivant selon le mode de lecture.
    pub fn next(&self) {
        self.send(Command::Next);
    }

    /// Titre précédent, ou début du titre si on en a écouté plus de 3 s.
    pub fn prev(&self) {
        self.send(Command::Prev);
    }

    /// Choisit un titre par son indice (fait le tour de la liste).
    pub fn select(&self, index: isize) {
        self.send(Command::Select(index));
    }

    /// Se place à `position_s` secondes dans le titre entendu (borné à sa durée).
    pub fn seek(&self, position_s: f64) {
        if position_s.is_finite() {
            self.send(Command::Seek(position_s));
        }
    }

    /// Règle le volume général 0..1 (courbe quadratique, lissée sur 30 ms). N'affecte pas la voie d'analyse.
    pub fn set_volume(&self, volume: f32) {
        if volume.is_finite() {
            self.shared.set_volume(volume.clamp(0.0, 1.0));
        }
    }

    /// Coupe ou rétablit le son. N'affecte pas la voie d'analyse.
    pub fn set_muted(&self, muted: bool) {
        self.shared.muted.store(muted, Ordering::Relaxed);
    }

    /// Change le mode de lecture (prend effet à la prochaine fin de titre).
    pub fn set_mode(&self, mode: PlayMode) {
        self.shared.set_mode(mode);
        self.send(Command::SetMode(mode));
    }

    /// Pistes de la playlist.
    #[must_use]
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Fréquence de sortie (Hz).
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Prend la voie d'analyse (une seule fois). Le rendu n'y pousse qu'à partir de cet appel.
    pub fn take_analysis_tap(&mut self) -> Option<AnalysisTap> {
        let consumer = self.tap.take()?;
        self.shared.tap_enabled.store(true, Ordering::Relaxed);
        Some(AnalysisTap {
            consumer,
            sample_rate: self.sample_rate,
        })
    }

    /// Instantané de l'état du lecteur.
    #[must_use]
    pub fn state(&self) -> PlayerState {
        let s = &self.shared;
        let rate = f64::from(self.sample_rate);
        let track = s.track.load(Ordering::Relaxed);
        let position_frames = s.position_frames.load(Ordering::Relaxed);
        let duration_frames = s.duration_frames.load(Ordering::Relaxed);
        PlayerState {
            playing: s.playing.load(Ordering::Relaxed),
            track: (track != NO_TRACK).then_some(track as usize),
            position_s: position_frames as f64 / rate,
            position_frames,
            duration_s: (duration_frames > 0).then(|| duration_frames as f64 / rate),
            volume: s.volume(),
            muted: s.muted.load(Ordering::Relaxed),
            mode: s.mode(),
            rendered_frames: s.rendered_frames.load(Ordering::Relaxed),
            underruns: s.underruns.load(Ordering::Relaxed),
            tap_overflows: s.tap_overflows.load(Ordering::Relaxed),
            device_errors: s.device_errors.load(Ordering::Relaxed),
        }
    }

    fn send(&self, command: Command) {
        if self.commands.try_send(command).is_err() {
            tracing::warn!(?command, "commande du lecteur perdue (décodeur arrêté ou débordé)");
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // La sortie d'abord : plus personne ne lit le ring, puis le décodeur.
        self.output = None;
        // Canal plein : le décodeur finira de toute façon en voyant l'émetteur lâché, à son prochain `recv`.
        let _ = self.commands.try_send(Command::Shutdown);
        if let Some(engine) = self.engine.take() {
            let (tx, _) = crossbeam_channel::bounded(0);
            // Remplacer l'émetteur le lâche : le décodeur sort de `recv` s'il y dormait.
            self.commands = tx;
            if engine.join().is_err() {
                tracing::warn!("le thread du décodeur s'est arrêté en paniquant");
            }
        }
    }
}
