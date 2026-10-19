//! Outils partagés des tests d'intégration du thread d'analyse : analyseur sonde, attente bornée, montage d'une
//! capture synthétique.

#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cf_analysis::{
    AnalysisConfig, AnalysisOutputs, AnalysisThread, Analyzer, AudioEvent, AudioEventKind, AudioFeatures, EventSink,
    StepContext,
};
use cf_audio::capture::{
    Capture, CaptureConfig, DeviceSelector, InputFormat, SampleType, Signal, SyntheticBus, SyntheticDevice,
};
use cf_audio::feed::HOP;

/// Ce que la sonde a vu, lisible depuis le test.
#[derive(Debug, Default)]
pub struct ProbeState {
    pub steps: AtomicU64,
    pub resets: AtomicU64,
    pub idles: AtomicU64,
    /// Passages à zéro montants cumulés (continuité assurée d'un pas à l'autre).
    pub crossings: AtomicU64,
    pub samples: AtomicU64,
    /// Valeur efficace du dernier pas × 10⁶.
    pub rms_micro: AtomicU32,
}

impl ProbeState {
    /// Fréquence dominante (Hz) entre deux relevés `(passages, échantillons)`.
    pub fn frequency_since(&self, before: (u64, u64)) -> f64 {
        let (crossings, samples) = self.mark();
        (crossings - before.0) as f64 * 48_000.0 / (samples - before.1).max(1) as f64
    }

    pub fn mark(&self) -> (u64, u64) {
        (
            self.crossings.load(Ordering::Relaxed),
            self.samples.load(Ordering::Relaxed),
        )
    }

    pub fn rms(&self) -> f32 {
        self.rms_micro.load(Ordering::Relaxed) as f32 / 1e6
    }
}

/// Analyseur de test : compte, mesure la fréquence et la valeur efficace, et publie cette dernière dans `loud` pour
/// vérifier que ce que calcule l'analyseur arrive bien jusqu'au lecteur. Sans allocation (il tourne sous
/// `assert_no_alloc`).
#[derive(Debug)]
pub struct Probe {
    pub state: Arc<ProbeState>,
    last: f32,
}

impl Probe {
    pub fn new() -> (Self, Arc<ProbeState>) {
        let state = Arc::new(ProbeState::default());
        (
            Self {
                state: Arc::clone(&state),
                last: 0.0,
            },
            state,
        )
    }
}

impl Analyzer for Probe {
    fn step(&mut self, hop: &[f32; HOP], ctx: &StepContext, out: &mut AudioFeatures, _: &mut EventSink<'_>) {
        assert_eq!(ctx.sample_rate, 48_000);
        let mut crossings = 0;
        let mut energy = 0.0_f64;
        for &sample in hop {
            if self.last < 0.0 && sample >= 0.0 {
                crossings += 1;
            }
            self.last = sample;
            energy += f64::from(sample) * f64::from(sample);
        }
        let rms = (energy / HOP as f64).sqrt() as f32;
        self.state.crossings.fetch_add(crossings, Ordering::Relaxed);
        self.state.samples.fetch_add(HOP as u64, Ordering::Relaxed);
        self.state.rms_micro.store((rms * 1e6) as u32, Ordering::Relaxed);
        self.state.steps.fetch_add(1, Ordering::Relaxed);
        out.loud = rms;
    }

    fn idle(&mut self, out: &mut AudioFeatures) {
        self.state.idles.fetch_add(1, Ordering::Relaxed);
        out.rest_energy();
    }

    fn reset(&mut self) {
        self.state.resets.fetch_add(1, Ordering::Relaxed);
        self.last = 0.0;
    }
}

/// Journaux `debug` des deux crates vers la sortie des tests (visibles avec `--nocapture`).
pub fn init_logs() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("cf_analysis=debug,cf_audio=debug")
        .with_test_writer()
        .try_init();
}

/// Attend une condition, en échouant au-delà de `timeout`. Renvoie le temps écoulé.
pub fn wait_for(what: &str, timeout: Duration, mut done: impl FnMut() -> bool) -> Duration {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < timeout, "délai dépassé ({timeout:?}) : {what}");
        thread::sleep(Duration::from_millis(1));
    }
    start.elapsed()
}

pub fn format(sample_rate: u32, channels: u16, sample: SampleType) -> InputFormat {
    InputFormat {
        sample_rate,
        channels,
        sample,
    }
}

pub fn sine(freq_hz: f32, amplitude: f32) -> Signal {
    Signal::Sine { freq_hz, amplitude }
}

/// Réglages d'analyse des tests : bilans de journal fréquents pour voir la latence.
pub fn analysis_config() -> AnalysisConfig {
    AnalysisConfig {
        log_interval: Duration::from_millis(500),
        ..AnalysisConfig::default()
    }
}

/// Un banc synthétique avec un périphérique, une analyse sonde, et une capture qui l'ouvre par son nom.
pub struct Rig {
    pub bus: SyntheticBus,
    pub analysis: AnalysisThread,
    pub outputs: AnalysisOutputs,
    pub probe: Arc<ProbeState>,
    pub capture: Capture,
}

pub fn rig(device: SyntheticDevice, capture: CaptureConfig) -> Result<Rig, Box<dyn std::error::Error>> {
    init_logs();
    let bus = SyntheticBus::new();
    let name = device.name.clone();
    bus.add(device);
    let (probe, state) = Probe::new();
    let (analysis, outputs) = AnalysisThread::spawn(probe, analysis_config())?;
    let config = CaptureConfig {
        device: DeviceSelector::Name(name),
        retry_interval: Duration::from_millis(50),
        ..capture
    };
    let capture = Capture::start(bus.backend(), config, analysis.control())?;
    Ok(Rig {
        bus,
        analysis,
        outputs,
        probe: state,
        capture,
    })
}

impl Rig {
    pub fn features(&mut self) -> AudioFeatures {
        *self.outputs.features.latest()
    }

    /// Attend un événement de cette nature (les autres sont ignorés) ; `None` au-delà du délai.
    pub fn next_event(&self, kind: AudioEventKind, timeout: Duration) -> Option<AudioEvent> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.outputs.events.recv_timeout(left) {
                Ok(event) if event.kind == kind => return Some(event),
                Ok(_) => {}
                Err(_) => return None,
            }
        }
    }
}
