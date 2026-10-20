//! Lecture des données de référence de Sillage (`crates/cf-analysis/tests/golden/*.json`, produites par
//! `tools/sillage-golden/`). Seuls les champs utiles aux tests sont déclarés ; le format est décrit dans le README du
//! script.

use std::error::Error;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Une piste CC0 et son fichier de référence.
pub const TRACKS: [(&str, &str); 4] = [
    ("enprogames-robotic-city", "enprogames - Robotic City.mp3"),
    ("snabisch-night-prowler", "Snabisch - Night Prowler.mp3"),
    ("tozan-winning-the-race", "Tozan - Winning the Race.mp3"),
    ("yongki-chase", "Yongki - Chase.mp3"),
];

#[derive(Debug, Deserialize)]
pub struct Golden {
    pub track: Track,
    pub analysis: Analysis,
    pub summary: Summary,
    /// Tous les pas où `BeatState.kick` est vrai.
    pub kicks: Vec<u64>,
    /// Tous les changements de `BeatState.bpm`.
    pub bpm: Vec<BpmChange>,
    pub spectra: Vec<Spectrum>,
}

#[derive(Debug, Deserialize)]
pub struct Track {
    #[serde(rename = "pcmSamples")]
    pub pcm_samples: usize,
    #[serde(rename = "sourceChannels")]
    pub source_channels: usize,
}

#[derive(Debug, Deserialize)]
pub struct Analysis {
    pub steps: u64,
    #[serde(rename = "stepRate")]
    pub step_rate: f64,
    #[serde(rename = "stepSamples")]
    pub step_samples: usize,
}

#[derive(Debug, Deserialize)]
pub struct Summary {
    #[serde(rename = "kickCount")]
    pub kick_count: usize,
    #[serde(rename = "steadyBpm")]
    pub steady_bpm: f64,
    #[serde(rename = "finalBpm")]
    pub final_bpm: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct BpmChange {
    pub step: u64,
    pub bpm: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct Spectrum {
    pub step: u64,
    pub bytes: Vec<u8>,
}

impl Golden {
    /// BPM de Sillage après le pas `step` (valeur tenue depuis le dernier changement).
    pub fn bpm_at(&self, step: u64) -> Option<f64> {
        self.bpm
            .iter()
            .take_while(|c| c.step <= step)
            .last()
            .and_then(|c| c.bpm)
    }
}

/// Charge `tests/golden/<name>.json`.
pub fn load(name: &str) -> Result<Golden, Box<dyn Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"));
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

/// Chemin d'une piste CC0 versionnée.
pub fn music_path(file: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/music")
        .join(file)
}
