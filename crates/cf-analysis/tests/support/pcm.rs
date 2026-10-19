//! Décodage d'une piste en PCM mono 48 kHz, comme le script de référence (`tools/sillage-golden/decode.ts`) : décodage,
//! mixage mono `(G + D) / 2` en float, rééchantillonnage vers 48 kHz si besoin.
//!
//! Le script de référence décode avec ffmpeg (rééchantillonneur `swr`) ; ici `symphonia` et `rubato`. Les échantillons
//! ne sont donc pas identiques au bit près, seulement au niveau du bruit de rééchantillonnage.

use std::error::Error;
use std::fs::File;
use std::path::Path;

use rubato::audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

/// Fréquence d'analyse.
const TARGET_RATE: u32 = 48_000;

/// Le même PCM que le script de référence : `ffmpeg` avec le même filtre (`aresample=48000:resampler=swr`), canaux
/// d'origine conservés puis mixage `0,5 · (G + D)`. `None` si `ffmpeg` n'est pas installé ou échoue : les tests qui en
/// dépendent se contentent alors du décodeur de `symphonia`.
pub fn load_mono_48k_ffmpeg(path: &Path, channels: usize) -> Option<Vec<f32>> {
    let output = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-af", "aresample=48000:resampler=swr", "-ac"])
        .arg(channels.to_string())
        .args(["-f", "f32le", "-acodec", "pcm_f32le", "-"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let interleaved: Vec<f32> = output
        .stdout
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    Some(match channels {
        1 => interleaved,
        2 => interleaved.chunks_exact(2).map(|f| 0.5 * (f[0] + f[1])).collect(),
        _ => return None,
    })
}

/// PCM mono 48 kHz de `path` et fréquence d'origine.
pub fn load_mono_48k(path: &Path) -> Result<(Vec<f32>, u32), Box<dyn Error>> {
    let mss = MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
    let mut format = symphonia::default::get_probe().probe(
        &Hint::new(),
        mss,
        FormatOptions::default(),
        MetadataOptions::default(),
    )?;
    let track = format.default_track(TrackType::Audio).ok_or("aucune piste audio")?;
    let track_id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or("paramètres audio absents")?;
    let mut decoder = symphonia::default::get_codecs().make_audio_decoder(params, &AudioDecoderOptions::default())?;

    let mut mono = Vec::new();
    let mut interleaved: Vec<f32> = Vec::new();
    let mut source_rate = 0;
    while let Some(packet) = format.next_packet()? {
        if packet.track_id != track_id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buffer) => {
                source_rate = buffer.spec().rate();
                let channels = buffer.spec().channels().count();
                buffer.copy_to_vec_interleaved(&mut interleaved);
                mono.extend(
                    interleaved
                        .chunks_exact(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32),
                );
            }
            // Trame corrompue : on saute, comme le décodeur de ffmpeg.
            Err(SymphoniaError::DecodeError(_)) => {}
            Err(error) => return Err(error.into()),
        }
    }
    if source_rate == TARGET_RATE || source_rate == 0 {
        return Ok((mono, source_rate));
    }
    let mut resampler = Fft::<f32>::new(source_rate as usize, TARGET_RATE as usize, 1024, 1, FixedSync::Both)?;
    let input = SequentialSlice::new(&mono, 1, mono.len())?;
    let resampled = resampler.process_all(&input, mono.len(), None)?;
    Ok((resampled.take_data(), source_rate))
}
