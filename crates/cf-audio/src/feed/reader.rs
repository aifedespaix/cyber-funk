//! Lecture d'un flux d'entrée par pas fixes de 512 échantillons à 48 kHz, avec rééchantillonnage si la source tourne
//! à une autre fréquence.
//!
//! Le rééchantillonnage se fait ici, sur le thread d'analyse, et pas dans le callback : `rubato` recommande de garder
//! le callback léger, et ce thread a le budget pour une FFT par pas. Le rééchantillonneur synchrone `Fft` en sortie
//! fixe de 512 trames produit exactement un pas d'analyse par appel ; son délai (moitié de son bloc FFT, quelques
//! millisecondes) est exposé par [`FeedReader::output_delay`].

use std::fmt;
use std::time::Instant;

use rubato::audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Resampler};

use super::{ANALYSIS_SAMPLE_RATE, FeedError, FeedFault, FeedStats, HOP, InputFeed, Mark};
use crate::source::SourceKind;

/// Résultat d'une tentative de lecture d'un pas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HopStatus {
    /// Un pas complet a été écrit dans le tampon fourni.
    Ready {
        /// Instant où le callback a livré le dernier échantillon d'entrée utilisé par ce pas (`None` si la marque
        /// de ce bloc a été perdue). Sert à mesurer la latence callback → publication.
        delivered_at: Option<Instant>,
    },
    /// Pas encore assez d'échantillons : rien n'a été écrit, l'entrée partielle est gardée pour l'appel suivant.
    Pending,
}

/// Côté lecture d'un flux, propriété du thread d'analyse.
///
/// Tous les tampons sont alloués à la construction ; [`FeedReader::next_hop`], [`FeedReader::drain`] et
/// [`FeedReader::reset`] n'allouent plus rien (vérifié par les tests `assert_no_alloc`).
pub struct FeedReader {
    feed: InputFeed,
    resampler: Option<Fft<f32>>,
    pending: Vec<f32>,
    pending_len: usize,
    consumed: u64,
    delivered_at: Option<Instant>,
    last_data_at: Option<Instant>,
}

impl fmt::Debug for FeedReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeedReader")
            .field("feed", &self.feed)
            .field("resampled", &self.resampler.is_some())
            .field("pending_len", &self.pending_len)
            .field("consumed", &self.consumed)
            .finish_non_exhaustive()
    }
}

impl FeedReader {
    /// Prépare la lecture d'un flux : construit le rééchantillonneur si la source n'est pas à 48 kHz.
    ///
    /// # Errors
    ///
    /// [`FeedError::Resampler`] si `rubato` refuse le couple de fréquences.
    pub fn new(feed: InputFeed) -> Result<Self, FeedError> {
        let resampler = if feed.sample_rate == ANALYSIS_SAMPLE_RATE {
            None
        } else {
            let from = feed.sample_rate;
            let fft = Fft::<f32>::new(from as usize, ANALYSIS_SAMPLE_RATE as usize, HOP, 1, FixedSync::Output)
                .map_err(|source| FeedError::Resampler { from, source })?;
            Some(fft)
        };
        let capacity = resampler.as_ref().map_or(HOP, Resampler::input_frames_max).max(HOP);
        Ok(Self {
            feed,
            resampler,
            pending: vec![0.0; capacity],
            pending_len: 0,
            consumed: 0,
            delivered_at: None,
            last_data_at: None,
        })
    }

    /// Sorte de source lue.
    #[must_use]
    pub fn kind(&self) -> SourceKind {
        self.feed.kind
    }

    /// Fréquence d'origine du flux (Hz).
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.feed.sample_rate
    }

    /// Compteurs partagés du flux.
    #[must_use]
    pub fn stats(&self) -> &FeedStats {
        &self.feed.stats
    }

    /// Panne signalée par le backend, s'il y en a une.
    #[must_use]
    pub fn fault(&self) -> Option<FeedFault> {
        self.feed.stats.fault()
    }

    /// Vrai si le côté écriture a été fermé.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.feed.is_closed()
    }

    /// Dernier instant (horloge du thread d'analyse) où des échantillons ont été trouvés dans le ring.
    #[must_use]
    pub fn last_data_at(&self) -> Option<Instant> {
        self.last_data_at
    }

    /// Délai introduit par le rééchantillonneur, en échantillons à 48 kHz (0 sans rééchantillonnage).
    #[must_use]
    pub fn output_delay(&self) -> usize {
        self.resampler.as_ref().map_or(0, Resampler::output_delay)
    }

    /// Tente de produire le prochain pas de [`HOP`] échantillons à 48 kHz dans `hop`.
    ///
    /// `now` est l'instant courant du thread d'analyse (injecté pour les tests). N'alloue pas, ne bloque pas.
    ///
    /// # Errors
    ///
    /// [`FeedError::Resample`] si `rubato` rejette les tampons : tailles incohérentes, donc bogue de ce module.
    pub fn next_hop(&mut self, hop: &mut [f32; HOP], now: Instant) -> Result<HopStatus, FeedError> {
        let need = self.resampler.as_ref().map_or(HOP, Resampler::input_frames_next);
        let Some(window) = self.pending.get_mut(self.pending_len..need) else {
            // `input_frames_next` ne dépasse jamais `input_frames_max` (taille de `pending`) : cas impossible, on
            // repart d'une entrée vide plutôt que de paniquer.
            self.pending_len = 0;
            return Ok(HopStatus::Pending);
        };
        let (popped, _) = self.feed.samples.pop_partial_slice(window);
        let got = popped.len();
        if got > 0 {
            self.pending_len += got;
            self.consumed += got as u64;
            self.last_data_at = Some(now);
            self.track_delivery();
        }
        if self.pending_len < need {
            return Ok(HopStatus::Pending);
        }
        let input = &self.pending[..need];
        match self.resampler.as_mut() {
            None => hop.copy_from_slice(input),
            Some(resampler) => {
                // Les deux tailles sont garanties par construction ; une erreur ici serait un bogue, remonté tel quel.
                let adapter_in = SequentialSlice::new(input, 1, need).map_err(|_| size_error(need))?;
                let mut adapter_out = SequentialSlice::new_mut(&mut hop[..], 1, HOP).map_err(|_| size_error(HOP))?;
                resampler.process_into_buffer(&adapter_in, &mut adapter_out, None)?;
            }
        }
        self.pending_len = 0;
        Ok(HopStatus::Ready {
            delivered_at: self.delivered_at,
        })
    }

    /// Jette tout ce qui attend dans le flux (source inactive : on la garde « chaude » sans l'analyser).
    pub fn drain(&mut self, now: Instant) {
        let available = self.feed.samples.slots();
        if available > 0 {
            if let Ok(chunk) = self.feed.samples.read_chunk(available) {
                chunk.commit_all();
            }
            self.consumed += available as u64;
            self.last_data_at = Some(now);
        }
        self.pending_len = 0;
        self.track_delivery();
    }

    /// Repart d'un état vide : entrée partielle oubliée, mémoire du rééchantillonneur remise à zéro. Appelé quand
    /// l'analyse bascule vers cette source, pour ne pas mêler un reste ancien au signal frais.
    pub fn reset(&mut self) {
        self.pending_len = 0;
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.reset();
        }
    }

    /// Avance dans les marques jusqu'au bloc qui contient le dernier échantillon consommé.
    ///
    /// L'échantillon d'indice `k` a été livré par la première marque dont `end > k` ; le dernier consommé est
    /// `consumed - 1`, donc on cherche la première marque avec `end >= consumed`. Les marques antérieures
    /// (blocs entièrement consommés) sont retirées.
    fn track_delivery(&mut self) {
        let consumed = self.consumed;
        while let Ok(&Mark { end, at }) = self.feed.marks.peek() {
            self.delivered_at = Some(at);
            if end >= consumed || self.feed.marks.pop().is_err() {
                break;
            }
        }
    }
}

fn size_error(frames: usize) -> FeedError {
    FeedError::BufferSize { frames }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feed::feed;

    fn sine(rate: u32, freq: f32, start: usize, len: usize) -> Vec<f32> {
        (start..start + len)
            .map(|i| 0.5 * (std::f32::consts::TAU * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    /// Fréquence estimée par les passages à zéro montants.
    fn estimate_freq(samples: &[f32], rate: f32) -> f32 {
        let rising = samples.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
        rising as f32 * rate / samples.len() as f32
    }

    #[test]
    fn passthrough_at_48k_produces_exact_hops() {
        let (mut writer, input) = feed(SourceKind::Capture, 48_000, 4096).unwrap();
        let mut reader = FeedReader::new(input).unwrap();
        assert_eq!(reader.output_delay(), 0);
        let ramp: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        writer.push_interleaved(&ramp, 1);
        let mut hop = [0.0; HOP];
        let now = Instant::now();
        assert!(matches!(
            reader.next_hop(&mut hop, now).unwrap(),
            HopStatus::Ready { delivered_at: Some(_) }
        ));
        assert!((hop[0] - 0.0).abs() < f32::EPSILON);
        assert!((hop[511] - 511.0).abs() < f32::EPSILON);
        // 488 restants : pas assez pour un pas, gardés en attente.
        assert_eq!(reader.next_hop(&mut hop, now).unwrap(), HopStatus::Pending);
        let more: Vec<f32> = (1000..1100).map(|i| i as f32).collect();
        writer.push_interleaved(&more, 1);
        assert!(matches!(
            reader.next_hop(&mut hop, now).unwrap(),
            HopStatus::Ready { .. }
        ));
        assert!((hop[0] - 512.0).abs() < f32::EPSILON);
        assert!((hop[511] - 1023.0).abs() < f32::EPSILON);
    }

    #[test]
    fn delivery_instant_is_the_block_holding_the_last_sample() {
        let (mut writer, input) = feed(SourceKind::Capture, 48_000, 4096).unwrap();
        let mut reader = FeedReader::new(input).unwrap();
        writer.push_interleaved(&[0.0_f32; 300], 1);
        let before_second = Instant::now();
        writer.push_interleaved(&[0.0_f32; 300], 1);
        let mut hop = [0.0; HOP];
        let delivered = match reader.next_hop(&mut hop, Instant::now()).unwrap() {
            HopStatus::Ready { delivered_at } => delivered_at,
            HopStatus::Pending => None,
        };
        // Le 512e échantillon vient du second bloc.
        assert!(delivered.unwrap() >= before_second);
    }

    fn resample_check(rate: u32) {
        let (mut writer, input) = feed(SourceKind::Capture, rate, 1 << 16).unwrap();
        let mut reader = FeedReader::new(input).unwrap();
        assert!(reader.output_delay() > 0);
        let mut out = Vec::new();
        let mut hop = [0.0; HOP];
        let block = rate as usize / 100;
        let total = rate as usize * 2;
        let mut sent = 0;
        while sent < total {
            writer.push_interleaved(&sine(rate, 1000.0, sent, block), 1);
            sent += block;
            while let HopStatus::Ready { .. } = reader.next_hop(&mut hop, Instant::now()).unwrap() {
                out.extend_from_slice(&hop);
            }
        }
        // Deux secondes d'entrée donnent deux secondes de sortie, au délai et au dernier pas partiel près.
        let expected = 2 * 48_000 / HOP;
        let hops = out.len() / HOP;
        assert!(
            hops + 3 >= expected && hops <= expected,
            "{hops} pas pour {expected} attendus"
        );
        let settled = &out[4 * HOP..];
        let freq = estimate_freq(settled, 48_000.0);
        assert!((freq - 1000.0).abs() < 10.0, "fréquence {freq} Hz");
        let peak = settled.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.5).abs() < 0.02, "crête {peak}");
    }

    #[test]
    fn resamples_44k1_to_48k() {
        resample_check(44_100);
    }

    #[test]
    fn resamples_96k_to_48k() {
        resample_check(96_000);
    }

    #[test]
    fn drain_discards_and_marks_activity() {
        let (mut writer, input) = feed(SourceKind::Demo, 48_000, 4096).unwrap();
        let mut reader = FeedReader::new(input).unwrap();
        assert!(reader.last_data_at().is_none());
        writer.push_interleaved(&[0.3_f32; 700], 1);
        let now = Instant::now();
        reader.drain(now);
        assert_eq!(reader.last_data_at(), Some(now));
        let mut hop = [0.0; HOP];
        assert_eq!(reader.next_hop(&mut hop, now).unwrap(), HopStatus::Pending);
        writer.push_interleaved(&[0.7_f32; HOP], 1);
        assert!(matches!(
            reader.next_hop(&mut hop, now).unwrap(),
            HopStatus::Ready { .. }
        ));
        assert!(hop.iter().all(|s| (s - 0.7).abs() < 1e-6));
    }

    #[test]
    fn reset_forgets_partial_input() {
        let (mut writer, input) = feed(SourceKind::Capture, 48_000, 4096).unwrap();
        let mut reader = FeedReader::new(input).unwrap();
        writer.push_interleaved(&[1.0_f32; 300], 1);
        let mut hop = [0.0; HOP];
        assert_eq!(reader.next_hop(&mut hop, Instant::now()).unwrap(), HopStatus::Pending);
        reader.reset();
        writer.push_interleaved(&[0.25_f32; HOP], 1);
        assert!(matches!(
            reader.next_hop(&mut hop, Instant::now()).unwrap(),
            HopStatus::Ready { .. }
        ));
        assert!(hop.iter().all(|s| (s - 0.25).abs() < 1e-6));
    }
}
