//! Un événement ouvert : ouvrir, enregistrer, enregistrer sous, avec copie de secours avant tout écrasement.

use std::fs;
use std::path::{Path, PathBuf};

use super::io::io_error;
use super::{PersistError, backup_path, write_atomic};
use crate::load::{LoadedShow, LookLibrary, event_to_ron};
use crate::model::Event;

/// Un événement et le fichier `.cfshow` d'où il vient (ou où il a été enregistré).
#[derive(Debug, Clone, PartialEq)]
pub struct EventDocument {
    event: Event,
    path: Option<PathBuf>,
}

impl EventDocument {
    /// Événement neuf, sans fichier : le premier enregistrement passe par « enregistrer sous ».
    #[must_use]
    pub fn new(event: Event) -> Self {
        Self { event, path: None }
    }

    /// Ouvre, migre et valide un fichier d'événement.
    ///
    /// # Errors
    ///
    /// [`PersistError::Event`] : fichier illisible, invalide ou d'une version inconnue.
    pub fn open(path: &Path, library: &LookLibrary) -> Result<(Self, LoadedShow), PersistError> {
        let show = LoadedShow::load(path, library)?;
        let doc = Self {
            event: show.event().clone(),
            path: Some(path.to_path_buf()),
        };
        Ok((doc, show))
    }

    /// L'événement.
    #[must_use]
    pub fn event(&self) -> &Event {
        &self.event
    }

    /// L'événement, modifiable (éditeur).
    pub fn event_mut(&mut self) -> &mut Event {
        &mut self.event
    }

    /// Fichier associé, s'il y en a un.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Enregistre dans le fichier associé.
    ///
    /// # Errors
    ///
    /// [`PersistError::NoPath`] sans fichier ; sinon voir [`Self::save_as`].
    pub fn save(&self) -> Result<(), PersistError> {
        let path = self.path.as_deref().ok_or(PersistError::NoPath)?;
        write_event(&self.event, path)
    }

    /// Enregistre sous un autre nom, qui devient le fichier associé. Si le fichier existe, sa version précédente est
    /// d'abord copiée dans `<nom>.bak`. Le fichier associé ne change qu'en cas de réussite.
    ///
    /// # Errors
    ///
    /// Sérialisation, copie de secours ou écriture impossible : l'ancien fichier est alors intact.
    pub fn save_as(&mut self, path: &Path) -> Result<(), PersistError> {
        write_event(&self.event, path)?;
        self.path = Some(path.to_path_buf());
        Ok(())
    }
}

/// Sérialise d'abord (une erreur ne touche à rien), copie l'ancien fichier en `.bak` s'il existe, puis écrit
/// atomiquement. La copie de secours est elle-même atomique : un `.bak` n'est jamais à moitié écrit.
fn write_event(event: &Event, path: &Path) -> Result<(), PersistError> {
    let text = event_to_ron(event)?;
    match fs::read(path) {
        Ok(previous) => write_atomic(&backup_path(path), &previous)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_error(path, "lecture avant écrasement", e)),
    }
    write_atomic(path, text.as_bytes())
}
