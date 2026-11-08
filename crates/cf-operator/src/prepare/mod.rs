//! La Préparation (J8-02) : éditer l'événement, ses créneaux et leurs cues avant la soirée.
//!
//! ```text
//! PrepConfig + OperatorPrefs ──▶ tabs::settings::restore::init_session (Startup) ──▶ PrepSession
//!   (événement copié, historique, « modifié », profil actif)
//! tabs::settings::draw : profils · import/export · liste des DJ · fiche du DJ (formulaires d'ici)
//!   formulaires ──▶ brouillon de l'événement ──▶ PrepSession::commit (une étape d'annulation)
//! ```
//!
//! On édite une copie : le show qui joue n'est jamais modifié (règle 8). Un fichier enregistré se joue au prochain
//! lancement (profil actif, ou `--show`). Le Live n'affiche rien de tout cela : l'édition y est verrouillée.

pub(crate) mod edit;
pub(crate) mod forms;
mod history;
pub(crate) mod lists;
pub(crate) mod panel;
mod session;
pub(crate) mod widgets;

pub use session::PrepConfig;
pub(crate) use session::{PrepSession, Selection, Status};
