//! Erreurs de sauvegarde, de reprise et d'enregistrement.

use std::path::PathBuf;

use crate::load::LoadError;

/// Pourquoi une sauvegarde ou un enregistrement a échoué.
#[derive(Debug, thiserror::Error)]
pub enum PersistError {
    /// Lecture, écriture, copie ou renommage impossible.
    #[error("{} : {action} impossible : {source}", path.display())]
    Io {
        /// Fichier visé.
        path: PathBuf,
        /// Ce qu'on tentait (« lecture », « écriture »…).
        action: &'static str,
        /// Erreur du système.
        source: std::io::Error,
    },
    /// La sauvegarde d'état est illisible ou d'un format inconnu. L'appelant la journalise et démarre à neuf.
    #[error("sauvegarde illisible : {0}")]
    Corrupt(#[source] LoadError),
    /// Une sauvegarde n'a pas pu être écrite en RON.
    #[error("écriture RON impossible : {0}")]
    Serialize(#[from] ron::Error),
    /// L'événement n'a pas pu être lu ou écrit en RON.
    #[error(transparent)]
    Event(#[from] LoadError),
    /// La sauvegarde vient d'un autre événement.
    #[error("la sauvegarde est celle de « {saved} », pas de « {current} »")]
    WrongEvent {
        /// Événement sauvegardé.
        saved: String,
        /// Événement ouvert.
        current: String,
    },
    /// Le créneau ou le cue sauvegardé n'existe plus (événement modifié depuis la sauvegarde).
    #[error("la sauvegarde vise le créneau {slot}, cue {cue} : introuvable dans l'événement")]
    OutOfRange {
        /// Rang du créneau.
        slot: usize,
        /// Rang du cue.
        cue: usize,
    },
    /// Le chef du spectacle n'a pas pu ouvrir l'événement.
    #[error(transparent)]
    Director(#[from] crate::director::DirectorError),
    /// « Enregistrer » sans fichier : l'événement n'a jamais été ouvert ni enregistré (utiliser « enregistrer sous »).
    #[error("aucun fichier associé à l'événement : utiliser « enregistrer sous »")]
    NoPath,
}
