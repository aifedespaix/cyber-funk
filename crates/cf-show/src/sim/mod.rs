//! Simulateur de set sans rendu (J2-08) : un set entier rejoué en accéléré, au pas de 1/60 s, pour tester le chef du
//! spectacle et lire ce qu'il a fait.
//!
//! ```text
//! SyntheticMusic (tempo, intro / montée / drop / break)  ┐
//! Script (actions et gestes de la musique à des heures)  ┴▶ ShowDirector::update ─▶ Recorder ─▶ Trace (texte)
//! ```
//!
//! Tout est déterministe (graine, pas de temps fixe, aucune horloge murale) : la trace d'un scénario se compare par
//! instantané `insta`. Utilisé par `tests/sim_scenarios.rs` et `examples/simulate.rs`.

mod music;
mod run;
mod script;
mod trace;

pub use music::{MusicFrame, Part, PartKind, SyntheticMusic};
pub use run::{SimError, SimReport, simulate};
pub use script::{Command, Script};
pub use trace::{Recorder, Trace, format_time};
