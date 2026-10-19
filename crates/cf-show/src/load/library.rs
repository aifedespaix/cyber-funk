//! Bibliothèque de looks : un fichier par look dans `assets/looks/`, nommé d'après l'identifiant du look.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{LoadError, parse_look};
use crate::model::{Look, LookId};
use crate::validate::validate_look;

/// Un look de la bibliothèque et le fichier d'où il vient.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryLook {
    /// Le look, validé.
    pub look: Look,
    /// Fichier source (pour les messages et le rechargement).
    pub path: PathBuf,
}

/// Les looks livrés avec l'app ou ajoutés par l'utilisateur, indexés par identifiant.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LookLibrary {
    looks: BTreeMap<LookId, LibraryLook>,
}

impl LookLibrary {
    /// Bibliothèque vide (un événement qui embarque tous ses looks).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Charge tous les fichiers `*.ron` du dossier (sans descendre dans les sous-dossiers), par ordre de nom.
    ///
    /// À appeler à l'ouverture de l'événement, jamais pendant le show.
    ///
    /// # Errors
    ///
    /// Dossier illisible, ou premier fichier invalide (lecture, version, validation, nom de fichier).
    pub fn load_dir(dir: &Path) -> Result<Self, LoadError> {
        let io_error = |source| LoadError::Io {
            path: dir.to_path_buf(),
            source,
        };
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "ron") {
                paths.push(path);
            }
        }
        paths.sort();
        let mut library = Self::new();
        for path in paths {
            let text = std::fs::read_to_string(&path).map_err(|source| LoadError::Io {
                path: path.clone(),
                source,
            })?;
            library.insert_file(&path, &text)?;
        }
        Ok(library)
    }

    /// Ajoute (ou remplace) le look lu dans `text`, venu du fichier `path` : lecture, migration, validation, et
    /// vérification que l'identifiant est bien le nom du fichier.
    ///
    /// # Errors
    ///
    /// Fichier invalide, ou identifiant différent du nom du fichier.
    pub fn insert_file(&mut self, path: &Path, text: &str) -> Result<(), LoadError> {
        let origin = path.display().to_string();
        let look = parse_look(text, &origin)?;
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if look.id.as_str() != stem {
            return Err(LoadError::LookIdMismatch {
                path: path.to_path_buf(),
                id: look.id.to_string(),
                stem,
            });
        }
        let issues = validate_look(&look);
        if !issues.is_empty() {
            return Err(LoadError::Invalid { origin, issues });
        }
        self.looks.insert(
            look.id.clone(),
            LibraryLook {
                look,
                path: path.to_path_buf(),
            },
        );
        Ok(())
    }

    /// Le look de cet identifiant, s'il est dans la bibliothèque.
    #[must_use]
    pub fn get(&self, id: &LookId) -> Option<&LibraryLook> {
        self.looks.get(id)
    }

    /// Vrai si la bibliothèque contient ce look.
    #[must_use]
    pub fn contains(&self, id: &LookId) -> bool {
        self.looks.contains_key(id)
    }

    /// Nombre de looks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.looks.len()
    }

    /// Vrai si la bibliothèque est vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.looks.is_empty()
    }

    /// Les looks, par ordre d'identifiant.
    pub fn iter(&self) -> impl Iterator<Item = (&LookId, &LibraryLook)> {
        self.looks.iter()
    }
}
