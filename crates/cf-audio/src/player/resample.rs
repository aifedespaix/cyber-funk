//! Rééchantillonnage stéréo d'une piste vers la fréquence de la sortie (ex. 44,1 kHz → 48 kHz), sur le thread du
//! décodeur (pas de contrainte temps réel ici, mais les tampons sont tout de même réutilisés d'un paquet à l'autre).
//!
//! Le retard propre au filtre de `rubato` est retiré en tête, et la queue est vidée en fin de piste : la piste
//! rééchantillonnée dure exactement `trames_source × sortie / source` (arrondi), condition d'un enchaînement sans
//! trou et d'une position exacte.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

use super::error::PlayerError;

/// Taille d'un bloc d'entrée (trames). `rubato` découpe en sous-blocs d'environ 256 trames ; ce choix garde un retard
/// de filtre de quelques millisecondes.
const CHUNK_FRAMES: usize = 1024;
const CHANNELS: usize = 2;

/// Rééchantillonneur stéréo entrelacé à rapport fixe.
pub(crate) struct StereoResampler {
    inner: Fft<f32>,
    /// Entrée en attente d'un bloc complet (entrelacée).
    input: Vec<f32>,
    /// Sortie d'un bloc (entrelacée), dimensionnée une fois pour toutes.
    output: Vec<f32>,
    /// Trames de sortie encore à jeter en tête (retard du filtre).
    delay_left: usize,
    input_frames: u64,
    output_frames: u64,
    rate_in: u32,
    rate_out: u32,
}

impl std::fmt::Debug for StereoResampler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StereoResampler")
            .field("rate_in", &self.rate_in)
            .field("rate_out", &self.rate_out)
            .finish_non_exhaustive()
    }
}

impl StereoResampler {
    pub fn new(rate_in: u32, rate_out: u32) -> Result<Self, PlayerError> {
        let inner = Fft::<f32>::new(
            rate_in as usize,
            rate_out as usize,
            CHUNK_FRAMES,
            CHANNELS,
            FixedSync::Input,
        )
        .map_err(|err| PlayerError::Resampler(err.to_string()))?;
        let delay_left = inner.output_delay();
        let output = vec![0.0; inner.output_frames_max() * CHANNELS];
        Ok(Self {
            input: Vec::with_capacity(inner.input_frames_max() * CHANNELS),
            output,
            delay_left,
            inner,
            input_frames: 0,
            output_frames: 0,
            rate_in,
            rate_out,
        })
    }

    /// Ajoute des trames source (stéréo entrelacé) et ajoute à `out` les trames rééchantillonnées disponibles.
    pub fn push(&mut self, samples: &[f32], out: &mut Vec<f32>) -> Result<(), PlayerError> {
        self.input_frames += (samples.len() / CHANNELS) as u64;
        let mut rest = samples;
        while !rest.is_empty() {
            let need = self.inner.input_frames_next() * CHANNELS - self.input.len();
            let take = need.min(rest.len());
            self.input.extend_from_slice(&rest[..take]);
            rest = &rest[take..];
            if self.input.len() == self.inner.input_frames_next() * CHANNELS {
                self.process(None, out)?;
            }
        }
        Ok(())
    }

    /// Fin de piste : traite l'entrée restante et vide le filtre jusqu'à la longueur attendue.
    pub fn finish(&mut self, out: &mut Vec<f32>) -> Result<(), PlayerError> {
        let expected =
            (u128::from(self.input_frames) * u128::from(self.rate_out)).div_ceil(u128::from(self.rate_in)) as u64;
        // Chaque tour produit au moins un bloc de sortie : la boucle termine.
        while self.output_frames < expected {
            let partial = self.input.len() / CHANNELS;
            self.input.resize(self.inner.input_frames_next() * CHANNELS, 0.0);
            let before = out.len();
            self.process(Some(partial), out)?;
            let produced = ((out.len() - before) / CHANNELS) as u64;
            if self.output_frames > expected {
                // Le dernier bloc déborde : on ne garde que la longueur exacte.
                let excess = (self.output_frames - expected) as usize;
                out.truncate(out.len() - excess.min(produced as usize) * CHANNELS);
                self.output_frames = expected;
            }
            if produced == 0 && self.delay_left == 0 {
                break;
            }
        }
        self.reset();
        Ok(())
    }

    /// Oublie l'état du filtre (seek, nouvelle piste).
    pub fn reset(&mut self) {
        self.inner.reset();
        self.input.clear();
        self.delay_left = self.inner.output_delay();
        self.input_frames = 0;
        self.output_frames = 0;
    }

    fn process(&mut self, partial: Option<usize>, out: &mut Vec<f32>) -> Result<(), PlayerError> {
        let frames_in = self.inner.input_frames_next();
        let frames_out = self.inner.output_frames_max();
        let input = InterleavedSlice::new(&self.input, CHANNELS, frames_in)
            .map_err(|err| PlayerError::Resampler(err.to_string()))?;
        let mut output = InterleavedSlice::new_mut(&mut self.output, CHANNELS, frames_out)
            .map_err(|err| PlayerError::Resampler(err.to_string()))?;
        let indexing = partial.map(|len| Indexing::new().partial_len(len));
        let (_, produced) = self
            .inner
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .map_err(|err| PlayerError::Resampler(err.to_string()))?;
        self.input.clear();
        let skip = self.delay_left.min(produced);
        self.delay_left -= skip;
        out.extend_from_slice(&self.output[skip * CHANNELS..produced * CHANNELS]);
        self.output_frames += (produced - skip) as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_length_matches_rate_ratio() {
        let mut resampler = StereoResampler::new(44_100, 48_000).unwrap();
        let mut out = Vec::new();
        // 1,5 s de signal, livré en paquets irréguliers comme un décodeur MP3.
        let input = vec![0.25_f32; 66_150 * 2];
        for packet in input.chunks(1152 * 2 + 6) {
            resampler.push(packet, &mut out).unwrap();
        }
        resampler.finish(&mut out).unwrap();
        assert_eq!(out.len() / 2, 72_000);
    }

    #[test]
    fn delay_is_trimmed_so_a_constant_stays_constant() {
        let mut resampler = StereoResampler::new(44_100, 48_000).unwrap();
        let mut out = Vec::new();
        resampler.push(&vec![0.5_f32; 44_100 * 2], &mut out).unwrap();
        resampler.finish(&mut out).unwrap();
        // Loin des bords (oscillations du filtre sur une marche), le niveau est conservé.
        let middle = &out[2_000 * 2..40_000 * 2];
        assert!(middle.iter().all(|s| (s - 0.5).abs() < 1e-3), "niveau conservé");
        // Le début n'est pas du silence (le retard du filtre a été retiré).
        assert!(out[200] > 0.3);
    }
}
