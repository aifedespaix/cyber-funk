//! Décodage hors ligne d'une piste entière en PCM mono, pour l'analyse de fond (grille de temps en cache).
//!
//! Même décodeur et même rééchantillonneur que la lecture, donc la même durée et les mêmes échantillons que ceux que
//! l'auditeur entend. Le PCM n'est jamais gardé en entier : il passe par blocs au visiteur.

use std::path::Path;

use super::error::Result;
use super::source::TrackSource;

/// Fréquence de travail de la lecture de durée : la durée en trames en dépend, pas la durée en secondes.
const PROBE_RATE_HZ: u32 = 48_000;

/// Durée (s) d'une piste, lue dans son en-tête sans la décoder. À appeler hors du thread audio (ouvre le fichier).
///
/// # Errors
/// Fichier introuvable ou illisible, format non géré, ou durée absente de l'en-tête.
pub fn probe_duration_s(path: &Path) -> Result<f64> {
    let source = TrackSource::open(path, PROBE_RATE_HZ)?;
    match source.duration_frames() {
        0 => Err(super::error::PlayerError::Decode {
            path: path.to_owned(),
            source: symphonia::core::errors::Error::Unsupported("durée absente de l'en-tête"),
        }),
        frames => Ok(frames as f64 / f64::from(PROBE_RATE_HZ)),
    }
}

/// Décode `path` et donne au visiteur le mono (moyenne gauche/droite) à la fréquence `rate_hz`, par blocs successifs.
/// Rend le nombre total d'échantillons livrés.
///
/// # Errors
/// Fichier introuvable ou illisible, format non géré, erreur de décodage.
pub fn decode_mono(path: &Path, rate_hz: u32, mut visit: impl FnMut(&[f32])) -> Result<u64> {
    let mut source = TrackSource::open(path, rate_hz)?;
    let mut stereo: Vec<f32> = Vec::new();
    let mut mono: Vec<f32> = Vec::new();
    let mut total = 0_u64;
    loop {
        stereo.clear();
        let more = source.read(&mut stereo)?;
        mono.clear();
        mono.extend(stereo.as_chunks::<2>().0.iter().map(|f| f32::midpoint(f[0], f[1])));
        if !mono.is_empty() {
            total += mono.len() as u64;
            visit(&mono);
        }
        if !more {
            return Ok(total);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WAV PCM 16 bits mono de `seconds` secondes de silence à 8 kHz.
    fn silent_wav(seconds: u32) -> Vec<u8> {
        let data_len = 8_000 * 2 * seconds;
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8_000_u32.to_le_bytes());
        wav.extend_from_slice(&16_000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        wav.resize(wav.len() + data_len as usize, 0);
        wav
    }

    #[test]
    fn la_duree_d_un_wav_est_lue_dans_l_en_tete() {
        let dir = tempfile_dir("probe-ok");
        let path = dir.join("silence.wav");
        std::fs::write(&path, silent_wav(3)).unwrap();
        let duration = probe_duration_s(&path).unwrap();
        assert!((duration - 3.0).abs() < 0.01, "durée lue : {duration}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn un_fichier_qui_n_est_pas_de_l_audio_donne_une_erreur() {
        let dir = tempfile_dir("probe-bad");
        let path = dir.join("faux.mp3");
        std::fs::write(&path, b"ceci n'est pas de l'audio").unwrap();
        assert!(probe_duration_s(&path).is_err());
        assert!(probe_duration_s(&dir.join("absent.wav")).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn tempfile_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cf-audio-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
