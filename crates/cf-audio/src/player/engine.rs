//! Thread du décodeur : tient la playlist, décode la piste courante et remplit le ring audio d'avance.
//!
//! Il n'est pas temps réel (il alloue, journalise, lit le disque) mais ne bloque jamais le rendu : quand le ring est
//! plein, il attend une commande avec un délai court au lieu de tourner à vide.
//!
//! Enchaînement sans trou : à la fin d'une piste, la suivante est ouverte aussitôt et ses trames suivent les
//! dernières de la précédente dans le ring, séparées par un [`Marker`] non `flush`. Le ring garde
//! [`super::RING_SECONDS`] d'avance pour couvrir l'ouverture du fichier suivant.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};
use rtrb::Producer;

use super::error::PlayerError;
use super::shared::{Command, Marker, NO_TRACK, Shared};
use super::source::TrackSource;
use crate::playlist::{Playlist, Step};

/// Attente maximale quand le ring est plein : le rendu libère ~240 trames par 5 ms à 48 kHz, inutile de revenir
/// plus souvent ; une commande réveille le thread aussitôt.
const WAIT: Duration = Duration::from_millis(5);

/// Le thread du décodeur et ce qu'il possède.
pub(crate) struct Engine {
    pub playlist: Playlist,
    pub rate_out: u32,
    pub audio: Producer<f32>,
    pub markers: Producer<Marker>,
    pub commands: Receiver<Command>,
    pub shared: Arc<Shared>,
}

/// État de travail du thread.
struct Worker {
    engine: Engine,
    source: Option<TrackSource>,
    /// Indice de la piste ouverte dans `source`.
    source_track: usize,
    /// Trames décodées pas encore poussées dans le ring (entrelacées), à partir de `pending_pos`.
    pending: Vec<f32>,
    pending_pos: usize,
    /// Repère à pousser dès que `pending` est vidé (son `at_frame` est fixé à ce moment-là).
    pending_marker: Option<Marker>,
    /// Trames stéréo poussées dans le ring depuis le démarrage.
    pushed_frames: u64,
    /// Trames produites par la piste courante (une piste vide compte comme un échec).
    track_frames: u64,
    /// Échecs consécutifs (ouverture, piste vide) : au-delà du nombre de pistes, on abandonne.
    failures: usize,
}

impl Engine {
    pub fn spawn(self) -> Result<JoinHandle<()>, PlayerError> {
        std::thread::Builder::new()
            .name("cf-audio-decoder".into())
            .spawn(move || Worker::new(self).run())
            .map_err(PlayerError::Spawn)
    }
}

impl Worker {
    fn new(engine: Engine) -> Self {
        Self {
            engine,
            source: None,
            source_track: 0,
            pending: Vec::with_capacity(16 * 1024),
            pending_pos: 0,
            pending_marker: None,
            pushed_frames: 0,
            track_frames: 0,
            failures: 0,
        }
    }

    fn run(mut self) {
        match self.engine.playlist.index() {
            Some(index) => self.open(index, false),
            None => self.set_idle(true),
        }
        loop {
            while let Ok(command) = self.engine.commands.try_recv() {
                if !self.handle(command) {
                    return;
                }
            }
            if !self.drain_pending() || !self.push_marker() {
                if !self.wait() {
                    return;
                }
                continue;
            }
            let Some(source) = &mut self.source else {
                // Rien à décoder : on dort jusqu'à la prochaine commande.
                match self.engine.commands.recv() {
                    Ok(command) if self.handle(command) => continue,
                    _ => return,
                }
            };
            let before = self.pending.len();
            match source.read(&mut self.pending) {
                Ok(more) => {
                    self.track_frames += ((self.pending.len() - before) / 2) as u64;
                    if !more {
                        self.on_track_end();
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "piste interrompue, passage à la suivante");
                    self.on_track_end();
                }
            }
        }
    }

    /// Applique une commande. Rend `false` pour arrêter le thread.
    fn handle(&mut self, command: Command) -> bool {
        match command {
            Command::Shutdown => return false,
            Command::SetMode(mode) => self.engine.playlist.set_mode(mode),
            Command::Next => {
                self.resync_to_playing();
                let step = self.engine.playlist.next_track();
                self.apply(step, true);
            }
            Command::Prev => {
                self.resync_to_playing();
                let step = self.engine.playlist.prev_track(self.position_s());
                self.apply(step, true);
            }
            Command::Select(index) => {
                let step = self.engine.playlist.select(index);
                self.apply(step, true);
            }
            Command::Seek(seconds) => self.seek(seconds),
        }
        true
    }

    fn apply(&mut self, step: Option<Step>, flush: bool) {
        match step {
            Some(Step::Load(index)) => self.open(index, flush),
            Some(Step::Restart) => self.restart(flush),
            None => {}
        }
    }

    /// Le décodeur a pu prendre de l'avance sur la piste suivante alors qu'on entend encore la fin de la précédente :
    /// « précédent » et « seek » portent sur ce qu'on entend.
    fn resync_to_playing(&mut self) {
        let playing = self.engine.shared.track.load(Ordering::Relaxed);
        if playing != NO_TRACK {
            self.engine.playlist.resync(playing as usize);
        }
    }

    /// Position de lecture de la piste entendue (s).
    fn position_s(&self) -> f64 {
        self.engine.shared.position_frames.load(Ordering::Relaxed) as f64 / f64::from(self.engine.rate_out)
    }

    /// Ouvre la piste `index` (ou, si elle est illisible, les suivantes) et prépare son repère.
    fn open(&mut self, mut index: usize, flush: bool) {
        if flush {
            self.discard_pending();
        }
        let count = self.engine.playlist.tracks().len();
        loop {
            let Some(track) = self.engine.playlist.tracks().get(index).cloned() else {
                return;
            };
            match TrackSource::open(&track.path, self.engine.rate_out) {
                Ok(source) => {
                    tracing::info!(index, title = %track.title, artist = %track.artist, "piste chargée");
                    self.queue_marker(index, 0, source.duration_frames(), flush);
                    self.source = Some(source);
                    self.source_track = index;
                    self.track_frames = 0;
                    self.set_idle(false);
                    return;
                }
                Err(err) => {
                    tracing::warn!(path = ?track.path, error = %err, "piste illisible, ignorée");
                    if !self.count_failure(count) {
                        return;
                    }
                    match self.engine.playlist.next_track() {
                        Some(Step::Load(next)) => index = next,
                        _ => return,
                    }
                }
            }
        }
    }

    /// Relance la piste courante au début (mode boucle, ou « précédent » après 3 s).
    fn restart(&mut self, flush: bool) {
        let index = self.engine.playlist.index().unwrap_or(0);
        if flush {
            self.discard_pending();
        }
        if let Some(source) = self.source.as_mut().filter(|_| self.source_track == index)
            && let Ok(start) = source.seek(0.0)
        {
            let duration = source.duration_frames();
            self.track_frames = 0;
            self.queue_marker(index, start, duration, flush);
            return;
        }
        // Pas de source sur cette piste, ou flux impossible à rembobiner : on la rouvre.
        self.open(index, flush);
    }

    fn seek(&mut self, seconds: f64) {
        self.resync_to_playing();
        let Some(index) = self.engine.playlist.index() else {
            return;
        };
        if self.source.is_none() || self.source_track != index {
            self.open(index, true);
        }
        let Some(source) = &mut self.source else { return };
        let duration_s = source.duration_frames() as f64 / f64::from(self.engine.rate_out);
        let seconds = if duration_s > 0.0 {
            seconds.clamp(0.0, (duration_s - 0.05).max(0.0))
        } else {
            seconds.max(0.0)
        };
        match source.seek(seconds) {
            Ok(start) => {
                let duration = source.duration_frames();
                self.discard_pending();
                self.queue_marker(index, start, duration, true);
            }
            Err(err) => tracing::warn!(seconds, error = %err, "seek impossible, ignoré"),
        }
    }

    fn on_track_end(&mut self) {
        let count = self.engine.playlist.tracks().len();
        if self.track_frames == 0 {
            if !self.count_failure(count) {
                return;
            }
        } else {
            self.failures = 0;
        }
        let step = self.engine.playlist.on_track_end();
        self.apply(step, false);
    }

    /// Compte un échec ; rend `false` (et se met au repos) quand toutes les pistes ont échoué d'affilée.
    fn count_failure(&mut self, count: usize) -> bool {
        self.failures += 1;
        if self.failures >= count.max(1) {
            tracing::warn!("aucune piste lisible : lecteur au repos");
            self.source = None;
            self.set_idle(true);
            return false;
        }
        true
    }

    fn queue_marker(&mut self, track: usize, start_frame: u64, duration_frames: u64, flush: bool) {
        // Un repère non encore poussé est remplacé : il décrivait un signal qui ne sera plus joué, ou il est rendu
        // obsolète par celui-ci (son `flush` est conservé, le saut reste un saut).
        let flush = flush || self.pending_marker.is_some_and(|m| m.flush);
        self.pending_marker = Some(Marker {
            at_frame: 0,
            track: track as u32,
            start_frame,
            duration_frames,
            flush,
        });
    }

    /// Jette ce qui n'a pas encore été poussé : un saut rend ce signal inutile.
    fn discard_pending(&mut self) {
        self.pending.clear();
        self.pending_pos = 0;
    }

    /// Pousse `pending` dans le ring, par trames entières. Rend `true` quand tout est passé.
    fn drain_pending(&mut self) -> bool {
        let remaining = &self.pending[self.pending_pos..];
        let room = self.engine.audio.slots() & !1;
        let count = remaining.len().min(room);
        if count > 0 && self.engine.audio.push_entire_slice(&remaining[..count]).is_ok() {
            self.pending_pos += count;
            self.pushed_frames += (count / 2) as u64;
        }
        if self.pending_pos == self.pending.len() {
            self.discard_pending();
            true
        } else {
            false
        }
    }

    /// Pousse le repère en attente, daté de la position courante du ring. Rend `false` si la file est pleine.
    fn push_marker(&mut self) -> bool {
        let Some(mut marker) = self.pending_marker else {
            return true;
        };
        marker.at_frame = self.pushed_frames;
        if self.engine.markers.push(marker).is_err() {
            return false;
        }
        self.pending_marker = None;
        true
    }

    /// Attend une commande au plus [`WAIT`]. Rend `false` pour arrêter le thread.
    fn wait(&mut self) -> bool {
        match self.engine.commands.recv_timeout(WAIT) {
            Ok(command) => self.handle(command),
            Err(RecvTimeoutError::Timeout) => true,
            Err(RecvTimeoutError::Disconnected) => false,
        }
    }

    fn set_idle(&self, idle: bool) {
        self.engine.shared.engine_idle.store(idle, Ordering::Relaxed);
    }
}
