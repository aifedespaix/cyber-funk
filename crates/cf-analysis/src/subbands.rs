//! Sous-bandes de P2 §2.1 : `SB` (sub-basse), `PB` (punch), `LM` (bas-médium), `HM` (haut-médium), `TR`
//! (transitoires), plus le flux par bande, le HFC et le spectre en 32 bandes logarithmiques.
//!
//! - `SB` et `PB` : deux biquads passe-bande en cascade dans le domaine temporel, car la FFT de 2048 points à 48 kHz
//!   (23,4 Hz par bin) ne sépare pas 35 Hz de 95 Hz ; énergie RMS sur 1024 échantillons ;
//! - `LM`, `HM`, `TR` : lus dans le spectre en octets de l'`AnalyserNode` émulé, déjà calculé pour le détecteur de
//!   kicks (aucune FFT de plus : C1 prolonge `BeatAnalyzer`, il ne double rien).
//!
//! Les niveaux publiés sont des amplitudes RMS linéaires, non normalisées : la normalisation (AGC, courbe, seuil)
//! est l'affaire de la chaîne de filtrage de `cf-render/audio`, signal par signal. Pas d'allocation ni de verrou :
//! tout l'état est de taille fixe.

use cf_audio::feed::HOP;

use crate::analyser_emulation::{BIN_COUNT, FFT_SIZE, MAX_DECIBELS, MIN_DECIBELS};
use crate::filters::{AdaptiveZ, OneEuro};

/// Nombre de bandes logarithmiques du spectre lissé.
pub const SPECTRUM_BANDS: usize = 32;
/// Bornes (Hz) du spectre en 32 bandes.
pub const SPECTRUM_LOW_HZ: f64 = 100.0;
/// Haut du spectre en 32 bandes (Hz).
pub const SPECTRUM_HIGH_HZ: f64 = 16_000.0;

/// Bornes (Hz) des bandes lues dans la FFT.
const LM_HZ: (f64, f64) = (150.0, 500.0);
const HM_HZ: (f64, f64) = (500.0, 4_000.0);
const TR_HZ: (f64, f64) = (6_000.0, 16_000.0);
/// Étalonnage : amplitude RMS d'un sinus plein-échelle vu par la FFT émulée (fenêtre de Blackman, module `/N`).
/// Mesuré par `fft_bands_read_rms_amplitude` ; garde les niveaux FFT comparables à ceux des biquads.
const FFT_TO_RMS: f32 = 6.2;
/// Constantes de temps (s).
const FLUX_STATS_S: f32 = 4.0;
const TRANSIENT_DECAY_S: f32 = 0.09;
/// Période réfractaire (s) d'un transitoire HFC (P2 §2.1).
const TRANSIENT_REFRACTORY_S: f32 = 0.06;
/// Seuil du z-score d'un transitoire (P2 §2.1).
const TRANSIENT_Z: f32 = 2.0;
/// Bornes des scores publiés : au-delà, l'information est la même et les consommateurs n'ont pas à se protéger.
const Z_MIN: f32 = -8.0;
const Z_MAX: f32 = 16.0;

/// Les cinq niveaux de sous-bande : amplitude RMS linéaire (0 en silence, ≈ 0,7 pour un sinus plein-échelle).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SubBands {
    /// Sub-basse 20–60 Hz.
    pub sb: f32,
    /// Punch 60–150 Hz.
    pub pb: f32,
    /// Bas-médium 150–500 Hz.
    pub lm: f32,
    /// Haut-médium 500–4000 Hz.
    pub hm: f32,
    /// Transitoires 6–16 kHz.
    pub tr: f32,
}

/// Ce que l'analyse des sous-bandes publie à chaque pas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SubBandState {
    /// Niveaux des cinq bandes.
    pub bands: SubBands,
    /// Score z du flux positif du punch `F_PB` (montée de l'enveloppe de `PB`).
    pub z_pb: f32,
    /// Score z du flux positif spectral des transitoires `F_TR`.
    pub z_tr: f32,
    /// Score z du contenu haute fréquence `HFC = Σ k·|X_k|²`.
    pub hfc_z: f32,
    /// Impulsion après un transitoire HFC (`z > 2`, réfractaire 60 ms), 1 → 0 en 90 ms.
    pub transient: f32,
    /// Spectre en 32 bandes logarithmiques de 100 Hz à 16 kHz, 0..1, filtré One-Euro.
    pub spectrum: [f32; SPECTRUM_BANDS],
}

impl SubBandState {
    /// État de repos.
    pub const REST: Self = Self {
        bands: SubBands { sb: 0.0, pb: 0.0, lm: 0.0, hm: 0.0, tr: 0.0 },
        z_pb: 0.0,
        z_tr: 0.0,
        hfc_z: 0.0,
        transient: 0.0,
        spectrum: [0.0; SPECTRUM_BANDS],
    };
}

impl Default for SubBandState {
    fn default() -> Self {
        Self::REST
    }
}

/// Biquad passe-bande à gain crête 0 dB (P2 §2.1), en double précision : à 35 Hz sur 48 kHz, les pôles sont si près
/// du cercle unité que la simple précision dérive.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    fn band_pass(f0: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = std::f64::consts::TAU * f0 / sample_rate;
        let a = w0.sin() / (2.0 * q);
        let norm = 1.0 + a;
        Self { b0: a / norm, a1: -2.0 * w0.cos() / norm, a2: (1.0 - a) / norm, z1: 0.0, z2: 0.0 }
    }

    /// Forme transposée directe II ; `b = (b0, 0, −b0)`.
    #[inline]
    fn process(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.z2 - self.a1 * y;
        self.z2 = -self.b0 * x - self.a2 * y;
        y
    }

    fn clear(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Deux biquads en cascade et l'énergie des deux derniers pas (1024 échantillons).
#[derive(Debug, Clone, Copy)]
struct TimeBand {
    stages: [Biquad; 2],
    energy: [f64; 2],
}

impl TimeBand {
    fn new(f0: f64, q: f64, sample_rate: f64) -> Self {
        let stage = Biquad::band_pass(f0, q, sample_rate);
        Self { stages: [stage, stage], energy: [0.0; 2] }
    }

    /// Filtre le pas et renvoie l'amplitude RMS sur les deux derniers pas.
    fn process(&mut self, hop: &[f32; HOP]) -> f32 {
        let mut sum = 0.0;
        for &x in hop {
            let mid = self.stages[0].process(f64::from(x));
            let y = self.stages[1].process(mid);
            sum += y * y;
        }
        self.energy = [self.energy[1], sum];
        ((self.energy[0] + self.energy[1]) / (2 * HOP) as f64).sqrt() as f32
    }

    fn clear(&mut self) {
        self.stages.iter_mut().for_each(Biquad::clear);
        self.energy = [0.0; 2];
    }
}

/// Bins `[from, to)` d'une plage en Hz (au moins un bin).
fn bin_range(hz: (f64, f64), sample_rate: u32) -> (usize, usize) {
    let per_hz = FFT_SIZE as f64 / f64::from(sample_rate.max(1));
    let from = ((hz.0 * per_hz) as usize).min(BIN_COUNT - 1);
    let to = ((hz.1 * per_hz) as usize).clamp(from + 1, BIN_COUNT);
    (from, to)
}

/// Octet de l'`AnalyserNode` → amplitude `|X|/N` linéaire.
fn byte_to_magnitude(byte: u8) -> f32 {
    if byte == 0 {
        return 0.0;
    }
    let db = MIN_DECIBELS + f64::from(byte) / 255.0 * (MAX_DECIBELS - MIN_DECIBELS);
    10.0_f64.powf(db / 20.0) as f32
}

/// Amplitude RMS d'une plage de bins : racine de la somme des carrés, étalonnée par [`FFT_TO_RMS`].
fn band_rms(magnitudes: &[f32; BIN_COUNT], (from, to): (usize, usize)) -> f32 {
    let power: f32 = magnitudes[from..to].iter().map(|m| m * m).sum();
    power.sqrt() * FFT_TO_RMS
}

/// L'analyse des sous-bandes. Tous les tampons sont alloués à la construction.
#[derive(Debug, Clone)]
pub struct SubBandAnalyzer {
    sb: TimeBand,
    pb: TimeBand,
    pb_previous: f32,
    magnitudes: [f32; BIN_COUNT],
    tr_previous: f32,
    pb_stats: AdaptiveZ,
    tr_stats: AdaptiveZ,
    hfc_stats: AdaptiveZ,
    refractory_s: f32,
    spectrum_filters: [OneEuro; SPECTRUM_BANDS],
    state: SubBandState,
    sample_rate: u32,
}

impl SubBandAnalyzer {
    /// Analyse neuve pour des pas de [`HOP`] échantillons à `sample_rate` Hz.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let sr = f64::from(sample_rate.max(1));
        Self {
            sb: TimeBand::new(35.0, 0.9, sr),
            pb: TimeBand::new(95.0, 1.1, sr),
            pb_previous: 0.0,
            magnitudes: [0.0; BIN_COUNT],
            tr_previous: 0.0,
            pb_stats: AdaptiveZ::new(FLUX_STATS_S, 0.004),
            tr_stats: AdaptiveZ::new(FLUX_STATS_S, 0.002),
            hfc_stats: AdaptiveZ::new(FLUX_STATS_S, 1e-9),
            refractory_s: 0.0,
            spectrum_filters: [OneEuro::new(1.0, 0.5); SPECTRUM_BANDS],
            state: SubBandState::REST,
            sample_rate,
        }
    }

    /// Dernier état publié.
    #[must_use]
    pub fn state(&self) -> &SubBandState {
        &self.state
    }

    /// Oublie tout (nouvelle source ou nouvelle piste).
    pub fn reset(&mut self) {
        *self = Self::new(self.sample_rate);
    }

    /// Signal interrompu : tout retombe à zéro, les statistiques sont gardées.
    pub fn rest(&mut self) {
        self.sb.clear();
        self.pb.clear();
        self.pb_previous = 0.0;
        self.tr_previous = 0.0;
        self.spectrum_filters = [OneEuro::new(1.0, 0.5); SPECTRUM_BANDS];
        self.state = SubBandState::REST;
    }

    /// Avance d'un pas : `hop` (domaine temporel), `spectrum` (octets de l'analyseur émulé), `dt_s` secondes.
    pub fn update(&mut self, hop: &[f32; HOP], spectrum: &[u8; BIN_COUNT], dt_s: f32) -> &SubBandState {
        let dt = dt_s.clamp(0.0, 0.25);
        for (m, &b) in self.magnitudes.iter_mut().zip(spectrum.iter()) {
            *m = byte_to_magnitude(b);
        }
        let sr = self.sample_rate;
        let s = &mut self.state;
        s.bands.sb = self.sb.process(hop);
        s.bands.pb = self.pb.process(hop);
        s.bands.lm = band_rms(&self.magnitudes, bin_range(LM_HZ, sr));
        s.bands.hm = band_rms(&self.magnitudes, bin_range(HM_HZ, sr));
        s.bands.tr = band_rms(&self.magnitudes, bin_range(TR_HZ, sr));

        // Flux positif : montée de l'enveloppe de `PB` (temps), montée de l'énergie de `TR` (spectre).
        let f_pb = (s.bands.pb - self.pb_previous).max(0.0);
        let f_tr = (s.bands.tr - self.tr_previous).max(0.0);
        self.pb_previous = s.bands.pb;
        self.tr_previous = s.bands.tr;
        s.z_pb = self.pb_stats.update(f_pb, dt).clamp(Z_MIN, Z_MAX);
        s.z_tr = self.tr_stats.update(f_tr, dt).clamp(Z_MIN, Z_MAX);

        // HFC = Σ k·|X_k|².
        let hfc: f32 = self.magnitudes.iter().enumerate().map(|(k, m)| k as f32 * m * m).sum();
        s.hfc_z = self.hfc_stats.update(hfc, dt).clamp(Z_MIN, Z_MAX);
        self.refractory_s = (self.refractory_s - dt).max(0.0);
        s.transient *= (-dt / TRANSIENT_DECAY_S).exp();
        if self.refractory_s <= 0.0 && s.hfc_z > TRANSIENT_Z {
            s.transient = 1.0;
            self.refractory_s = TRANSIENT_REFRACTORY_S;
        }

        // Spectre en 32 bandes logarithmiques, niveau d'octet normalisé puis One-Euro.
        let ratio = (SPECTRUM_HIGH_HZ / SPECTRUM_LOW_HZ).powf(1.0 / SPECTRUM_BANDS as f64);
        let mut lo_hz = SPECTRUM_LOW_HZ;
        for (band, filter) in s.spectrum.iter_mut().zip(self.spectrum_filters.iter_mut()) {
            let (from, to) = bin_range((lo_hz, lo_hz * ratio), sr);
            let sum: f32 = spectrum[from..to].iter().map(|&b| f32::from(b)).sum();
            *band = filter.step(sum / ((to - from) as f32 * 255.0), dt).clamp(0.0, 1.0);
            lo_hz *= ratio;
        }
        &self.state
    }
}
