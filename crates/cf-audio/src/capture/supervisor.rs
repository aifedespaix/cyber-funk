//! Boucle du superviseur de capture : ouverture par nom, détection de perte, reconnexion.
//!
//! Ce thread n'est pas temps réel : il peut allouer, verrouiller (l'état partagé) et journaliser. Il ne touche jamais
//! aux échantillons ; il lit seulement les compteurs atomiques du flux ouvert.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError};

use super::{CaptureConfig, CaptureState, DeviceSelector, FeedSink, InputBackend, InputFormat};
use crate::feed::FeedStats;

/// Période de surveillance : délai maximal entre une panne signalée et la fermeture du flux par le superviseur.
/// (L'analyse, elle, voit la panne à son pas suivant, sans attendre ce délai.)
const TICK: Duration = Duration::from_millis(50);

/// Ordres envoyés au superviseur.
#[derive(Debug)]
pub(super) enum Command {
    Select(DeviceSelector),
    Stop,
}

/// Flux ouvert et ce qu'il faut pour le surveiller.
struct Connected<S> {
    _stream: S,
    stats: Arc<FeedStats>,
    device: String,
    format: InputFormat,
    callbacks: u64,
    progress_at: Instant,
}

pub(super) struct Supervisor<B: InputBackend, K: FeedSink> {
    backend: B,
    config: CaptureConfig,
    sink: K,
    commands: Receiver<Command>,
    state: Arc<Mutex<CaptureState>>,
    connected: Option<Connected<B::Stream>>,
    failures: u32,
    lost_device: Option<String>,
    next_retry: Instant,
}

impl<B: InputBackend, K: FeedSink> Supervisor<B, K> {
    pub(super) fn new(
        backend: B,
        config: CaptureConfig,
        sink: K,
        commands: Receiver<Command>,
        state: Arc<Mutex<CaptureState>>,
    ) -> Self {
        Self {
            backend,
            config,
            sink,
            commands,
            state,
            connected: None,
            failures: 0,
            lost_device: None,
            next_retry: Instant::now(),
        }
    }

    pub(super) fn run(mut self) {
        tracing::debug!(device = %self.config.device, "capture : superviseur démarré");
        self.try_open(Instant::now());
        loop {
            match self.commands.recv_timeout(TICK) {
                Ok(Command::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(Command::Select(device)) => {
                    tracing::info!(%device, "capture : changement de périphérique");
                    self.connected = None;
                    self.config.device = device;
                    self.failures = 0;
                    self.lost_device = None;
                    self.try_open(Instant::now());
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            let now = Instant::now();
            self.check_health(now);
            if self.connected.is_none() && now >= self.next_retry {
                self.try_open(now);
            }
        }
        // Fermer le flux ici, sur le thread qui l'a ouvert.
        self.connected = None;
        self.set_state(CaptureState::Stopped);
        tracing::debug!("capture : superviseur arrêté");
    }

    fn try_open(&mut self, now: Instant) {
        match self.backend.open(&self.config.device, self.config.ring_seconds) {
            Ok(opened) => {
                if let Some(lost) = self.lost_device.take() {
                    tracing::info!(
                        device = %opened.device,
                        previous = %lost,
                        attempts = self.failures,
                        format = %opened.format,
                        "capture : périphérique reconnecté"
                    );
                } else {
                    tracing::info!(device = %opened.device, format = %opened.format, "capture : périphérique ouvert");
                }
                let stats = Arc::clone(opened.feed.stats());
                self.sink.attach(opened.feed);
                self.set_state(CaptureState::Connected {
                    device: opened.device.clone(),
                    format: opened.format,
                });
                self.connected = Some(Connected {
                    _stream: opened.stream,
                    callbacks: stats.callbacks(),
                    stats,
                    device: opened.device,
                    format: opened.format,
                    progress_at: now,
                });
                self.failures = 0;
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                // Une alerte à la première tentative, puis le silence (debug) : pas de rafale de journaux pendant
                // qu'une interface reste débranchée.
                if self.failures == 1 {
                    tracing::warn!(
                        device = %self.config.device,
                        %error,
                        retry_ms = self.config.retry_interval.as_millis(),
                        "capture : ouverture impossible, nouvelles tentatives en attendant le périphérique"
                    );
                } else {
                    tracing::debug!(device = %self.config.device, %error, attempts = self.failures, "capture : toujours absent");
                }
                self.next_retry = now + self.config.retry_interval;
                self.set_state(CaptureState::Waiting {
                    reason: error.to_string(),
                    attempts: self.failures,
                });
            }
        }
    }

    /// Ferme le flux s'il est en panne ou figé ; la reconnexion suit au prochain essai.
    fn check_health(&mut self, now: Instant) {
        let Some(connected) = self.connected.as_mut() else {
            return;
        };
        let reason = if let Some(fault) = connected.stats.fault() {
            fault.to_string()
        } else {
            let callbacks = connected.stats.callbacks();
            if callbacks != connected.callbacks {
                connected.callbacks = callbacks;
                connected.progress_at = now;
                return;
            }
            if now.duration_since(connected.progress_at) < self.config.stall_timeout {
                return;
            }
            format!(
                "plus aucun échantillon depuis {} ms",
                self.config.stall_timeout.as_millis()
            )
        };
        tracing::warn!(
            device = %connected.device,
            format = %connected.format,
            %reason,
            dropped = connected.stats.dropped(),
            "capture : flux perdu, reconnexion par nom"
        );
        self.lost_device = Some(connected.device.clone());
        self.connected = None;
        self.next_retry = now + self.config.retry_interval;
        self.set_state(CaptureState::Waiting { reason, attempts: 0 });
    }

    fn set_state(&self, state: CaptureState) {
        *self.state.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use crossbeam_channel::Receiver;

    use super::super::{Capture, SampleType, Signal, SyntheticBus, SyntheticDevice};
    use super::*;
    use crate::feed::InputFeed;

    fn wait_for(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + timeout;
        while !done() {
            assert!(Instant::now() < deadline, "délai dépassé : {what}");
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn setup(name: &str) -> (SyntheticBus, Capture, Receiver<InputFeed>) {
        let bus = SyntheticBus::new();
        let format = InputFormat {
            sample_rate: 48_000,
            channels: 2,
            sample: SampleType::F32,
        };
        bus.add(SyntheticDevice::new(name, format, Signal::Silence));
        let (tx, rx) = crossbeam_channel::unbounded();
        let config = CaptureConfig {
            device: DeviceSelector::Name(name.into()),
            retry_interval: Duration::from_millis(20),
            ..CaptureConfig::default()
        };
        let capture = Capture::start(bus.backend(), config, move |feed| {
            let _ = tx.send(feed);
        })
        .unwrap();
        (bus, capture, rx)
    }

    #[test]
    fn reconnects_by_name_after_unplug() {
        let (bus, capture, feeds) = setup("Synth USB");
        let first = feeds.recv_timeout(Duration::from_secs(2)).unwrap();
        wait_for("connexion", Duration::from_secs(2), || {
            matches!(capture.state(), CaptureState::Connected { .. })
        });
        assert!(bus.unplug("Synth USB"));
        wait_for("attente", Duration::from_secs(2), || {
            matches!(capture.state(), CaptureState::Waiting { .. })
        });
        assert!(first.stats().fault().is_some());
        wait_for("fermeture", Duration::from_secs(2), || first.is_closed());
        // Plusieurs tentatives échouent pendant que le périphérique est absent, sans rien confier à l'analyse.
        thread::sleep(Duration::from_millis(100));
        assert!(feeds.try_recv().is_err());
        assert!(bus.replug("Synth USB"));
        let second = feeds.recv_timeout(Duration::from_secs(2)).unwrap();
        wait_for("reconnexion", Duration::from_secs(2), || {
            matches!(capture.state(), CaptureState::Connected { .. })
        });
        wait_for("échantillons", Duration::from_secs(2), || second.stats().written() > 0);
        assert!(second.stats().fault().is_none());
    }

    #[test]
    fn missing_device_waits_then_connects_when_plugged() {
        let bus = SyntheticBus::new();
        let (tx, feeds) = crossbeam_channel::unbounded();
        let config = CaptureConfig {
            device: DeviceSelector::Name("Plus tard".into()),
            retry_interval: Duration::from_millis(20),
            ..CaptureConfig::default()
        };
        let capture = Capture::start(bus.backend(), config, move |feed| {
            let _ = tx.send(feed);
        })
        .unwrap();
        wait_for(
            "attente",
            Duration::from_secs(2),
            || matches!(capture.state(), CaptureState::Waiting { attempts, .. } if attempts >= 2),
        );
        let format = InputFormat {
            sample_rate: 44_100,
            channels: 1,
            sample: SampleType::I16,
        };
        bus.add(SyntheticDevice::new("Plus tard", format, Signal::Silence));
        let feed = feeds.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(feed.sample_rate(), 44_100);
        drop(capture);
        wait_for("fermeture à l'arrêt", Duration::from_secs(2), || feed.is_closed());
    }

    #[test]
    fn stalled_stream_is_reopened() {
        let bus = SyntheticBus::new();
        let format = InputFormat {
            sample_rate: 48_000,
            channels: 1,
            sample: SampleType::F32,
        };
        let mut device = SyntheticDevice::new("Muet", format, Signal::Silence);
        // Le générateur continue de tourner mais ne livre plus rien : comme un pilote qui se tait sans erreur.
        device.total_frames = Some(4_800);
        bus.add(device);
        let (tx, feeds) = crossbeam_channel::unbounded();
        let config = CaptureConfig {
            stall_timeout: Duration::from_millis(150),
            ..CaptureConfig::default()
        };
        let _capture = Capture::start(bus.backend(), config, move |feed| {
            let _ = tx.send(feed);
        })
        .unwrap();
        let first = feeds.recv_timeout(Duration::from_secs(2)).unwrap();
        let second = feeds.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(first.is_closed());
        assert!(!second.is_closed());
    }

    #[test]
    fn select_switches_device() {
        let (bus, capture, feeds) = setup("Synth A");
        let format = InputFormat {
            sample_rate: 96_000,
            channels: 4,
            sample: SampleType::I32,
        };
        bus.add(SyntheticDevice::new("Synth B", format, Signal::Silence));
        let first = feeds.recv_timeout(Duration::from_secs(2)).unwrap();
        capture.select(DeviceSelector::Name("synth b".into()));
        let second = feeds.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(second.sample_rate(), 96_000);
        wait_for("fermeture de l'ancien", Duration::from_secs(2), || first.is_closed());
        wait_for(
            "état",
            Duration::from_secs(2),
            || matches!(capture.state(), CaptureState::Connected { ref device, .. } if device == "Synth B"),
        );
    }
}
