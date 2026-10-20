//! Gain automatique du micro par bande (portage de `BandGain` et de `readMicrophone` de `engine/audio.ts`).
//!
//! Un micro de portable capte la musique à -30 dB ou moins : les seuils de `beat.ts` sont écrits pour des octets
//! d'analyseur de fichier musical. Chaque zone du spectre (basses, médiums, aigus) suit donc son propre pic récent et
//! règle un gain pour que les passages forts atteignent le niveau visé. N'est appliqué qu'aux sources de capture.

use crate::analyser_emulation::{BIN_COUNT, FFT_SIZE};

/// Bandes qui règlent le gain des basses : les basses (kick) et les bas-médiums, en indices de bins.
const MIC_BASS: (usize, usize) = (1, 9);
const MIC_BODY: (usize, usize) = (1, 48);
/// Niveau visé (0–1) pour les basses d'un passage fort : celui d'un fichier musical ordinaire.
const MIC_TARGET: f64 = 0.62;
/// Plancher du pic suivi (sur 255) : en dessous, on ne grossit pas le bruit de fond d'une pièce vide.
const MIC_FLOOR: f64 = 14.0;
/// Gain maximal : un micro de portable très lointain reste exploitable sans devenir du souffle.
const MIC_MAX_GAIN: f64 = 28.0;
/// Retombée du pic suivi, par image à 60 i/s : une vingtaine de secondes pour oublier un passage fort.
const MIC_DECAY: f64 = 0.9993;
/// Médiums (300–2500 Hz) et aigus (5–12 kHz) ont leur propre gain : un souffle de pièce n'y devient pas un signal.
const MIC_MID_HZ: (f64, f64) = (300.0, 2500.0);
const MIC_HIGH_HZ: (f64, f64) = (5000.0, 12000.0);
/// Niveau visé par bande (sur 1) et gain maximal : réglés pour que `mid` et `high` (voir `beat.ts`) retrouvent leur
/// plage de fichier.
const MIC_MID_TARGET: f64 = 0.4;
const MIC_HIGH_TARGET: f64 = 0.15;
const MIC_BAND_MAX_GAIN: f64 = 12.0;

/// Gain automatique d'une bande : suit le pic récent, monte doucement et redescend vite.
#[derive(Debug, Clone, Copy)]
pub struct BandGain {
    target: f64,
    max_gain: f64,
    floor: f64,
    peak: f64,
    gain: f64,
}

impl BandGain {
    /// Gain d'une bande : niveau visé (0..1), gain maximal, plancher du pic (sur 255).
    #[must_use]
    pub fn new(target: f64, max_gain: f64, floor: f64) -> Self {
        Self {
            target,
            max_gain,
            floor,
            peak: floor,
            gain: 1.0,
        }
    }

    /// Gain courant.
    #[must_use]
    pub fn gain(&self) -> f64 {
        self.gain
    }

    /// `energy` : énergie de la bande (0..255). `frames` : durée écoulée en images à 60 i/s (1 = une image à 60 i/s) :
    /// l'oubli du pic ne dépend pas de la cadence de lecture. Renvoie le gain.
    pub fn update(&mut self, energy: f64, frames: f64) -> f64 {
        self.peak = self.floor.max(energy).max(self.peak * MIC_DECAY.powf(frames));
        let wanted = self.max_gain.min(self.target * 255.0 / self.peak);
        let rate: f64 = if wanted < self.gain { 0.2 } else { 0.02 };
        self.gain += (wanted - self.gain) * (1.0 - (1.0 - rate).powf(frames));
        self.gain
    }

    /// Repart d'un gain neutre (changement de source).
    pub fn reset(&mut self) {
        self.peak = self.floor;
        self.gain = 1.0;
    }
}

/// Les trois gains du micro et les bornes (en bins) de chaque zone, pour un analyseur de [`FFT_SIZE`] points.
#[derive(Debug, Clone, Copy)]
pub struct MicGain {
    low: BandGain,
    mid: BandGain,
    high: BandGain,
    mid_from: usize,
    mid_to: usize,
    high_from: usize,
    high_to: usize,
}

impl MicGain {
    /// Gains neufs pour des échantillons à `sample_rate` Hz.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        let hz = f64::from(sample_rate) / FFT_SIZE as f64;
        let bin = |f: f64| (f / hz).round() as usize;
        Self {
            low: BandGain::new(MIC_TARGET, MIC_MAX_GAIN, MIC_FLOOR),
            mid: BandGain::new(MIC_MID_TARGET, MIC_BAND_MAX_GAIN, MIC_FLOOR / 2.0),
            high: BandGain::new(MIC_HIGH_TARGET, MIC_BAND_MAX_GAIN, MIC_FLOOR / 4.0),
            mid_from: bin(MIC_MID_HZ.0),
            mid_to: bin(MIC_MID_HZ.1),
            high_from: bin(MIC_HIGH_HZ.0),
            high_to: bin(MIC_HIGH_HZ.1),
        }
    }

    /// Gains courants (basses/corps, médiums, aigus).
    #[must_use]
    pub fn gains(&self) -> [f64; 3] {
        [self.low.gain(), self.mid.gain(), self.high.gain()]
    }

    /// Remet à l'échelle d'un fichier musical le spectre `raw` du micro, dans `out`. `frames` : durée du pas en images
    /// à 60 i/s. Sans allocation.
    pub fn apply(&mut self, raw: &[u8; BIN_COUNT], out: &mut [u8; BIN_COUNT], frames: f64) {
        let bass = mean(raw, MIC_BASS.0, MIC_BASS.1);
        let body = mean(raw, MIC_BODY.0, MIC_BODY.1);
        let low = self.low.update(bass.max(body * 1.4), frames);
        let mid = self.mid.update(mean(raw, self.mid_from, self.mid_to), frames);
        let high = self.high.update(mean(raw, self.high_from, self.high_to), frames);
        // Les basses jusqu'aux médiums, puis les médiums jusqu'aux aigus : chaque octet prend le gain de sa zone.
        for (i, (slot, &byte)) in out.iter_mut().zip(raw.iter()).enumerate() {
            let gain = if i < self.mid_from {
                low
            } else if i < self.high_from {
                mid
            } else {
                high
            };
            // Troncature, comme l'écriture d'un flottant dans un `Uint8Array`.
            *slot = (f64::from(byte) * gain).min(255.0) as u8;
        }
    }

    /// Repart de gains neutres (changement de source).
    pub fn reset(&mut self) {
        self.low.reset();
        self.mid.reset();
        self.high.reset();
    }
}

/// Moyenne des octets `from..to` (borne haute exclue, bornée à la taille du spectre) ; 0 si la plage est vide.
fn mean(data: &[u8], from: usize, to: usize) -> f64 {
    let end = to.min(data.len());
    if end <= from {
        return 0.0;
    }
    data[from..end].iter().map(|&b| f64::from(b)).sum::<f64>() / (end - from) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_edges_match_sillage_at_48_khz() {
        let g = MicGain::new(48_000);
        assert_eq!((g.mid_from, g.mid_to, g.high_from, g.high_to), (13, 107, 213, 512));
    }

    #[test]
    fn quiet_room_is_not_amplified_beyond_the_floor() {
        let mut g = BandGain::new(MIC_TARGET, MIC_MAX_GAIN, MIC_FLOOR);
        for _ in 0..600 {
            g.update(3.0, 1.0);
        }
        // Pic plancher 14 : gain visé 0,62 × 255 / 14 ≈ 11,3, jamais le maximum de 28 sur du bruit de fond.
        assert!(g.gain() < 12.0, "{}", g.gain());
    }

    #[test]
    fn gain_rises_slowly_and_falls_fast() {
        let mut g = BandGain::new(MIC_TARGET, MIC_MAX_GAIN, MIC_FLOOR);
        for _ in 0..30 {
            g.update(14.0, 1.0);
        }
        let risen = g.gain();
        assert!(risen > 1.0 && risen < 11.3);
        let before = g.gain();
        g.update(250.0, 1.0);
        // Un pic fort : le gain visé tombe à 0,62 × 255 / 250 ; 20 % de l'écart est repris dès l'image suivante.
        let wanted = MIC_TARGET * 255.0 / 250.0;
        assert!((g.gain() - (before + 0.2 * (wanted - before))).abs() < 1e-9);
    }

    #[test]
    fn step_length_does_not_change_the_response() {
        // Même durée (3 s) à 60 i/s et au pas du thread d'analyse (0,64 image) : le gain doit coïncider.
        let mut a = BandGain::new(MIC_TARGET, MIC_MAX_GAIN, MIC_FLOOR);
        let mut b = a;
        for _ in 0..180 {
            a.update(40.0, 1.0);
        }
        let frames: f64 = 512.0 / 48_000.0 * 60.0;
        for _ in 0..(180.0 / frames).round() as usize {
            b.update(40.0, frames);
        }
        assert!((a.gain() - b.gain()).abs() < 0.05, "{} contre {}", a.gain(), b.gain());
    }

    #[test]
    fn apply_scales_each_zone_and_clamps() {
        let mut g = MicGain::new(48_000);
        let mut raw = [0_u8; BIN_COUNT];
        raw[1..48].fill(20);
        raw[100] = 200;
        let mut out = [0_u8; BIN_COUNT];
        g.apply(&raw, &mut out, 1.0);
        assert!(out[10] >= 20);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn reset_restores_unit_gain() {
        let mut g = MicGain::new(48_000);
        let raw = [30_u8; BIN_COUNT];
        let mut out = [0_u8; BIN_COUNT];
        for _ in 0..100 {
            g.apply(&raw, &mut out, 1.0);
        }
        assert!(g.gains()[0] > 1.0);
        g.reset();
        assert_eq!(g.gains(), [1.0, 1.0, 1.0]);
    }
}
