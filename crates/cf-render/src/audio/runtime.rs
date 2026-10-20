//! Les threads audio et leur pilotage : analyse, capture, lecteur de démo.
//!
//! ```text
//! Capture ─(FeedSink = AnalysisControl)─┐
//! Player ──take_analysis_feed───────────┴▶ AnalysisThread ──▶ AudioBridge ──▶ Res<AudioFeatures>, Kick, …
//! ```
//!
//! `cf-audio` ne peut pas brancher ses flux sur l'analyse (`cf-analysis` dépend de lui) : c'est ici que
//! `AnalysisControl::attach` est appelé, pour le lecteur ; la capture reçoit une télécommande comme `FeedSink`.
//!
//! **Rien ne bloque l'image.** Tous les ordres sont des envois sur canaux bornés lus par des threads qui tournent à
//! autre chose qu'attendre le système audio : ouvrir un micro peut prendre une seconde, c'est le superviseur de capture
//! qui l'attend, pas le thread de Bevy. L'analyse publie `live = false` pendant ce temps et l'image continue.

use std::sync::Arc;

use cf_analysis::{AnalysisConfig, AnalysisControl, AnalysisError, AnalysisThread, BeatAnalyzer};
use cf_audio::capture::{
    Capture, CaptureConfig, CaptureError, CpalBackend, DeviceSelector, InputBackend, InputDeviceInfo,
};
use cf_audio::player::{CpalOutput, NullOutput, OutputBackend, Player, PlayerConfig};
use cf_audio::playlist::{Playlist, Track, scan_music_dir};
use cf_audio::source::SourceKind;

use super::bridge::AudioBridge;
use super::config::{AudioConfig, InputBackendChoice, OutputChoice};
use super::types::{AudioCommand, AudioSource, AudioStatus};

/// Période de rafraîchissement de l'instantané du panneau opérateur (s).
pub(super) const STATUS_PERIOD_S: f64 = 0.1;

/// Fréquence d'échantillonnage de la sortie nulle : celle du pipeline d'analyse.
const NULL_OUTPUT_RATE: u32 = 48_000;

/// Threads audio et état du pont. Ressource « non-send » : le lecteur garde la sortie `cpal`, qui n'est pas `Sync`.
///
/// L'ordre des champs est l'ordre d'arrêt : capture et lecteur d'abord (ils parlent à l'analyse), l'analyse en dernier.
pub struct AudioRuntime {
    capture: Option<Capture>,
    player: Option<Player>,
    bridge: AudioBridge,
    control: AnalysisControl,
    input: InputBackendChoice,
    tracks: Arc<[Track]>,
    source: AudioSource,
    /// Ce qui a été demandé aux threads la dernière fois : évite de renvoyer le même ordre à chaque image.
    applied: Option<(AudioSource, DeviceSelector)>,
    mic_device: DeviceSelector,
    line_device: DeviceSelector,
    /// Périphérique que la capture a reçu en dernier.
    capture_device: Option<DeviceSelector>,
    last_track: Option<usize>,
    /// Choisir la source Démo lance la lecture. Faux seulement pour le tout premier choix quand `autoplay` est coupé.
    play_on_demo: bool,
    devices: Vec<String>,
    notes: Vec<String>,
    pub(super) last_status_s: f64,
    // Dernier champ : arrêté après tous les autres.
    analysis: AnalysisThread,
}

impl std::fmt::Debug for AudioRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AudioRuntime")
            .field("source", &self.source)
            .field("capture", &self.capture.is_some())
            .field("player", &self.player.is_some())
            .finish_non_exhaustive()
    }
}

impl AudioRuntime {
    /// Démarre l'analyse et le lecteur de démo, choisit la source de départ. La capture n'est ouverte qu'à la première
    /// demande de micro ou de ligne : un opérateur qui ne joue que des démos n'a pas son micro ouvert pour rien.
    ///
    /// Un lecteur qui ne démarre pas (pas de carte son, aucune piste) n'est pas une erreur : la source Démo devient
    /// indisponible et la raison se lit dans [`AudioStatus::notes`].
    ///
    /// # Errors
    ///
    /// [`AnalysisError`] si le thread d'analyse ne peut pas être créé : sans lui il n'y a rien à pont.
    pub fn start(config: &AudioConfig) -> Result<Self, AnalysisError> {
        let analysis_config = AnalysisConfig {
            // Aucune source tant que `apply_source` n'a pas tranché : pas de bascule parasite au démarrage.
            initial_source: None,
            ..AnalysisConfig::default()
        };
        let (analysis, outputs) = AnalysisThread::spawn(BeatAnalyzer::new(), analysis_config)?;
        let control = analysis.control();
        let mut notes = Vec::new();
        let (player, tracks) = start_player(config, &control, &mut notes);

        let mut runtime = Self {
            capture: None,
            player,
            bridge: AudioBridge::new(outputs),
            control,
            input: config.input.clone(),
            tracks,
            source: config.initial_source,
            applied: None,
            mic_device: config.mic_device.clone(),
            line_device: config.line_device.clone(),
            capture_device: None,
            last_track: None,
            play_on_demo: config.autoplay,
            devices: Vec::new(),
            notes,
            last_status_s: f64::NEG_INFINITY,
            analysis,
        };
        if runtime.source == AudioSource::Demo && runtime.player.is_none() {
            runtime
                .notes
                .push("source Démo indisponible : repli sur le micro".to_owned());
            runtime.source = AudioSource::Microphone;
        }
        runtime.reconcile();
        runtime.play_on_demo = true;
        Ok(runtime)
    }

    /// Le pont de lecture (système `pump_audio`).
    pub(super) fn bridge_mut(&mut self) -> &mut AudioBridge {
        &mut self.bridge
    }

    /// Applique un ordre de l'opérateur. Les changements de source sont regroupés par [`AudioRuntime::reconcile`],
    /// à appeler une fois après la série d'ordres d'une image.
    pub(super) fn apply(&mut self, command: &AudioCommand) {
        match command {
            AudioCommand::SelectSource(source) => self.source = *source,
            AudioCommand::SelectDevice { source, device } => match source {
                AudioSource::Microphone => self.mic_device = device.clone(),
                AudioSource::Line => self.line_device = device.clone(),
                AudioSource::Demo => {}
            },
            AudioCommand::RefreshDevices => self.refresh_devices(),
            AudioCommand::DemoPlay => self.with_player(Player::play),
            AudioCommand::DemoPause => self.with_player(Player::pause),
            AudioCommand::DemoToggle => self.with_player(Player::toggle),
            AudioCommand::DemoNext => self.with_player(Player::next),
            AudioCommand::DemoPrev => self.with_player(Player::prev),
            AudioCommand::DemoSelect(index) => {
                let index = isize::try_from(*index).unwrap_or(isize::MAX);
                self.with_player(|p| p.select(index));
            }
            AudioCommand::DemoVolume(volume) => {
                let volume = *volume;
                self.with_player(|p| p.set_volume(volume));
            }
        }
    }

    fn with_player(&self, f: impl FnOnce(&Player)) {
        if let Some(player) = &self.player {
            f(player);
        }
    }

    /// Met les threads d'accord avec la source demandée, et remet l'analyse à zéro quand la piste change.
    /// N'envoie rien si rien n'a changé depuis la dernière fois.
    pub(super) fn reconcile(&mut self) {
        let device = match self.source {
            AudioSource::Microphone => self.mic_device.clone(),
            AudioSource::Line => self.line_device.clone(),
            AudioSource::Demo => DeviceSelector::Default,
        };
        let wanted = (self.source, device);
        if self.applied.as_ref() != Some(&wanted) {
            self.apply_source(wanted.0, &wanted.1);
            self.applied = Some(wanted);
        }
        if let Some(player) = &self.player {
            let track = player.state().track;
            if track != self.last_track {
                self.last_track = track;
                if self.source == AudioSource::Demo {
                    // Nouvelle piste : tempo et ambiance de la précédente n'ont plus de sens (`beat.reset()` de Sillage).
                    self.control.reset_analysis();
                }
            }
        }
    }

    fn apply_source(&mut self, source: AudioSource, device: &DeviceSelector) {
        tracing::info!(source = source.label(), device = %device, "audio : changement de source");
        match source {
            AudioSource::Microphone | AudioSource::Line => {
                self.ensure_capture(device);
                // Le gain automatique rattrape un micro qui capte la musique à -30 dB ; une entrée ligne est déjà au
                // niveau d'un fichier, l'amplifier y grossirait le bruit de fond des silences.
                self.control.set_auto_gain(source == AudioSource::Microphone);
                self.control.select(Some(SourceKind::Capture));
                // La démo ne doit pas continuer à jouer par-dessus ce qu'on capte (et rentrer dans le micro).
                self.with_player(Player::pause);
            }
            AudioSource::Demo => {
                self.control.select(Some(SourceKind::Demo));
                if self.play_on_demo {
                    self.with_player(Player::play);
                }
            }
        }
    }

    /// Ouvre la capture au premier besoin, puis ne la rouvre que si le périphérique change.
    fn ensure_capture(&mut self, device: &DeviceSelector) {
        if self.capture_device.as_ref() == Some(device) {
            return;
        }
        match &self.capture {
            Some(capture) => capture.select(device.clone()),
            None => {
                self.capture = self.start_capture(device);
            }
        }
        self.capture_device = Some(device.clone());
    }

    fn start_capture(&mut self, device: &DeviceSelector) -> Option<Capture> {
        let config = CaptureConfig {
            device: device.clone(),
            ..CaptureConfig::default()
        };
        let sink = self.control.clone();
        let result = match &self.input {
            InputBackendChoice::Cpal => Capture::start(CpalBackend::new(), config, sink),
            InputBackendChoice::Synthetic(bus) => Capture::start(bus.backend(), config, sink),
            InputBackendChoice::Disabled => {
                self.notes
                    .push("aucune entrée audio : micro et ligne ne produisent rien".to_owned());
                return None;
            }
        };
        match result {
            Ok(capture) => Some(capture),
            Err(error) => {
                tracing::error!(%error, "audio : capture impossible à démarrer");
                self.notes.push(format!("capture impossible à démarrer : {error}"));
                None
            }
        }
    }

    /// Relit la liste des périphériques d'entrée. Appelée sur demande de l'opérateur et au démarrage, jamais à l'image.
    pub(super) fn refresh_devices(&mut self) {
        match self.list_devices() {
            Ok(devices) => self.devices = devices.into_iter().map(|d| d.name).collect(),
            Err(error) => {
                tracing::warn!(%error, "audio : liste des périphériques d'entrée illisible");
                self.devices.clear();
                self.notes.push(format!("périphériques d'entrée illisibles : {error}"));
            }
        }
    }

    fn list_devices(&self) -> Result<Vec<InputDeviceInfo>, CaptureError> {
        match &self.input {
            InputBackendChoice::Cpal => CpalBackend::new().input_devices(),
            InputBackendChoice::Synthetic(bus) => bus.backend().input_devices(),
            InputBackendChoice::Disabled => Ok(Vec::new()),
        }
    }

    /// Remplit l'instantané du panneau opérateur.
    pub(super) fn fill_status(&self, status: &mut AudioStatus) {
        status.source = self.source;
        status.analysis_running = true;
        status.capture = self.capture.as_ref().map(Capture::state);
        status.demo = self.player.as_ref().map(Player::state);
        status.tracks = Arc::clone(&self.tracks);
        status.analysis = self.analysis.stats();
        status.devices.clone_from(&self.devices);
        status.mic_device.clone_from(&self.mic_device);
        status.line_device.clone_from(&self.line_device);
        status.notes.clone_from(&self.notes);
    }
}

/// Démarre le lecteur de démo sur les pistes de `config.music_dir` et branche son flux sur l'analyse.
fn start_player(
    config: &AudioConfig,
    control: &AnalysisControl,
    notes: &mut Vec<String>,
) -> (Option<Player>, Arc<[Track]>) {
    let tracks = match scan_music_dir(&config.music_dir) {
        Ok(tracks) => tracks,
        Err(error) => {
            tracing::warn!(%error, "audio : dossier de pistes illisible");
            notes.push(format!("pistes illisibles : {error}"));
            Vec::new()
        }
    };
    let output: Box<dyn OutputBackend> = match config.output {
        OutputChoice::Disabled => return (None, Arc::from(tracks)),
        OutputChoice::Null { speed } => Box::new(NullOutput::new(NULL_OUTPUT_RATE).with_speed(speed)),
        OutputChoice::Default => match CpalOutput::default_device() {
            Ok(output) => Box::new(output),
            Err(error) => {
                tracing::warn!(%error, "audio : pas de sortie audio, lecteur de démo indisponible");
                notes.push(format!("pas de sortie audio : {error}"));
                return (None, Arc::from(tracks));
            }
        },
    };
    if tracks.is_empty() {
        notes.push(format!("aucune piste dans {}", config.music_dir.display()));
        return (None, Arc::from(tracks));
    }
    let playlist = Playlist::new(tracks.clone(), None, config.playlist_seed);
    let mut player = match Player::new(playlist, PlayerConfig::default(), output) {
        Ok(player) => player,
        Err(error) => {
            tracing::warn!(%error, "audio : lecteur de démo impossible à démarrer");
            notes.push(format!("lecteur de démo impossible à démarrer : {error}"));
            return (None, Arc::from(tracks));
        }
    };
    if let Some(feed) = player.take_analysis_feed() {
        control.attach(feed);
    }
    (Some(player), Arc::from(tracks))
}
