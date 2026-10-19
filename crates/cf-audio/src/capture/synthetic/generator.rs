//! Générateur des périphériques simulés : le signal, la conversion vers chaque format d'échantillon, et la boucle
//! qui joue le rôle du callback `cpal`.

use std::f64::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::SyntheticDevice;
use crate::feed::{FeedFault, FeedWriter, InputSample};

/// Retard au-delà duquel le générateur se recale au lieu de rattraper en rafale (machine de test surchargée).
const MAX_LAG: Duration = Duration::from_millis(100);

/// Signal produit par un périphérique simulé.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Signal {
    /// Silence numérique.
    Silence,
    /// Sinusoïde.
    Sine {
        /// Fréquence (Hz).
        freq_hz: f32,
        /// Amplitude crête, 0..1.
        amplitude: f32,
    },
    /// Coups de grosse caisse synthétiques (sinus grave amorti) à tempo fixe.
    Kicks {
        /// Tempo (battements par minute).
        bpm: f32,
        /// Amplitude crête, 0..1.
        amplitude: f32,
    },
}

/// Échantillon que le générateur sait produire.
pub(super) trait SyntheticSample: InputSample {
    fn from_f32(value: f32) -> Self;
}

impl SyntheticSample for f32 {
    fn from_f32(value: f32) -> Self {
        value
    }
}

impl SyntheticSample for i16 {
    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * 32_767.0).round() as Self
    }
}

impl SyntheticSample for u16 {
    fn from_f32(value: f32) -> Self {
        (value.clamp(-1.0, 1.0) * 32_767.0 + 32_768.0).round() as Self
    }
}

impl SyntheticSample for i32 {
    fn from_f32(value: f32) -> Self {
        (f64::from(value.clamp(-1.0, 1.0)) * 2_147_483_647.0).round() as Self
    }
}

/// Valeur du signal à l'échantillon `n`.
fn signal_at(signal: Signal, n: u64, sample_rate: u32) -> f32 {
    let t = n as f64 / f64::from(sample_rate);
    match signal {
        Signal::Silence => 0.0,
        Signal::Sine { freq_hz, amplitude } => {
            let phase = (t * f64::from(freq_hz)).fract();
            (f64::from(amplitude) * (TAU * phase).sin()) as f32
        }
        Signal::Kicks { bpm, amplitude } => {
            let period = 60.0 / f64::from(bpm.max(1.0));
            let since = t % period;
            // Sinus à 55 Hz amorti en ~50 ms : assez proche d'une grosse caisse pour exciter la bande des basses.
            (f64::from(amplitude) * (-since * 20.0).exp() * (TAU * 55.0 * since).sin()) as f32
        }
    }
}

/// Boucle du thread générateur : un bloc par période, à l'heure, par le même chemin que le callback `cpal`.
pub(super) fn generate<S: SyntheticSample>(
    device: &SyntheticDevice,
    mut writer: FeedWriter,
    plugged: &AtomicBool,
    stop: &AtomicBool,
) {
    let channels = usize::from(device.format.channels.max(1));
    let rate = device.format.sample_rate.max(1);
    let block = device.block_frames.max(1);
    let period = Duration::from_secs_f64(block as f64 / f64::from(rate));
    let faults = writer.fault_reporter();
    // Tampon alloué une fois, avant la boucle : la boucle elle-même n'alloue plus.
    let mut buffer = vec![S::from_f32(0.0); block * channels];
    let mut produced = 0_u64;
    let mut deadline = Instant::now();
    while !stop.load(Ordering::Acquire) {
        if !plugged.load(Ordering::Acquire) {
            // Ce que ferait le callback d'erreur de `cpal` à la disparition du périphérique.
            faults.report(FeedFault::DeviceLost);
            return;
        }
        let remaining = device
            .total_frames
            .map_or(block as u64, |total| total.saturating_sub(produced));
        let frames = (block as u64).min(remaining) as usize;
        if frames > 0 {
            for (i, frame) in buffer.chunks_exact_mut(channels).take(frames).enumerate() {
                frame.fill(S::from_f32(signal_at(device.signal, produced + i as u64, rate)));
            }
            writer.push_interleaved(&buffer[..frames * channels], channels);
            produced += frames as u64;
        }
        deadline += period;
        let now = Instant::now();
        if deadline > now {
            thread::sleep(deadline - now);
        } else if now - deadline > MAX_LAG {
            deadline = now;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_round_trip_through_input_conversion() {
        for value in [-1.0_f32, -0.5, 0.0, 0.25, 1.0] {
            assert!((i16::from_f32(value).to_f32() - value).abs() < 1e-3);
            assert!((u16::from_f32(value).to_f32() - value).abs() < 1e-3);
            assert!((i32::from_f32(value).to_f32() - value).abs() < 1e-6);
            assert!((f32::from_f32(value).to_f32() - value).abs() < 1e-9);
        }
    }

    #[test]
    fn signals_have_expected_shape() {
        let sine = Signal::Sine {
            freq_hz: 1000.0,
            amplitude: 0.5,
        };
        assert!(signal_at(sine, 0, 48_000).abs() < 1e-6);
        assert!((signal_at(sine, 12, 48_000) - 0.5).abs() < 1e-3);
        let kicks = Signal::Kicks {
            bpm: 120.0,
            amplitude: 0.8,
        };
        let peak = (0..2400).map(|n| signal_at(kicks, n, 48_000).abs()).fold(0.0, f32::max);
        let tail = (20_000..24_000)
            .map(|n| signal_at(kicks, n, 48_000).abs())
            .fold(0.0, f32::max);
        assert!(peak > 0.5 && tail < 0.01, "crête {peak}, fin {tail}");
        assert!(signal_at(Signal::Silence, 123, 48_000).abs() < f32::EPSILON);
    }
}
