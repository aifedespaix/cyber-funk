//! Démo de la capture supervisée et du thread d'analyse, sans Bevy.
//!
//! ```text
//! cargo run -p cf-analysis --example capture -- --list              # périphériques d'entrée
//! cargo run -p cf-analysis --example capture -- "Scarlett"          # capture par nom (ou fragment de nom)
//! cargo run -p cf-analysis --example capture -- --seconds 120       # périphérique par défaut, 2 minutes
//! cargo run -p cf-analysis --example capture -- --synthetic         # sans matériel : débranchement simulé
//! ```
//!
//! Test de débranchement sur matériel réel (critère J1-03) : lancer avec le nom de l'interface USB, débrancher son
//! câble pendant l'exécution, attendre quelques secondes, le rebrancher. Attendu dans le journal : `live = false`
//! aussitôt (« panne signalée » ou « plus aucun échantillon »), « flux perdu, reconnexion par nom », des tentatives,
//! puis « périphérique reconnecté » et le bilan repasse à `live=true`, sans plantage ni redémarrage de l'analyse
//! (le compteur de pas reprend là où il s'était arrêté).
//!
//! En debug, la démo installe `AllocDisabler` : une allocation dans une section temps réel (le vrai callback `cpal`
//! ou un pas d'analyse) la ferait avorter. Sans carte son, le périphérique ALSA `null` (« generate zero samples ») sert
//! de banc d'essai du vrai callback : il livre du silence sans cadence temps réel, donc des millions d'appels.

use std::error::Error;
use std::thread;
use std::time::{Duration, Instant};

use cf_analysis::{AnalysisConfig, AnalysisOutputs, AnalysisThread, NeutralAnalyzer};
use cf_audio::capture::{
    Capture, CaptureConfig, CpalBackend, DeviceSelector, InputBackend, InputFormat, SampleType, Signal, SyntheticBus,
    SyntheticDevice,
};
use tracing_subscriber::EnvFilter;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOCATOR: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

struct Args {
    list: bool,
    synthetic: bool,
    seconds: u64,
    device: DeviceSelector,
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = Args {
        list: false,
        synthetic: false,
        seconds: 60,
        device: DeviceSelector::Default,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--list" => args.list = true,
            "--synthetic" => args.synthetic = true,
            "--seconds" => args.seconds = it.next().ok_or("--seconds attend une durée")?.parse()?,
            name => args.device = DeviceSelector::Name(name.to_owned()),
        }
    }
    Ok(args)
}

fn main() -> Result<(), Box<dyn Error>> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,cf_analysis=debug,cf_audio=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
    let args = parse_args()?;
    if args.list {
        for device in CpalBackend::new().input_devices()? {
            let format = device
                .default_format
                .map_or_else(|| "format par défaut inutilisable".to_owned(), |f| f.to_string());
            tracing::info!(default = device.is_default, %format, "{}", device.name);
        }
        return Ok(());
    }
    let (analysis, mut outputs) = AnalysisThread::spawn(NeutralAnalyzer, AnalysisConfig::default())?;
    if args.synthetic {
        run_synthetic(&analysis, &mut outputs)
    } else {
        let config = CaptureConfig {
            device: args.device,
            ..CaptureConfig::default()
        };
        let capture = Capture::start(CpalBackend::new(), config, analysis.control())?;
        let end = Instant::now() + Duration::from_secs(args.seconds);
        while Instant::now() < end {
            report(&analysis, &mut outputs, &capture, Duration::from_secs(1));
        }
        Ok(())
    }
}

/// Scénario sans matériel : interface simulée à 44,1 kHz, débranchée puis rebranchée.
fn run_synthetic(analysis: &AnalysisThread, outputs: &mut AnalysisOutputs) -> Result<(), Box<dyn Error>> {
    let bus = SyntheticBus::new();
    let format = InputFormat {
        sample_rate: 44_100,
        channels: 2,
        sample: SampleType::I16,
    };
    let kicks = Signal::Kicks {
        bpm: 128.0,
        amplitude: 0.8,
    };
    bus.add(SyntheticDevice::new("Interface USB simulée", format, kicks));
    let config = CaptureConfig {
        device: DeviceSelector::Name("USB simulée".into()),
        ..CaptureConfig::default()
    };
    let capture = Capture::start(bus.backend(), config, analysis.control())?;
    for _ in 0..4 {
        report(analysis, outputs, &capture, Duration::from_millis(500));
    }
    tracing::info!("démo : débranchement de l'interface simulée");
    let _ = bus.unplug("Interface USB simulée");
    for _ in 0..4 {
        report(analysis, outputs, &capture, Duration::from_millis(500));
    }
    tracing::info!("démo : rebranchement");
    let _ = bus.replug("Interface USB simulée");
    for _ in 0..4 {
        report(analysis, outputs, &capture, Duration::from_millis(500));
    }
    Ok(())
}

/// Attend `period`, puis journalise les événements reçus et un bilan.
fn report(analysis: &AnalysisThread, outputs: &mut AnalysisOutputs, capture: &Capture, period: Duration) {
    thread::sleep(period);
    for event in outputs.events.try_iter() {
        tracing::info!(time_s = format_args!("{:.3}", event.time_s), step = event.step, kind = ?event.kind, "événement");
    }
    let features = *outputs.features.latest();
    let stats = analysis.stats();
    tracing::info!(
        live = features.live,
        step = features.step,
        time_s = format_args!("{:.2}", features.time_s),
        latency_mean = ?stats.latency_mean,
        latency_max = ?stats.latency_max,
        input_dropped = stats.input_dropped,
        capture = ?capture.state(),
        "bilan"
    );
}
