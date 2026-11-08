//! La séquence de démarrage et d'arrêt du show 3D : la machine pure ([`RunSequencer`]) et sa frise ([`timeline`]).
//!
//! Elle réutilise les cibles de la pré-intro (route, façades, ciel, enseigne, phares, vitesse) ; voir `intro/`.

mod machine;
pub mod timeline;

pub use machine::{RunOutput, RunSequencer};
pub use timeline::RunStages;
