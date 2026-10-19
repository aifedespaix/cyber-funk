//! Outils communs des tests du lecteur : fichiers WAV synthétiques et pilotage de la sortie manuelle.

// Aides de test : chaque binaire de test n'en utilise qu'une partie, et `allow-unwrap-in-tests` ne couvre que les
// fonctions `#[test]`, pas leurs aides.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use cf_audio::player::{ManualDriver, ManualOutput, Player, PlayerConfig};
use cf_audio::playlist::{PlayMode, Playlist, track_from_path};

pub const RATE: u32 = 48_000;

/// Dossier temporaire propre à un test.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("cf-audio-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Écrit un WAV PCM 16 bits stéréo (les deux voies identiques) de `frames` trames.
pub fn write_wav(path: &Path, rate: u32, frames: usize, sample: impl Fn(usize) -> f32) {
    let channels = 2_u16;
    let data_len = (frames * usize::from(channels) * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes()); // PCM entier
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let value = (sample(i).clamp(-1.0, 1.0) * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        for _ in 0..channels {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    std::fs::write(path, bytes).unwrap();
}

/// Lecteur sur sortie manuelle, volume 1 (gain unité), playlist ordonnée sur `paths`.
pub fn manual_player(paths: &[PathBuf], mode: PlayMode, muted: bool) -> (Player, ManualDriver) {
    let tracks = paths.iter().map(|p| track_from_path(p.clone(), false)).collect();
    let playlist = Playlist::new(tracks, Some(0), 1);
    let (output, driver) = ManualOutput::new(RATE);
    let config = PlayerConfig {
        mode,
        volume: 1.0,
        muted,
    };
    let player = Player::new(playlist, config, Box::new(output)).unwrap();
    (player, driver)
}

/// Rend `frames` trames stéréo par blocs de `block`, en laissant au décodeur le temps de suivre (le test vérifie le
/// signal, pas la vitesse du décodeur). Rend la voie gauche.
pub fn pull_left(driver: &mut ManualDriver, frames: usize, block: usize) -> Vec<f32> {
    let mut left = Vec::with_capacity(frames);
    let mut buf = vec![0.0_f32; block * 2];
    while left.len() < frames {
        let n = block.min(frames - left.len());
        assert!(driver.wait_buffered(n, Duration::from_secs(5)), "le décodeur suit");
        assert!(driver.pull(&mut buf[..n * 2], 2));
        left.extend(buf[..n * 2].iter().step_by(2));
    }
    left
}

pub fn approx(a: f32, b: f32) -> bool {
    // Marge au-dessus du demi-pas de quantification 16 bits (1,5e-5).
    (a - b).abs() < 5e-5
}
