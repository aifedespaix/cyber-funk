//! La séquence de démarrage et d'arrêt du show 3D : la machine pure ([`RunSequencer`]), sa frise ([`timeline`]) et son
//! application au monde ([`RunPlugin`]).
//!
//! Elle réutilise les cibles de la pré-intro (route, façades, ciel, enseigne, phares, vitesse) ; voir `intro/`.

mod apply;
pub mod handover;
mod machine;
#[cfg(test)]
mod stages_tests;
#[cfg(test)]
mod tests;
pub mod timeline;
pub mod variant;

pub use apply::{RunDriven, RunPlugin, RunStatus};
pub use machine::{RunOutput, RunSequencer};
pub use timeline::RunStages;
