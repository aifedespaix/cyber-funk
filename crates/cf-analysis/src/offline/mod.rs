//! Analyse hors ligne des pistes de démo : une grille de temps exacte, gardée en cache.
//!
//! ```text
//! première ouverture : décodage (cf-audio) ─▶ BeatAnalyzer pas à pas ─▶ ajustement de grille ─▶ cache RON
//! ouvertures suivantes : hash du fichier ─▶ cache RON ─▶ TrackGrid          (aucune réanalyse)
//! lecture : TrackGrid::reading(position de la piste, now) ─▶ TempoArbiter::offer(TempoSource::File, …)
//! ```
//!
//! La grille donne le tempo et la phase de toute la piste, mesures comprises (`bar_aligned`). L'énergie (basses,
//! kicks, sections en direct) reste celle de l'analyse temps réel : seule la source de tempo change.
//!
//! La clé du cache est un hash du contenu du fichier, pas son chemin : renommer ou déplacer une piste ne coûte rien,
//! la modifier invalide son entrée.

mod analyse;
mod fit;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::thread::JoinHandle;

use cf_audio::feed::ANALYSIS_SAMPLE_RATE;
use cf_audio::player::{PlayerError, decode_mono};
use crossbeam_channel::{Receiver, TryRecvError};
use serde::{Deserialize, Serialize};

use crate::features::Section;
use crate::tempo::TempoReading;

/// Version du format de cache et de l'algorithme : à incrémenter quand l'un des deux change.
pub const GRID_VERSION: u32 = 1;

/// Une section et l'instant où elle commence (s depuis le début de la piste).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SectionSpan {
    /// Début de la section (s).
    pub start_s: f64,
    /// Section en cours à partir de là.
    pub section: Section,
}

/// Grille de temps d'une piste, en temps de piste (secondes depuis le début du fichier).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackGrid {
    /// Version du format ([`GRID_VERSION`]).
    pub version: u32,
    /// Hash du contenu du fichier analysé.
    pub source_hash: u64,
    /// Durée de la piste (s).
    pub duration_s: f64,
    /// Tempo stable (BPM).
    pub bpm: f64,
    /// Instant d'un début de mesure (s), ramené dans `(-4 temps, 0]` : la position en temps est positive dès le
    /// début de la piste, et ses multiples de 4 sont les débuts de mesure.
    pub first_bar_s: f64,
    /// Confiance 0..1 (écart des kicks à la grille).
    pub confidence: f32,
    /// Nombre de kicks qui ont servi à l'ajustement.
    pub kicks: usize,
    /// Sections de la piste, dans l'ordre (la première commence à 0).
    pub sections: Vec<SectionSpan>,
}

impl TrackGrid {
    /// Position en temps (noires) à `track_s`, la mesure commençant aux multiples de 4.
    #[must_use]
    pub fn beat_at(&self, track_s: f64) -> f64 {
        (track_s - self.first_bar_s) * self.bpm / 60.0
    }

    /// Lecture à offrir à l'arbitre (`TempoSource::File`) : `track_s` est la position de lecture de la piste et `now_s`
    /// l'instant de l'application correspondant.
    #[must_use]
    pub fn reading(&self, track_s: f64, now_s: f64) -> TempoReading {
        TempoReading {
            time_s: now_s,
            beat: self.beat_at(track_s),
            bpm: self.bpm,
            bar_aligned: true,
            confidence: self.confidence,
        }
    }

    /// Section de la piste à `track_s`.
    #[must_use]
    pub fn section_at(&self, track_s: f64) -> Section {
        self.sections
            .iter()
            .rev()
            .find(|s| s.start_s <= track_s)
            .map_or_else(Section::default, |s| s.section)
    }
}

/// Ce qui peut empêcher d'obtenir la grille d'une piste.
#[derive(Debug, thiserror::Error)]
pub enum GridError {
    /// Fichier illisible ou format non géré.
    #[error("décodage de la piste impossible : {0}")]
    Decode(#[from] PlayerError),
    /// Lecture du fichier (hash) ou du dossier de cache.
    #[error("accès au fichier impossible : {0}")]
    Io(#[from] std::io::Error),
    /// Pas de tempo exploitable (silence, ambiant, trop peu de kicks).
    #[error("aucun tempo stable dans cette piste")]
    NoTempo,
    /// Écriture du cache impossible.
    #[error("écriture du cache impossible : {0}")]
    Encode(#[from] ron::Error),
}

/// D'où est venue la grille.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    /// Relue depuis le cache.
    Hit,
    /// Calculée, puis écrite dans le cache.
    Computed,
}

/// Dossier de cache des grilles : un fichier RON par piste, nommé d'après le hash du contenu.
#[derive(Debug, Clone)]
pub struct GridCache {
    dir: PathBuf,
}

impl GridCache {
    /// Cache dans `dir` (créé à la première écriture).
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Cache dans le dossier de cache de l'utilisateur (`…/cyber-funk/grids`), s'il existe sur cette plateforme.
    #[must_use]
    pub fn in_user_cache() -> Option<Self> {
        dirs::cache_dir().map(|d| Self::new(d.join("cyber-funk").join("grids")))
    }

    fn entry(&self, hash: u64) -> PathBuf {
        self.dir.join(format!("{hash:016x}.ron"))
    }

    /// Grille de la piste `path` : relue du cache si elle y est (journal « relue depuis le cache »), sinon analysée
    /// en entier puis écrite. Bloquant et lourd la première fois : à appeler depuis un thread de fond
    /// ([`spawn_analysis`]).
    ///
    /// # Errors
    /// Voir [`GridError`]. Un cache illisible ou périmé n'est pas une erreur : la piste est réanalysée.
    pub fn load_or_analyse(&self, path: &Path) -> Result<(TrackGrid, CacheStatus), GridError> {
        let hash = hash_file(path)?;
        let entry = self.entry(hash);
        if let Some(grid) = read_entry(&entry, hash) {
            tracing::info!(piste = %path.display(), bpm = grid.bpm, "grille relue depuis le cache");
            return Ok((grid, CacheStatus::Hit));
        }
        tracing::info!(piste = %path.display(), "analyse hors ligne de la piste (première ouverture)");
        let grid = analyse_file(path, hash)?;
        if let Err(error) = self.write_entry(&entry, &grid) {
            // Le cache est un confort : sans lui, la grille reste utilisable, on réanalysera au prochain lancement.
            tracing::warn!(%error, "cache de grille non écrit");
        }
        Ok((grid, CacheStatus::Computed))
    }

    fn write_entry(&self, entry: &Path, grid: &TrackGrid) -> Result<(), GridError> {
        std::fs::create_dir_all(&self.dir)?;
        let text = ron::ser::to_string_pretty(grid, ron::ser::PrettyConfig::default())?;
        // Écriture puis renommage : un arrêt en cours d'écriture ne laisse pas d'entrée tronquée.
        let tmp = entry.with_extension("ron.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, entry)?;
        Ok(())
    }
}

/// Entrée de cache valide pour `hash`, ou `None` (absente, illisible, d'une autre version).
fn read_entry(entry: &Path, hash: u64) -> Option<TrackGrid> {
    let text = std::fs::read_to_string(entry).ok()?;
    match ron::from_str::<TrackGrid>(&text) {
        Ok(grid) if grid.version == GRID_VERSION && grid.source_hash == hash => Some(grid),
        Ok(_) => None,
        Err(error) => {
            tracing::warn!(%error, entry = %entry.display(), "entrée de cache illisible, réanalyse");
            None
        }
    }
}

/// Analyse la piste `path` en entier, sans cache.
///
/// # Errors
/// Voir [`GridError`].
pub fn analyse_file(path: &Path, source_hash: u64) -> Result<TrackGrid, GridError> {
    let mut collector = analyse::Collector::new();
    decode_mono(path, ANALYSIS_SAMPLE_RATE, |chunk| collector.push(chunk))?;
    collector.finish(source_hash).ok_or(GridError::NoTempo)
}

/// Analyse un PCM mono 48 kHz déjà en mémoire (tests, outils).
///
/// # Errors
/// [`GridError::NoTempo`] sans tempo exploitable.
pub fn analyse_pcm(mono_48k: &[f32]) -> Result<TrackGrid, GridError> {
    let mut collector = analyse::Collector::new();
    collector.push(mono_48k);
    collector.finish(0).ok_or(GridError::NoTempo)
}

/// Hash FNV-1a 64 bits du contenu d'un fichier. Choisi pour sa stabilité (le hasheur standard peut changer d'une
/// version de Rust à l'autre et invaliderait tous les caches) ; il n'a pas à résister à un adversaire.
///
/// # Errors
/// Fichier illisible.
pub fn hash_file(path: &Path) -> std::io::Result<u64> {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut file = File::open(path)?;
    let mut buffer = vec![0_u8; 1 << 16];
    let mut hash = OFFSET;
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hash);
        }
        for byte in &buffer[..read] {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(PRIME);
        }
    }
}

/// Une analyse en cours sur un thread de fond.
#[derive(Debug)]
pub struct GridJob {
    result: Receiver<Result<(TrackGrid, CacheStatus), GridError>>,
    handle: Option<JoinHandle<()>>,
}

impl GridJob {
    /// Le résultat s'il est prêt (une seule fois), sans attendre.
    pub fn poll(&mut self) -> Option<Result<(TrackGrid, CacheStatus), GridError>> {
        match self.result.try_recv() {
            Ok(result) => {
                if let Some(handle) = self.handle.take() {
                    let _ = handle.join();
                }
                Some(result)
            }
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }
}

/// Lance [`GridCache::load_or_analyse`] sur un thread de fond (`cf-grid`).
///
/// # Errors
/// Le thread n'a pas pu démarrer.
pub fn spawn_analysis(cache: GridCache, path: PathBuf) -> std::io::Result<GridJob> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let handle = std::thread::Builder::new().name("cf-grid".into()).spawn(move || {
        let _ = tx.send(cache.load_or_analyse(&path));
    })?;
    Ok(GridJob {
        result: rx,
        handle: Some(handle),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> TrackGrid {
        TrackGrid {
            version: GRID_VERSION,
            source_hash: 7,
            duration_s: 100.0,
            bpm: 120.0,
            first_bar_s: -0.5,
            confidence: 0.9,
            kicks: 100,
            sections: vec![
                SectionSpan {
                    start_s: 0.0,
                    section: Section::Groove,
                },
                SectionSpan {
                    start_s: 30.0,
                    section: Section::Peak,
                },
            ],
        }
    }

    #[test]
    fn beats_start_positive_and_bars_are_multiples_of_four() {
        let g = grid();
        assert!((g.beat_at(0.0) - 1.0).abs() < 1e-12);
        assert!((g.beat_at(1.5) - 4.0).abs() < 1e-12);
        assert!(g.reading(1.5, 10.0).bar_aligned);
    }

    #[test]
    fn section_follows_the_track_position() {
        let g = grid();
        assert_eq!(g.section_at(10.0), Section::Groove);
        assert_eq!(g.section_at(30.0), Section::Peak);
    }

    #[test]
    fn grid_round_trips_through_ron() {
        let g = grid();
        let text = ron::ser::to_string_pretty(&g, ron::ser::PrettyConfig::default()).expect("ron");
        assert_eq!(ron::from_str::<TrackGrid>(&text).expect("lecture"), g);
    }
}
