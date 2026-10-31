//! Analyse hors ligne des pistes (J1-07) : grille exacte sur un signal connu, cache relu au second lancement.

#![allow(clippy::print_stderr, reason = "bilan chiffré, visible avec --nocapture")]

use std::f64::consts::PI;
use std::path::PathBuf;

use cf_analysis::{CacheStatus, GridCache, TrackGrid, analyse_pcm, spawn_analysis};

const RATE: f64 = 48_000.0;

/// Train de coups de grosse caisse : un coup fort sur le temps 1 de chaque mesure, plus faible sur les trois autres.
fn accented_kicks(bpm: f64, seconds: f64, first_s: f64) -> Vec<f32> {
    let len = (seconds * RATE) as usize;
    let period = 60.0 / bpm;
    let mut out = vec![0.0_f32; len];
    let mut index = 0_u32;
    loop {
        let onset = first_s + f64::from(index) * period;
        if onset >= seconds {
            return out;
        }
        let amplitude = if index.is_multiple_of(4) { 0.9 } else { 0.45 };
        let start = (onset * RATE) as usize;
        for (n, slot) in out.iter_mut().enumerate().skip(start).take((0.6 * RATE) as usize) {
            let t = (n - start) as f64 / RATE;
            *slot += (amplitude * (-t / 0.05).exp() * (2.0 * PI * 55.0 * t).sin()) as f32;
        }
        index += 1;
    }
}

/// Écart (s) entre un kick réel à `t` et le temps entier le plus proche de la grille.
fn phase_error_s(grid: &TrackGrid, t: f64) -> f64 {
    let beat = grid.beat_at(t);
    (beat - beat.round()) * 60.0 / grid.bpm
}

#[test]
fn grid_is_stable_over_a_whole_track() {
    let (bpm, first_s, seconds) = (126.0, 0.4, 120.0);
    let grid = analyse_pcm(&accented_kicks(bpm, seconds, first_s)).expect("grille");
    eprintln!("{grid:?}");
    assert!((grid.bpm - bpm).abs() < 0.1, "tempo {}", grid.bpm);
    assert!(grid.confidence > 0.7, "confiance {}", grid.confidence);
    // Dérive : l'écart de phase d'un kick réel en fin de piste ne s'éloigne pas de celui du début (< 20 ms).
    let period = 60.0 / bpm;
    let early = phase_error_s(&grid, first_s + 4.0 * period);
    let late = phase_error_s(&grid, first_s + 220.0 * period);
    assert!((late - early).abs() < 0.02, "début {early}, fin {late}");
    assert!(grid.first_bar_s <= 0.0 && grid.beat_at(0.0) >= 0.0);
}

#[test]
fn bar_start_follows_the_accented_beat() {
    let (bpm, first_s) = (126.0, 0.4);
    let grid = analyse_pcm(&accented_kicks(bpm, 120.0, first_s)).expect("grille");
    let period = 60.0 / bpm;
    // Les temps 1 réels tombent sur des multiples de 4 de la grille (le retard de détection, constant, est < 0,3 temps).
    for bar in [2.0, 10.0, 30.0] {
        let beat = grid.beat_at(first_s + bar * 4.0 * period);
        let off = (beat / 4.0 - (beat / 4.0).round()).abs() * 4.0;
        assert!(off < 0.3, "mesure {bar} : temps {beat}");
    }
}

#[test]
fn silence_has_no_grid() {
    assert!(analyse_pcm(&vec![0.0; 48_000 * 10]).is_err());
}

fn demo_track() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/music/Tozan - Winning the Race.mp3")
}

#[test]
fn second_open_reads_the_cache() {
    let dir = std::env::temp_dir().join(format!("cf-grid-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = GridCache::new(&dir);

    let (first, status) = cache.load_or_analyse(&demo_track()).expect("analyse");
    assert_eq!(status, CacheStatus::Computed);
    eprintln!("{first:?}");
    assert!(first.bpm > 60.0 && first.bpm < 200.0 && first.duration_s > 30.0);
    assert!(first.confidence > 0.3, "confiance {}", first.confidence);

    let (second, status) = cache.load_or_analyse(&demo_track()).expect("cache");
    assert_eq!(status, CacheStatus::Hit);
    assert_eq!(second, first);

    // Même chose par le thread de fond.
    let mut job = spawn_analysis(cache, demo_track()).expect("thread");
    let result = loop {
        if let Some(result) = job.poll() {
            break result;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    assert_eq!(result.expect("grille").1, CacheStatus::Hit);
    let _ = std::fs::remove_dir_all(&dir);
}
