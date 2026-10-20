//! Chargement des fichiers RON : bibliothèque de looks, événement, fusion des deux et validation.
//!
//! Tout se charge à l'ouverture de l'événement (règle 8 : aucun chargement pendant un show). Un fichier invalide
//! donne une [`LoadError`] lisible, jamais une panique.

mod error;
mod library;
mod version;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use error::LoadError;
pub use library::{LibraryLook, LookLibrary};
pub use version::{parse_event, parse_look};

pub(crate) use version::{check_version, deserialize};

use crate::model::{Cue, Event, Look, LookId};
use crate::validate::validate_event;

/// Écrit un événement en RON (format courant, lisible, relu à l'identique par [`parse_event`]).
///
/// # Errors
///
/// Échec du sérialiseur RON (ne devrait pas arriver avec le modèle).
pub fn event_to_ron(event: &Event) -> Result<String, LoadError> {
    version::serialize(event)
}

/// Écrit un look en RON (format courant, lisible, relu à l'identique par [`parse_look`]).
///
/// # Errors
///
/// Échec du sérialiseur RON (ne devrait pas arriver avec le modèle).
pub fn look_to_ron(look: &Look) -> Result<String, LoadError> {
    version::serialize(look)
}

/// D'où vient un look utilisé par l'événement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookOrigin {
    /// Fichier de la bibliothèque.
    Library(PathBuf),
    /// Embarqué dans l'événement.
    Event {
        /// Fichier de la bibliothèque de même identifiant, masqué par ce look (l'événement prime).
        shadows: Option<PathBuf>,
    },
}

/// Un look disponible pour l'événement, et son origine.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedLook {
    /// Le look, validé.
    pub look: Look,
    /// Bibliothèque ou événement.
    pub origin: LookOrigin,
}

/// Un événement validé et tous les looks qu'il peut jouer (bibliothèque + looks embarqués, ceux de l'événement
/// priment). Toutes ses références de looks existent.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadedShow {
    event: Event,
    looks: BTreeMap<LookId, ResolvedLook>,
}

impl LoadedShow {
    /// Lit, migre et valide un fichier d'événement.
    ///
    /// # Errors
    ///
    /// Fichier illisible, RON invalide, version non prise en charge, ou contenu invalide.
    pub fn load(path: &Path, library: &LookLibrary) -> Result<Self, LoadError> {
        let text = std::fs::read_to_string(path).map_err(|source| LoadError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_ron(&text, &path.display().to_string(), library)
    }

    /// Lit, migre et valide un événement écrit en RON ; `origin` nomme la source dans les messages d'erreur.
    ///
    /// # Errors
    ///
    /// RON invalide, version non prise en charge, ou contenu invalide.
    pub fn from_ron(text: &str, origin: &str, library: &LookLibrary) -> Result<Self, LoadError> {
        Self::new(parse_event(text, origin)?, library, origin)
    }

    /// Valide un événement déjà en mémoire (éditeur) contre la bibliothèque.
    ///
    /// # Errors
    ///
    /// [`LoadError::Invalid`] avec tous les problèmes trouvés.
    pub fn new(event: Event, library: &LookLibrary, origin: &str) -> Result<Self, LoadError> {
        let mut looks: BTreeMap<LookId, ResolvedLook> = library
            .iter()
            .map(|(id, entry)| {
                let resolved = ResolvedLook {
                    look: entry.look.clone(),
                    origin: LookOrigin::Library(entry.path.clone()),
                };
                (id.clone(), resolved)
            })
            .collect();
        for look in &event.looks {
            let shadows = library.get(&look.id).map(|entry| entry.path.clone());
            looks.insert(
                look.id.clone(),
                ResolvedLook {
                    look: look.clone(),
                    origin: LookOrigin::Event { shadows },
                },
            );
        }
        let issues = validate_event(&event, &|id| looks.contains_key(id));
        if !issues.is_empty() {
            return Err(LoadError::Invalid {
                origin: origin.to_owned(),
                issues,
            });
        }
        Ok(Self { event, looks })
    }

    /// L'événement validé.
    #[must_use]
    pub fn event(&self) -> &Event {
        &self.event
    }

    /// Le look de cet identifiant (celui de l'événement s'il en embarque un, sinon celui de la bibliothèque).
    #[must_use]
    pub fn look(&self, id: &LookId) -> Option<&ResolvedLook> {
        self.looks.get(id)
    }

    /// Tous les looks disponibles, par ordre d'identifiant (de quoi tout précharger).
    pub fn looks(&self) -> impl Iterator<Item = (&LookId, &ResolvedLook)> {
        self.looks.iter()
    }

    /// Le look d'un cue, surcharges appliquées (copie : à faire à la préparation du cue, pas par image).
    /// `None` si le cue n'appartient pas à cet événement et référence un look inconnu.
    #[must_use]
    pub fn cue_look(&self, cue: &Cue) -> Option<Look> {
        self.looks
            .get(&cue.look)
            .map(|resolved| cue.overrides.apply(&resolved.look))
    }
}
