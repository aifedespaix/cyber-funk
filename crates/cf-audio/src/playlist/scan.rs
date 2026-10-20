//! Lecture des dossiers de musique : `assets/music/` (pistes versionnées, licence libre) puis `assets/music/local/`
//! (pistes personnelles, ignorées par git). Équivalent de `sillage-playlist.json.ts` et `local-music.ts`.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use super::Track;
use super::track_name::parse_track_name;

/// Extensions lues (comparées en minuscules). Reprend la liste de Sillage, moins `opus`, `weba` et `webm`, que
/// `symphonia` ne décode pas.
const AUDIO_EXTENSIONS: [&str; 7] = ["mp3", "ogg", "oga", "m4a", "aac", "wav", "flac"];

/// Sous-dossier des pistes personnelles, relatif au dossier de musique.
pub const LOCAL_DIR: &str = "local";

/// Erreur de lecture d'un dossier de musique.
#[derive(Debug, thiserror::Error)]
#[error("lecture du dossier de musique {path:?} impossible")]
pub struct ScanError {
    /// Dossier concerné.
    pub path: PathBuf,
    /// Erreur d'entrée/sortie d'origine.
    #[source]
    pub source: std::io::Error,
}

/// Le fichier est-il une piste que l'on sait lire (d'après son extension) ?
#[must_use]
pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| AUDIO_EXTENSIONS.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// Pistes de `music_dir` puis de `music_dir/local/`, chacune triée par nom (ordre naturel : « 2 » avant « 10 »).
///
/// Un dossier `local/` absent n'est pas une erreur (c'est le cas d'un clone neuf) ; un dossier principal absent non
/// plus : la playlist est alors vide, et l'appelant le signale.
///
/// # Errors
///
/// Rend [`ScanError`] si un dossier existe mais ne peut pas être lu.
pub fn scan_music_dir(music_dir: &Path) -> Result<Vec<Track>, ScanError> {
    let mut tracks = list_dir(music_dir, false)?;
    tracks.extend(list_dir(&music_dir.join(LOCAL_DIR), true)?);
    Ok(tracks)
}

/// Pistes d'un seul dossier (non récursif), triées.
fn list_dir(dir: &Path, local: bool) -> Result<Vec<Track>, ScanError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ScanError {
                path: dir.to_owned(),
                source,
            });
        }
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ScanError {
            path: dir.to_owned(),
            source,
        })?;
        let path = entry.path();
        if path.is_file() && is_audio_file(&path) {
            paths.push(path);
        }
    }
    paths.sort_by(|a, b| natural_cmp(&file_name(a), &file_name(b)));
    Ok(paths.into_iter().map(|path| track_from_path(path, local)).collect())
}

/// Piste décrite par son chemin : artiste et titre tirés du nom de fichier.
#[must_use]
pub fn track_from_path(path: PathBuf, local: bool) -> Track {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (artist, title) = parse_track_name(&stem);
    Track {
        title,
        artist,
        path,
        local,
    }
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Comparaison « naturelle », insensible à la casse : les suites de chiffres se comparent comme des nombres. Approche
/// le `localeCompare(…, { numeric: true, sensitivity: "base" })` de Sillage sans dépendre d'une base ICU.
fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.chars().peekable();
    let mut right = b.chars().peekable();
    loop {
        match (left.peek().copied(), right.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                let ln = take_number(&mut left);
                let rn = take_number(&mut right);
                // Même valeur : le plus court (moins de zéros en tête) d'abord, puis on continue.
                let order = ln
                    .trim_start_matches('0')
                    .len()
                    .cmp(&rn.trim_start_matches('0').len())
                    .then_with(|| {
                        ln.trim_start_matches('0')
                            .cmp(rn.trim_start_matches('0'))
                            .then(ln.len().cmp(&rn.len()))
                    });
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(l), Some(r)) => {
                let order = l.to_lowercase().cmp(r.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                left.next();
                right.next();
            }
        }
    }
}

fn take_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(c) = chars.next_if(char::is_ascii_digit) {
        digits.push(c);
    }
    digits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_order_compares_numbers_as_numbers() {
        let mut names = vec!["Track 10.mp3", "track 2.mp3", "Track 1.mp3", "alpha.mp3"];
        names.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(names, ["alpha.mp3", "Track 1.mp3", "track 2.mp3", "Track 10.mp3"]);
    }

    #[test]
    fn recognises_audio_extensions_case_insensitively() {
        assert!(is_audio_file(Path::new("a/b.MP3")));
        assert!(is_audio_file(Path::new("b.flac")));
        assert!(!is_audio_file(Path::new("LICENCES.md")));
        assert!(!is_audio_file(Path::new("noext")));
    }

    #[test]
    fn missing_dir_gives_empty_playlist() {
        let tracks = scan_music_dir(Path::new("/nonexistent/cyber-funk/music")).unwrap();
        assert_eq!(tracks.len(), 0);
    }

    #[test]
    fn scans_versioned_then_local_tracks() {
        let dir = std::env::temp_dir().join(format!("cf-audio-scan-{}", std::process::id()));
        let local = dir.join(LOCAL_DIR);
        std::fs::create_dir_all(&local).unwrap();
        for name in ["B - Two.mp3", "A - One.wav", "notes.txt"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        std::fs::write(local.join("Mine - Set.flac"), b"").unwrap();

        let tracks = scan_music_dir(&dir).unwrap();
        let summary: Vec<_> = tracks
            .iter()
            .map(|t| (t.artist.as_str(), t.title.as_str(), t.local))
            .collect();
        assert_eq!(
            summary,
            [("A", "One", false), ("B", "Two", false), ("Mine", "Set", true)]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn finds_the_versioned_demo_tracks() {
        let music = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/music");
        let tracks = scan_music_dir(&music).unwrap();
        let versioned = tracks.iter().filter(|t| !t.local).count();
        assert_eq!(versioned, 4, "les quatre démos CC de J0-03");
    }
}
