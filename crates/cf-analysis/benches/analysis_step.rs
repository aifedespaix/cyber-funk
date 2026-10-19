//! Coût d'un pas d'analyse (critère J1-04 : < 0,5 ms en release) : émulation de l'`AnalyserNode` (fenêtre, FFT de
//! 2048 points, lissage, dB), détecteur, et le tout par `BeatAnalyzer`, le chemin du thread d'analyse.
//!
//! `cargo bench -p cf-analysis`

use std::f64::consts::PI;
use std::hint::black_box;
use std::sync::atomic::AtomicU64;

use cf_analysis::analyser_emulation::FFT_SIZE;
use cf_analysis::{AnalyserEmulation, Analyzer, AudioFeatures, BeatAnalyzer, BeatDetector, EventSink, StepContext};
use cf_audio::feed::{HOP, HOP_SECONDS};
use criterion::{Criterion, criterion_group, criterion_main};

/// Nombre de pas du signal, rejoué en boucle : une dizaine de secondes de musique.
const STEPS: usize = 1024;

/// Coups de basse à 128 BPM sur du bruit : des spectres pleins, des kicks, le chemin complet.
fn music() -> Vec<[f32; HOP]> {
    let beat_samples = (60.0 / 128.0 * 48_000.0) as u64;
    (0..STEPS as u64)
        .map(|step| {
            let mut hop = [0.0_f32; HOP];
            for (i, slot) in hop.iter_mut().enumerate() {
                let n = step * HOP as u64 + i as u64;
                let t = (n % beat_samples) as f64 / 48_000.0;
                let kick = 0.8 * (-t / 0.05).exp() * (2.0 * PI * 55.0 * t).sin();
                let noise = (((n * 2_654_435_761) >> 8) % 2001) as f64 / 1000.0 - 1.0;
                *slot = (kick + 0.05 * noise) as f32;
            }
            hop
        })
        .collect()
}

fn benches(c: &mut Criterion) {
    let hops = music();
    let mut group = c.benchmark_group("analysis_step");

    group.bench_function("analyser_emulation", |b| {
        let mut analyser = AnalyserEmulation::new(0.51);
        let mut i = 0;
        b.iter(|| {
            analyser.push(black_box(&hops[i % STEPS]));
            i += 1;
            black_box(analyser.byte_frequency_data());
        });
    });

    group.bench_function("beat_detector", |b| {
        let mut analyser = AnalyserEmulation::new(0.51);
        let spectra: Vec<[u8; 1024]> = hops
            .iter()
            .map(|hop| {
                analyser.push(hop);
                *analyser.byte_frequency_data()
            })
            .collect();
        let mut detector = BeatDetector::new();
        let mut i = 0;
        b.iter(|| {
            i += 1;
            let now = i as f64 * HOP_SECONDS;
            black_box(detector.update(Some(black_box(&spectra[i % STEPS])), 48_000, FFT_SIZE, now, HOP_SECONDS));
        });
    });

    group.bench_function("beat_analyzer_step", |b| {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let dropped = AtomicU64::new(0);
        let mut analyzer = BeatAnalyzer::new();
        let mut features = AudioFeatures::NEUTRAL;
        let mut step = 0_u64;
        b.iter(|| {
            step += 1;
            let time_s = step as f64 * HOP_SECONDS;
            let ctx = StepContext {
                step,
                time_s,
                dt_s: HOP_SECONDS as f32,
                sample_rate: 48_000,
            };
            let mut sink = EventSink::new(&tx, &dropped, step, time_s);
            analyzer.step(black_box(&hops[step as usize % STEPS]), &ctx, &mut features, &mut sink);
            black_box(&features);
        });
        drop(rx);
    });
    group.finish();
}

criterion_group!(analysis_step, benches);
criterion_main!(analysis_step);
