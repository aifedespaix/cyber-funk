//! Décodage d'une piste avec `symphonia`, vers de la stéréo entrelacée `f32` à la fréquence de la sortie.
//!
//! Tourne sur le thread du décodeur (allocations permises, mais tampons réutilisés de paquet en paquet).

use std::fs::File;
use std::path::{Path, PathBuf};

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, TimeBase, Timestamp};

use super::error::{PlayerError, Result};
use super::resample::StereoResampler;

/// Une piste ouverte, prête à être décodée paquet par paquet.
pub(crate) struct TrackSource {
    path: PathBuf,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    time_base: Option<TimeBase>,
    rate_in: u32,
    rate_out: u32,
    /// `None` quand la piste est déjà à la fréquence de la sortie.
    resampler: Option<StereoResampler>,
    /// Paquet décodé, voies d'origine entrelacées.
    decoded: Vec<f32>,
    /// Paquet ramené en stéréo.
    stereo: Vec<f32>,
    duration_frames: u64,
    /// Trames source à jeter après un seek (le format s'est placé un peu avant la cible).
    skip_frames: u64,
    ended: bool,
}

impl std::fmt::Debug for TrackSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrackSource")
            .field("path", &self.path)
            .field("rate_in", &self.rate_in)
            .field("rate_out", &self.rate_out)
            .finish_non_exhaustive()
    }
}

impl TrackSource {
    /// Ouvre `path` et prépare le décodage vers `rate_out` Hz.
    pub fn open(path: &Path, rate_out: u32) -> Result<Self> {
        let decode_err = |source| PlayerError::Decode {
            path: path.to_owned(),
            source,
        };
        let file = File::open(path).map_err(|source| PlayerError::Open {
            path: path.to_owned(),
            source,
        })?;
        let stream = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let format = symphonia::default::get_probe()
            .probe(&hint, stream, FormatOptions::default(), MetadataOptions::default())
            .map_err(decode_err)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| PlayerError::NoAudioTrack(path.to_owned()))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| PlayerError::NoAudioTrack(path.to_owned()))?;
        let rate_in = params
            .sample_rate
            .ok_or_else(|| PlayerError::NoAudioTrack(path.to_owned()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &AudioDecoderOptions::default())
            .map_err(decode_err)?;
        let track_id = track.id;
        let time_base = track.time_base;
        let duration_frames = duration_in_frames(
            track.num_frames,
            track.duration.map(symphonia::core::units::Duration::get),
            time_base,
            rate_in,
            rate_out,
        );
        let resampler = if rate_in == rate_out {
            None
        } else {
            Some(StereoResampler::new(rate_in, rate_out)?)
        };
        Ok(Self {
            path: path.to_owned(),
            format,
            decoder,
            track_id,
            time_base,
            rate_in,
            rate_out,
            resampler,
            decoded: Vec::new(),
            stereo: Vec::new(),
            duration_frames,
            skip_frames: 0,
            ended: false,
        })
    }

    /// Durée de la piste en trames à la fréquence de sortie, 0 si le conteneur ne la donne pas.
    pub fn duration_frames(&self) -> u64 {
        self.duration_frames
    }

    /// Décode le paquet suivant et ajoute ses trames (stéréo entrelacée, fréquence de sortie) à `out`. Rend `false`
    /// une fois la piste terminée (la queue du rééchantillonneur est alors déjà vidée dans `out`).
    pub fn read(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        if self.ended {
            return Ok(false);
        }
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(packet)) => packet,
                // Fin du flux ; `ResetRequired` (OGG chaîné) et fin de fichier inattendue : on s'arrête là aussi.
                Ok(None) | Err(SymphoniaError::ResetRequired) => return self.end(out),
                Err(SymphoniaError::IoError(err)) if err.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return self.end(out);
                }
                Err(source) => {
                    return Err(PlayerError::Decode {
                        path: self.path.clone(),
                        source,
                    });
                }
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buffer) => {
                    let channels = buffer.spec().channels().count();
                    buffer.copy_to_vec_interleaved(&mut self.decoded);
                    to_stereo(&self.decoded, channels, &mut self.stereo);
                }
                // Paquet corrompu : on le saute, comme le recommande symphonia (un MP3 abîmé reste lisible).
                Err(SymphoniaError::DecodeError(reason)) => {
                    tracing::debug!(path = ?self.path, reason, "paquet illisible sauté");
                    continue;
                }
                Err(SymphoniaError::IoError(err)) => {
                    tracing::debug!(path = ?self.path, %err, "paquet illisible sauté");
                    continue;
                }
                Err(source) => {
                    return Err(PlayerError::Decode {
                        path: self.path.clone(),
                        source,
                    });
                }
            }
            let skip = (self.skip_frames as usize).min(self.stereo.len() / 2);
            self.skip_frames -= skip as u64;
            let frames = &self.stereo[skip * 2..];
            if frames.is_empty() {
                continue;
            }
            match &mut self.resampler {
                Some(resampler) => resampler.push(frames, out)?,
                None => out.extend_from_slice(frames),
            }
            return Ok(true);
        }
    }

    /// Se place à `seconds` depuis le début. Rend la position atteinte en trames à la fréquence de sortie.
    pub fn seek(&mut self, seconds: f64) -> Result<u64> {
        let seconds = seconds.max(0.0);
        let time = Time::try_from_secs_f64(seconds).unwrap_or_default();
        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time,
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|source| PlayerError::Decode {
                path: self.path.clone(),
                source,
            })?;
        self.decoder.reset();
        if let Some(resampler) = &mut self.resampler {
            resampler.reset();
        }
        self.ended = false;
        let required = self.ts_to_source_frames(seeked.required_ts);
        let actual = self.ts_to_source_frames(seeked.actual_ts);
        self.skip_frames = required.saturating_sub(actual);
        Ok((u128::from(required) * u128::from(self.rate_out) / u128::from(self.rate_in)) as u64)
    }

    fn end(&mut self, out: &mut Vec<f32>) -> Result<bool> {
        self.ended = true;
        if let Some(resampler) = &mut self.resampler {
            resampler.finish(out)?;
        }
        Ok(false)
    }

    /// Horodatage du conteneur → trames source. Sans base de temps, les horodatages sont des trames.
    fn ts_to_source_frames(&self, ts: Timestamp) -> u64 {
        let ts = ts.get().max(0) as u128;
        match self.time_base {
            Some(tb) => {
                (ts * u128::from(tb.numer.get()) * u128::from(self.rate_in) / u128::from(tb.denom.get())) as u64
            }
            None => ts as u64,
        }
    }
}

/// Durée en trames de sortie, d'après le nombre de trames de la piste ou, à défaut, sa durée dans la base de temps.
fn duration_in_frames(
    num_frames: Option<u64>,
    duration_ticks: Option<u64>,
    time_base: Option<TimeBase>,
    rate_in: u32,
    rate_out: u32,
) -> u64 {
    let source_frames = num_frames.or_else(|| {
        let (ticks, tb) = (duration_ticks?, time_base?);
        Some((u128::from(ticks) * u128::from(tb.numer.get()) * u128::from(rate_in) / u128::from(tb.denom.get())) as u64)
    });
    source_frames.map_or(0, |frames| {
        (u128::from(frames) * u128::from(rate_out) / u128::from(rate_in)) as u64
    })
}

/// Ramène un paquet entrelacé de `channels` voies en stéréo : mono dupliqué, stéréo tel quel, au-delà les deux
/// premières voies (avant gauche, avant droite) — un mixage complet du 5.1 n'a pas d'intérêt pour des démos.
fn to_stereo(samples: &[f32], channels: usize, out: &mut Vec<f32>) {
    out.clear();
    match channels {
        0 => {}
        1 => out.extend(samples.iter().flat_map(|&s| [s, s])),
        2 => out.extend_from_slice(samples),
        n => out.extend(samples.chunks_exact(n).flat_map(|frame| [frame[0], frame[1]])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_conversion() {
        let mut out = Vec::new();
        to_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, [0.1, 0.1, 0.2, 0.2]);
        to_stereo(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6], 3, &mut out);
        assert_eq!(out, [0.1, 0.2, 0.4, 0.5]);
    }

    #[test]
    fn duration_from_frames_or_time_base() {
        assert_eq!(duration_in_frames(Some(44_100), None, None, 44_100, 48_000), 48_000);
        let tb = TimeBase::try_new(1, 1000);
        assert_eq!(duration_in_frames(None, Some(2_000), tb, 44_100, 48_000), 96_000);
        assert_eq!(duration_in_frames(None, None, None, 44_100, 48_000), 0);
    }

    #[test]
    fn decodes_a_demo_mp3_to_the_output_rate() {
        let music = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/music");
        let path = music.join("Tozan - Winning the Race.mp3");
        let mut source = TrackSource::open(&path, 48_000).unwrap();
        assert!(
            source.duration_frames() > 48_000 * 60,
            "plus d'une minute : {}",
            source.duration_frames()
        );
        let mut out = Vec::new();
        while out.len() < 48_000 * 2 {
            assert!(source.read(&mut out).unwrap());
        }
        assert!(out.iter().any(|s| s.abs() > 0.01), "du son, pas du silence");
        let at = source.seek(30.0).unwrap();
        assert!(at.abs_diff(30 * 48_000) < 48, "seek précis à 1 ms près : {at}");
    }
}
