//! Émulation de `AnalyserNode.getByteFrequencyData` (Web Audio), réglée comme Sillage (`engine/audio.ts`).
//!
//! Les seuils de `beat.ts` sont écrits pour des octets 0..255 sortis de ce nœud : pour qu'ils restent valides, on
//! refait la même chaîne, avec les mêmes arrondis que Chromium (`realtime_analyser.cc`) et que le script de référence
//! `tools/sillage-golden/analyser.ts` :
//!
//! 1. les [`FFT_SIZE`] derniers échantillons mono ;
//! 2. fenêtre de Blackman (alpha 0,16), coefficients calculés en double puis appliqués en `f32` ;
//! 3. FFT réelle (`realfft`, en `f32` comme Chromium), module de chaque bin multiplié par `1 / FFT_SIZE` ;
//! 4. lissage temporel `X̂[k] = τ·X̂_préc[k] + (1 − τ)·|X[k]|`, mémoire en `f32` ;
//! 5. dB = 20·log10(X̂) arrondi en `f32`, puis octet = troncature de `255·(dB − min)/(max − min)` bornée à 0..255.
//!
//! Comme dans le navigateur, le lissage avance à chaque lecture : un appel à
//! [`AnalyserEmulation::byte_frequency_data`] est un « pas » de lissage.

use std::f64::consts::PI;
use std::fmt;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

/// `fftSize` de Sillage (`analyser.fftSize = 2048`).
pub const FFT_SIZE: usize = 2048;
/// `frequencyBinCount` : la moitié de `fftSize`.
pub const BIN_COUNT: usize = FFT_SIZE / 2;
/// `ANALYSER_SMOOTHING` de `engine/audio.ts`, valable pour une lecture par image à 60 i/s.
pub const SMOOTHING_TIME_CONSTANT: f64 = 0.35;
/// `minDecibels` par défaut du nœud (dB) : octet 0.
pub const MIN_DECIBELS: f64 = -100.0;
/// `maxDecibels` par défaut du nœud (dB) : octet 255.
pub const MAX_DECIBELS: f64 = -30.0;
/// Durée (s) entre deux lectures pour laquelle [`SMOOTHING_TIME_CONSTANT`] est réglé : une image à 60 i/s.
pub const REFERENCE_STEP_S: f64 = 1.0 / 60.0;
/// Alpha de la fenêtre de Blackman de la spécification Web Audio.
const BLACKMAN_ALPHA: f64 = 0.16;
/// Échelle du module : `|X[k]| / fftSize`, avec X la DFT non normalisée (spécification et Chromium).
const MAGNITUDE_SCALE: f64 = 1.0 / FFT_SIZE as f64;
const MASK: usize = FFT_SIZE - 1;

/// Constante de lissage équivalente quand on lit l'analyseur toutes les `step_s` secondes au lieu de 1/60 s.
///
/// Dans le navigateur le lissage est fixé *par lecture* (une par image) ; le pas du thread d'analyse (512 échantillons,
/// 10,7 ms) est plus court que l'image de référence, ce qui lisserait moins en temps réel. On garde la même mémoire
/// en secondes : `τ^(step / (1/60))`. Pour `step_s = REFERENCE_STEP_S`, on retrouve exactement `τ`.
#[must_use]
pub fn smoothing_for_step(step_s: f64) -> f64 {
    SMOOTHING_TIME_CONSTANT.powf(step_s / REFERENCE_STEP_S)
}

/// Un `AnalyserNode` Web Audio réglé comme celui de Sillage. Tous les tampons sont alloués à la construction.
pub struct AnalyserEmulation {
    smoothing: f64,
    plan: Arc<dyn RealToComplex<f32>>,
    window: Box<[f32]>,
    /// Les [`FFT_SIZE`] derniers échantillons, en anneau (`write` : prochain emplacement, donc le plus ancien).
    ring: Box<[f32]>,
    write: usize,
    input: Box<[f32]>,
    output: Box<[Complex<f32>]>,
    scratch: Box<[Complex<f32>]>,
    /// Modules lissés, en `f32` comme le tampon de Chromium.
    smoothed: Box<[f32]>,
    bytes: [u8; BIN_COUNT],
}

impl fmt::Debug for AnalyserEmulation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnalyserEmulation")
            .field("smoothing", &self.smoothing)
            .field("write", &self.write)
            .finish_non_exhaustive()
    }
}

impl AnalyserEmulation {
    /// Analyseur neuf (tampon de zéros, lissage à zéro). `smoothing` : `smoothingTimeConstant`, 0..1 ; 0,35 pour
    /// Sillage à 60 lectures par seconde ([`smoothing_for_step`] pour un autre rythme).
    #[must_use]
    pub fn new(smoothing: f64) -> Self {
        let plan = RealFftPlanner::<f32>::new().plan_fft_forward(FFT_SIZE);
        let a0 = 0.5 * (1.0 - BLACKMAN_ALPHA);
        let a2 = 0.5 * BLACKMAN_ALPHA;
        let window = (0..FFT_SIZE)
            .map(|i| {
                let x = i as f64 / FFT_SIZE as f64;
                (a0 - 0.5 * (2.0 * PI * x).cos() + a2 * (4.0 * PI * x).cos()) as f32
            })
            .collect();
        Self {
            smoothing,
            input: plan.make_input_vec().into_boxed_slice(),
            output: plan.make_output_vec().into_boxed_slice(),
            scratch: plan.make_scratch_vec().into_boxed_slice(),
            plan,
            window,
            ring: vec![0.0; FFT_SIZE].into_boxed_slice(),
            write: 0,
            smoothed: vec![0.0; BIN_COUNT].into_boxed_slice(),
            bytes: [0; BIN_COUNT],
        }
    }

    /// Fait glisser la fenêtre : ajoute `samples` (mono) à la fin du tampon. Sans allocation.
    pub fn push(&mut self, samples: &[f32]) {
        // Plus de FFT_SIZE échantillons d'un coup : seuls les derniers comptent.
        let tail = &samples[samples.len().saturating_sub(FFT_SIZE)..];
        for &sample in tail {
            self.ring[self.write] = sample;
            self.write = (self.write + 1) & MASK;
        }
    }

    /// `getByteFrequencyData` : spectre en octets (bin `k` = `k · sampleRate / FFT_SIZE` Hz) de la fenêtre courante,
    /// après avoir avancé le lissage d'un pas. Sans allocation.
    pub fn byte_frequency_data(&mut self) -> &[u8; BIN_COUNT] {
        for (i, (slot, w)) in self.input.iter_mut().zip(self.window.iter()).enumerate() {
            // Le plus ancien échantillon est à `write` ; produit en `f32`, comme `p[i] *= window[i]` de Chromium.
            *slot = self.ring[(self.write + i) & MASK] * w;
        }
        // Les tailles sont celles du plan : l'appel ne peut échouer. En cas de bogue, on garde le spectre précédent
        // plutôt que de paniquer (pas d'`expect` hors tests).
        if self
            .plan
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .is_err()
        {
            debug_assert!(false, "tailles de FFT incohérentes");
            return &self.bytes;
        }

        let k = self.smoothing;
        let range_scale = 255.0 / (MAX_DECIBELS - MIN_DECIBELS);
        for ((bin, smoothed), byte) in self
            .output
            .iter()
            .zip(self.smoothed.iter_mut())
            .zip(self.bytes.iter_mut())
        {
            let (re, im) = (f64::from(bin.re), f64::from(bin.im));
            let magnitude = re.hypot(im) * MAGNITUDE_SCALE;
            let value = k * f64::from(*smoothed) + (1.0 - k) * magnitude;
            *smoothed = if value.is_finite() { value as f32 } else { 0.0 };
            let linear = f64::from(*smoothed);
            // `LinearToDecibels` de Chromium travaille en `f32` : `20 * log10f(linear)`.
            let db = (20.0 * f64::from(linear.log10() as f32)) as f32;
            let scaled = if linear > 0.0 {
                range_scale * (f64::from(db) - MIN_DECIBELS)
            } else {
                0.0
            };
            // Troncature (`static_cast<unsigned char>`) ; `as` sature déjà à 0 et 255.
            *byte = scaled as u8;
        }
        &self.bytes
    }

    /// Dernier spectre calculé (zéros avant la première lecture).
    #[must_use]
    pub fn bytes(&self) -> &[u8; BIN_COUNT] {
        &self.bytes
    }

    /// Repart d'un nœud neuf : tampon de zéros, lissage à zéro (nouvelle source ou nouvelle piste).
    pub fn reset(&mut self) {
        self.ring.fill(0.0);
        self.write = 0;
        self.smoothed.fill(0.0);
        self.bytes = [0; BIN_COUNT];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_RATE: f64 = 48_000.0;

    fn sine(freq_hz: f64, amplitude: f32, len: usize) -> Vec<f32> {
        (0..len)
            .map(|n| amplitude * (2.0 * PI * freq_hz * n as f64 / SAMPLE_RATE).sin() as f32)
            .collect()
    }

    /// Même contrôle que le script de référence : un sinus d'amplitude 0,01 centré sur un bin lit 169 (-53,6 dB).
    #[test]
    fn centred_sine_reads_like_sillage_reference() {
        let mut analyser = AnalyserEmulation::new(0.0);
        let bin = 40;
        let freq = bin as f64 * SAMPLE_RATE / FFT_SIZE as f64;
        analyser.push(&sine(freq, 0.01, FFT_SIZE));
        let bytes = analyser.byte_frequency_data();
        assert_eq!(bytes[bin], 169);
        assert!(bytes[bin] > bytes[bin + 6] && bytes[bin] > bytes[bin - 6]);
    }

    #[test]
    fn silence_reads_zero() {
        let mut analyser = AnalyserEmulation::new(SMOOTHING_TIME_CONSTANT);
        analyser.push(&[0.0; 800]);
        assert!(analyser.byte_frequency_data().iter().all(|&b| b == 0));
    }

    #[test]
    fn smoothing_converges_geometrically() {
        let mut analyser = AnalyserEmulation::new(0.5);
        let bin = 60;
        analyser.push(&sine(bin as f64 * SAMPLE_RATE / FFT_SIZE as f64, 0.1, FFT_SIZE));
        let first = f64::from(analyser.byte_frequency_data()[bin]);
        let mut last = first;
        for _ in 0..8 {
            last = f64::from(analyser.byte_frequency_data()[bin]);
        }
        // Premier pas : moitié du module final, soit 6 dB de moins, donc 6 · 255 / 70 ≈ 22 octets.
        assert!((last - first - 22.0).abs() <= 2.0, "first {first} last {last}");
    }

    #[test]
    fn window_slides_by_pushed_samples() {
        let mut analyser = AnalyserEmulation::new(0.0);
        // Sinus à un bin donné, poussé par morceaux de tailles variées : même résultat que d'un seul bloc.
        let signal = sine(60.0 * SAMPLE_RATE / FFT_SIZE as f64, 0.2, 3 * FFT_SIZE + 17);
        let mut whole = AnalyserEmulation::new(0.0);
        whole.push(&signal);
        let expected = *whole.byte_frequency_data();
        for chunk in signal.chunks(512) {
            analyser.push(chunk);
        }
        assert_eq!(*analyser.byte_frequency_data(), expected);
    }

    #[test]
    fn reset_forgets_everything() {
        let mut analyser = AnalyserEmulation::new(0.35);
        analyser.push(&sine(1000.0, 0.5, FFT_SIZE));
        assert!(analyser.byte_frequency_data().iter().any(|&b| b > 0));
        analyser.reset();
        analyser.push(&[0.0; 512]);
        assert!(analyser.byte_frequency_data().iter().all(|&b| b == 0));
    }

    #[test]
    fn smoothing_for_step_keeps_reference_and_scales() {
        assert!((smoothing_for_step(REFERENCE_STEP_S) - SMOOTHING_TIME_CONSTANT).abs() < 1e-12);
        // Deux pas de la moitié de la durée de référence valent un pas de référence.
        let half = smoothing_for_step(REFERENCE_STEP_S / 2.0);
        assert!((half * half - SMOOTHING_TIME_CONSTANT).abs() < 1e-12);
    }
}
