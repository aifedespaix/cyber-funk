//! Boucle du thread d'analyse.
//!
//! Deux régimes dans un même tour :
//! - hors temps réel : traitement des ordres (construction et destruction des lecteurs de flux, qui allouent et
//!   libèrent), journaux de bilan ;
//! - sections temps réel, sous `assert_no_alloc` : vidage des sources inactives, chaque pas (lecture, analyse,
//!   publication, événements), passage au repos quand le signal disparaît. Aucune allocation, aucun journal ; le seul
//!   verrou possible est celui, interne et bref, de `crossbeam_channel::try_send` quand un lecteur d'événements est
//!   endormi sur le canal (le skill `realtime-audio` prescrit ce canal pour ce thread, qui n'est pas le callback).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Instant;

use assert_no_alloc::assert_no_alloc;
use cf_audio::feed::{ANALYSIS_SAMPLE_RATE, FeedError, FeedReader, HOP, HOP_SECONDS, HopStatus, InputFeed};
use cf_audio::source::SourceKind;
use crossbeam_channel::{Receiver, Sender};

use super::stats::{AnalysisStats, LATENCY_TARGET, LatencyWindow};
use super::{AnalysisConfig, Command};
use crate::analyzer::{Analyzer, StepContext};
use crate::events::{AudioEvent, AudioEventKind, EventSink};
use crate::features::AudioFeatures;

/// Pas analysés au plus par tour avant de revenir aux ordres : en rattrapage (après un accroc), l'opérateur garde la
/// main. 32 pas ≈ 340 ms d'audio, traités en bien moins de temps.
const MAX_STEPS_PER_TURN: u32 = 32;

pub(super) struct Runner<A: Analyzer> {
    analyzer: A,
    config: AnalysisConfig,
    commands: Receiver<Command>,
    publisher: triple_buffer::Input<AudioFeatures>,
    events: Sender<AudioEvent>,
    stats: Arc<AnalysisStats>,
    stop: Arc<AtomicBool>,
    slots: [Option<FeedReader>; SourceKind::COUNT],
    selected: Option<SourceKind>,
    hop: [f32; HOP],
    features: AudioFeatures,
    step: u64,
    live: bool,
    window: LatencyWindow,
    last_log: Instant,
    logged_input_dropped: u64,
    logged_dropped_events: u64,
}

impl<A: Analyzer> Runner<A> {
    pub(super) fn new(
        analyzer: A,
        config: AnalysisConfig,
        commands: Receiver<Command>,
        publisher: triple_buffer::Input<AudioFeatures>,
        events: Sender<AudioEvent>,
        stats: Arc<AnalysisStats>,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let mut analyzer = analyzer;
        analyzer.set_source(config.initial_source);
        Self {
            analyzer,
            selected: config.initial_source,
            config,
            commands,
            publisher,
            events,
            stats,
            stop,
            slots: [None, None],
            hop: [0.0; HOP],
            features: AudioFeatures::NEUTRAL,
            step: 0,
            live: false,
            window: LatencyWindow::default(),
            last_log: Instant::now(),
            logged_input_dropped: 0,
            logged_dropped_events: 0,
        }
    }

    pub(super) fn run(mut self) {
        tracing::debug!(source = ?self.selected, "analyse : thread démarré");
        while !self.stop.load(Ordering::Acquire) {
            self.handle_commands();
            let produced = self.pump();
            if let Some(reason) = assert_no_alloc(|| self.check_liveness(Instant::now())) {
                tracing::info!(source = ?self.selected, reason, "analyse : signal absent, live = false");
            }
            self.maybe_log();
            if produced == 0 {
                thread::sleep(self.config.poll_interval);
            }
        }
        tracing::debug!(steps = self.step, "analyse : thread arrêté");
    }

    fn handle_commands(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Attach(feed) => self.attach(feed),
                Command::Detach(kind) => {
                    tracing::info!(source = %kind, "analyse : flux retiré");
                    self.slots[kind.index()] = None;
                }
                Command::Select(kind) => self.select(kind),
                Command::ResetAnalysis => self.analyzer.reset(),
                Command::AutoGain(enabled) => self.analyzer.set_auto_gain(enabled),
            }
        }
    }

    fn attach(&mut self, feed: InputFeed) {
        let kind = feed.kind();
        let sample_rate = feed.sample_rate();
        match FeedReader::new(feed) {
            Ok(reader) => {
                tracing::info!(
                    source = %kind,
                    sample_rate,
                    resampler_delay = reader.output_delay(),
                    analysed = self.selected == Some(kind),
                    "analyse : flux rattaché"
                );
                // L'ancien lecteur de cet emplacement est libéré ici, hors section temps réel.
                self.slots[kind.index()] = Some(reader);
            }
            Err(error) => {
                self.stats.feed_errors.fetch_add(1, Ordering::Relaxed);
                tracing::error!(source = %kind, sample_rate, %error, "analyse : flux refusé");
            }
        }
    }

    fn select(&mut self, kind: Option<SourceKind>) {
        if kind == self.selected {
            return;
        }
        tracing::info!(from = ?self.selected, to = ?kind, "analyse : changement de source");
        self.selected = kind;
        if let Some(reader) = kind.and_then(|k| self.slots[k.index()].as_mut()) {
            reader.reset();
        }
        self.analyzer.set_source(kind);
        self.analyzer.reset();
        let mut sink = self.sink();
        sink.emit(AudioEventKind::SourceChanged { source: kind });
    }

    /// Vide les sources inactives et analyse la source choisie. Renvoie le nombre de pas publiés.
    fn pump(&mut self) -> u32 {
        let now = Instant::now();
        let selected = self.selected;
        assert_no_alloc(|| {
            for reader in self.slots.iter_mut().flatten() {
                // Une source en panne livre peut-être encore des restes : on les jette sans les analyser.
                if Some(reader.kind()) != selected || reader.fault().is_some() {
                    reader.drain(now);
                }
            }
        });
        let Some(kind) = selected else {
            return 0;
        };
        let mut produced = 0;
        while produced < MAX_STEPS_PER_TURN {
            match assert_no_alloc(|| self.step_once(kind)) {
                Ok(true) => produced += 1,
                Ok(false) => break,
                Err(error) => {
                    self.stats.feed_errors.fetch_add(1, Ordering::Relaxed);
                    tracing::error!(source = %kind, %error, "analyse : flux abandonné sur erreur de lecture");
                    self.slots[kind.index()] = None;
                    break;
                }
            }
        }
        produced
    }

    /// Un pas : lecture, analyse, publication. Section temps réel (sans allocation, sans journal).
    fn step_once(&mut self, kind: SourceKind) -> Result<bool, FeedError> {
        let Some(reader) = self.slots[kind.index()].as_mut() else {
            return Ok(false);
        };
        if reader.fault().is_some() {
            return Ok(false);
        }
        let HopStatus::Ready { delivered_at } = reader.next_hop(&mut self.hop, Instant::now())? else {
            return Ok(false);
        };
        self.step += 1;
        let time_s = self.step as f64 * HOP_SECONDS;
        let ctx = StepContext {
            step: self.step,
            time_s,
            dt_s: HOP_SECONDS as f32,
            sample_rate: ANALYSIS_SAMPLE_RATE,
        };
        let mut sink = EventSink::new(&self.events, &self.stats.dropped_events, self.step, time_s);
        if !self.live {
            self.live = true;
            sink.emit(AudioEventKind::LiveChanged { live: true });
        }
        self.features.step = self.step;
        self.features.time_s = time_s;
        self.features.live = true;
        self.analyzer.step(&self.hop, &ctx, &mut self.features, &mut sink);
        self.publisher.write(self.features);
        let published = Instant::now();
        self.stats.steps.store(self.step, Ordering::Relaxed);
        if let Some(delivered) = delivered_at {
            let latency = published.saturating_duration_since(delivered);
            self.stats.record_latency(latency);
            self.window.record(latency);
        }
        Ok(true)
    }

    /// Passe `live` à faux si la source choisie se tait, a disparu ou est en panne. Section temps réel ; renvoie la
    /// raison à journaliser (hors section) quand l'état vient de changer.
    fn check_liveness(&mut self, now: Instant) -> Option<&'static str> {
        if !self.live {
            return None;
        }
        let reason = match self.selected.and_then(|k| self.slots[k.index()].as_ref()) {
            None => Some("aucun flux pour la source choisie"),
            Some(reader) if reader.fault().is_some() => Some("panne signalée par le périphérique"),
            Some(reader) if reader.is_closed() => Some("flux fermé"),
            Some(reader) => match reader.last_data_at() {
                Some(at) if now.duration_since(at) < self.config.starve_timeout => None,
                _ => Some("plus aucun échantillon"),
            },
        }?;
        self.live = false;
        self.analyzer.idle(&mut self.features);
        self.features.live = false;
        self.publisher.write(self.features);
        let mut sink = self.sink();
        sink.emit(AudioEventKind::LiveChanged { live: false });
        Some(reason)
    }

    /// Émetteur daté de la fin du dernier pas (événements hors pas : source, `live`).
    fn sink(&self) -> EventSink<'_> {
        EventSink::new(
            &self.events,
            &self.stats.dropped_events,
            self.step,
            self.step as f64 * HOP_SECONDS,
        )
    }

    /// Bilan périodique au journal : latence callback → publication, pertes d'échantillons et d'événements.
    fn maybe_log(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_log) < self.config.log_interval {
            return;
        }
        self.last_log = now;
        if let Some((mean, max, steps)) = self.window.take() {
            let mean_ms = mean.as_secs_f64() * 1e3;
            let max_ms = max.as_secs_f64() * 1e3;
            tracing::debug!(
                mean_ms = format_args!("{mean_ms:.2}"),
                max_ms = format_args!("{max_ms:.2}"),
                steps,
                target_ms = LATENCY_TARGET.as_millis(),
                "analyse : latence callback → publication"
            );
            if mean > LATENCY_TARGET {
                tracing::warn!(mean_ms, "analyse : latence moyenne au-dessus de la cible");
            }
        }
        if let Some(reader) = self.selected.and_then(|k| self.slots[k.index()].as_ref()) {
            let dropped = reader.stats().dropped();
            self.stats.input_dropped.store(dropped, Ordering::Relaxed);
            if dropped > self.logged_input_dropped {
                tracing::warn!(
                    lost = dropped - self.logged_input_dropped,
                    total = dropped,
                    "analyse : échantillons jetés, ring plein (l'analyse a pris du retard)"
                );
            }
            self.logged_input_dropped = dropped;
        }
        let dropped_events = self.stats.dropped_events.load(Ordering::Relaxed);
        if dropped_events > self.logged_dropped_events {
            tracing::warn!(
                total = dropped_events,
                "analyse : événements jetés, canal plein (lecteur à l'arrêt ?)"
            );
            self.logged_dropped_events = dropped_events;
        }
    }
}
