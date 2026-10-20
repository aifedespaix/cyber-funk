//! Écriture atomique et lecture des sauvegardes.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use super::{PersistError, ShowSave};

pub(super) fn io_error(path: &Path, action: &'static str, source: std::io::Error) -> PersistError {
    PersistError::Io {
        path: path.to_path_buf(),
        action,
        source,
    }
}

/// `chemin` + `suffix` (`etat.ron` → `etat.ron.tmp`) : même dossier, donc même volume (le `rename` reste atomique).
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Copie de secours d'un fichier d'événement : `show.cfshow` → `show.cfshow.bak`.
#[must_use]
pub fn backup_path(path: &Path) -> PathBuf {
    with_suffix(path, ".bak")
}

/// Écrit `bytes` dans `path` sans jamais laisser un fichier à moitié écrit : fichier temporaire voisin, `sync`, puis
/// `rename` (atomique sur un même volume). Une coupure laisse au pire un `.tmp` orphelin, que la prochaine écriture
/// remplace et que personne ne relit ; l'ancien fichier reste intact.
///
/// # Errors
///
/// [`PersistError::Io`] ; le fichier temporaire est alors supprimé au mieux.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), PersistError> {
    let tmp = with_suffix(path, ".tmp");
    let result = (|| {
        let mut file = fs::File::create(&tmp).map_err(|e| io_error(&tmp, "création", e))?;
        file.write_all(bytes).map_err(|e| io_error(&tmp, "écriture", e))?;
        // Sans `sync`, un rename peut survivre à une coupure de courant alors que les données, non : fichier vide.
        file.sync_all().map_err(|e| io_error(&tmp, "synchronisation", e))?;
        drop(file);
        fs::rename(&tmp, path).map_err(|e| io_error(path, "renommage", e))
    })();
    if result.is_err() {
        // Nettoyage au mieux : l'erreur d'origine est celle qui compte, un `.tmp` restant est inoffensif.
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Écrit la sauvegarde d'état, atomiquement.
///
/// # Errors
///
/// Sérialisation ou écriture impossible : le set continue, sans reprise possible tant que ça dure.
pub fn write_save(path: &Path, save: &ShowSave) -> Result<(), PersistError> {
    write_atomic(path, save.to_ron()?.as_bytes())
}

/// Relit la sauvegarde d'état. `Ok(None)` : pas de sauvegarde, trop vieille (fenêtre de 15 min) ou datée du futur
/// (horloge reculée), comme `readSave` de Sillage. Un `.tmp` orphelin est ignoré.
///
/// # Errors
///
/// [`PersistError::Corrupt`] pour un fichier illisible ; [`PersistError::Io`] pour tout autre échec de lecture.
pub fn read_save(path: &Path, wall_now_s: f64) -> Result<Option<ShowSave>, PersistError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error(path, "lecture", e)),
    };
    let save = ShowSave::from_ron(&text, &path.display().to_string())?;
    Ok(save.is_resumable(wall_now_s).then_some(save))
}

/// Efface la sauvegarde (fin de set normale). Rien à faire si elle n'existe pas.
///
/// # Errors
///
/// [`PersistError::Io`] si le fichier existe mais ne peut pas être supprimé.
pub fn clear_save(path: &Path) -> Result<(), PersistError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(path, "suppression", e)),
    }
}
