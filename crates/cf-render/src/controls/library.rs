//! La bibliothèque des profils de contrôle (`assets/controllers/<id>.ron`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cf_control::ControlProfile;

/// Erreurs de lecture de la bibliothèque.
#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    /// Le dossier ou un fichier n'a pas pu être lu.
    #[error("lecture de {path} impossible : {source}")]
    Io {
        /// Fichier ou dossier en cause.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// Un fichier n'est pas un profil valide.
    #[error("{path} : {source}")]
    Profile {
        /// Fichier en cause.
        path: PathBuf,
        /// Erreur de `cf-control` (syntaxe RON, bindings en conflit…).
        source: Box<cf_control::Error>,
    },
}

/// Les profils livrés, par identifiant (nom du fichier sans extension).
#[derive(Debug, Clone, Default)]
pub struct ControllerLibrary {
    profiles: BTreeMap<String, ControlProfile>,
}

impl ControllerLibrary {
    /// Bibliothèque vide : seul le clavier par défaut reste actif.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Lit tous les `*.ron` d'un dossier. Un profil invalide est une erreur : on ne découvre pas un mapping cassé
    /// au moment de la passation d'un DJ.
    ///
    /// # Errors
    ///
    /// [`LibraryError`] à la première lecture ou validation qui échoue.
    pub fn load_dir(dir: &Path) -> Result<Self, LibraryError> {
        let io = |path: &Path| {
            let path = path.to_owned();
            move |source| LibraryError::Io { path, source }
        };
        let mut library = Self::new();
        for entry in std::fs::read_dir(dir).map_err(io(dir))? {
            let path = entry.map_err(io(dir))?.path();
            if path.extension().is_none_or(|e| e != "ron") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let text = std::fs::read_to_string(&path).map_err(io(&path))?;
            let profile_error = |source| LibraryError::Profile {
                path: path.clone(),
                source: Box::new(source),
            };
            let profile = ControlProfile::from_ron(&text).map_err(profile_error)?;
            profile.validate().map_err(profile_error)?;
            library.profiles.insert(id.to_owned(), profile);
        }
        Ok(library)
    }

    /// Ajoute (ou remplace) un profil.
    pub fn insert(&mut self, id: impl Into<String>, profile: ControlProfile) {
        self.profiles.insert(id.into(), profile);
    }

    /// Le profil d'identifiant `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&ControlProfile> {
        self.profiles.get(id)
    }

    /// Nombre de profils.
    #[must_use]
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Aucun profil ?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_profils_livres_se_chargent_et_sont_valides() {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/controllers"));
        let library = ControllerLibrary::load_dir(dir).unwrap();
        assert!(library.get("keyboard-sillage").is_some());
        assert!(library.get("generic-8knobs-8pads").is_some());
    }

    #[test]
    fn un_dossier_absent_est_une_erreur_claire() {
        let error = ControllerLibrary::load_dir(Path::new("/nulle/part")).unwrap_err();
        assert!(matches!(error, LibraryError::Io { .. }));
    }
}
