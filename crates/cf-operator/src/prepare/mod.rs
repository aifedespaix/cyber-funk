//! La Préparation (J8-02) : éditer l'événement, ses créneaux et leurs cues avant la soirée.
//!
//! ```text
//! PrepConfig ──▶ init_session (Startup) ──▶ PrepSession (événement copié, historique, « modifié »)
//! panel::draw : barre de fichier · listes (clic, glisser-déposer) · formulaire de la sélection
//!   formulaires ──▶ brouillon de l'événement ──▶ PrepSession::commit (une étape d'annulation)
//! ```
//!
//! On édite une copie : le show qui joue n'est jamais modifié (règle 8). Un fichier enregistré se joue en le
//! relançant avec `--show`. Le Live n'affiche rien de tout cela : l'édition y est verrouillée.

mod cue_form;
mod edit;
mod forms;
mod history;
mod lists;
mod panel;
mod session;
mod widgets;

pub(crate) use panel::draw;
pub use session::PrepConfig;
pub(crate) use session::{PrepSession, init_session};
