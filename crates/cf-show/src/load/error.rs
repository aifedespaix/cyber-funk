//! Erreurs de chargement : toujours lisibles par un humain, avec le fichier, la ligne et le chemin du champ.

use std::fmt;
use std::path::PathBuf;

use crate::model::FORMAT_VERSION;
use crate::validate::Issue;

/// Pourquoi un fichier d'événement ou de look n'a pas pu être chargé.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Le fichier ou le dossier n'a pas pu être lu.
    #[error("{} : lecture impossible : {source}", path.display())]
    Io {
        /// Fichier ou dossier visé.
        path: PathBuf,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// Le texte n'est pas du RON valide pour ce format (syntaxe, champ inconnu ou manquant, type inattendu,
    /// identifiant mal formé…).
    #[error("{origin}:{line}:{column} : {}{message}", field_prefix(field.as_deref()))]
    Syntax {
        /// Fichier lu (ou description de la source).
        origin: String,
        /// Chemin du champ fautif (`slots[0].cues[2].advance`), quand il est connu.
        field: Option<String>,
        /// Ligne de l'erreur, à partir de 1.
        line: usize,
        /// Colonne de l'erreur, à partir de 1.
        column: usize,
        /// Message du lecteur RON.
        message: String,
    },
    /// Le fichier n'a pas de champ `version`.
    #[error("{origin} : champ « version » absent (attendu en tête du fichier : `version: {FORMAT_VERSION}`)")]
    MissingVersion {
        /// Fichier lu.
        origin: String,
    },
    /// Le fichier vient d'une version plus récente de l'application.
    #[error(
        "{origin} : format en version {found}, plus récent que ce que cette application sait lire \
         (version {FORMAT_VERSION} au plus) : mettre l'application à jour"
    )]
    TooNew {
        /// Fichier lu.
        origin: String,
        /// Version trouvée dans le fichier.
        found: u32,
    },
    /// Le numéro de version n'a jamais existé.
    #[error("{origin} : version de format {found} inconnue (versions connues : 1 à {FORMAT_VERSION})")]
    UnknownVersion {
        /// Fichier lu.
        origin: String,
        /// Version trouvée dans le fichier.
        found: u32,
    },
    /// Un look de la bibliothèque ne porte pas le nom de son fichier.
    #[error("{} : l'identifiant du look « {id} » doit être le nom du fichier (« {stem} »)", path.display())]
    LookIdMismatch {
        /// Fichier du look.
        path: PathBuf,
        /// Identifiant écrit dans le fichier.
        id: String,
        /// Nom du fichier sans extension.
        stem: String,
    },
    /// Le fichier se lit, mais son contenu enfreint des règles (plages, références, doublons).
    #[error("{origin} : {} problème(s)\n{}", issues.len(), IssueList(issues))]
    Invalid {
        /// Fichier lu.
        origin: String,
        /// Tous les problèmes trouvés, chacun avec le chemin de son champ.
        issues: Vec<Issue>,
    },
    /// L'écriture en RON a échoué.
    #[error("écriture RON impossible : {0}")]
    Serialize(#[from] ron::Error),
}

impl LoadError {
    /// Chemin du champ fautif pour une erreur de lecture, ou du premier problème pour une erreur de validation.
    #[must_use]
    pub fn field(&self) -> Option<&str> {
        match self {
            Self::Syntax { field, .. } => field.as_deref(),
            Self::Invalid { issues, .. } => issues.first().map(|issue| issue.path.as_str()),
            _ => None,
        }
    }
}

fn field_prefix(field: Option<&str>) -> String {
    field.map(|f| format!("champ « {f} » : ")).unwrap_or_default()
}

struct IssueList<'a>(&'a [Issue]);

impl fmt::Display for IssueList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, issue) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("\n")?;
            }
            write!(f, "  - {issue}")?;
        }
        Ok(())
    }
}
