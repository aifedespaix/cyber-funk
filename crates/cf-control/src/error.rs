//! Erreurs de `cf-control`.

use crate::profile::ProfileIssue;

/// Ce qui peut échouer en chargeant ou en écrivant des profils de contrôle.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Le texte n'est pas un profil RON (la position de l'erreur est dans le message).
    #[error("profil de contrôle illisible : {0}")]
    Parse(#[from] ron::error::SpannedError),
    /// L'écriture RON a échoué.
    #[error("écriture du profil de contrôle impossible : {0}")]
    Serialize(#[from] ron::Error),
    /// Le profil est lisible mais contient des bindings invalides ou en conflit.
    #[error("profil « {profile} » refusé : {}", join(.issues))]
    InvalidProfile { profile: String, issues: Vec<ProfileIssue> },
}

/// Résultat de `cf-control`.
pub type Result<T, E = Error> = std::result::Result<T, E>;

fn join(issues: &[ProfileIssue]) -> String {
    issues.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ; ")
}
