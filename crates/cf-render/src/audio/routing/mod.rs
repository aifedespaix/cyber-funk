//! Matrice de routage et chaîne de filtrage de l'audio (P2 §2.2 et §2.3, chantier C1).
//!
//! Les shaders et les systèmes de rendu ne lisent plus les `AudioFeatures` bruts : ils lisent [`FilteredSignals`], où
//! chaque *cible* (macro-géométrie, tremblement de caméra, flash laser…) est une valeur déjà normalisée, lissée,
//! bornée et, quand c'est une variation de luminance, passée par le limiteur photosensible de `cf-show`.
//!
//! ```text
//! AudioFeatures ──▶ Route (RON) : source ▶ niveau/AGC ▶ courbe ▶ seuil ▶ lissage ▶ crête ▶ pente ▶ gain/bornes
//!                                          ▶ quantification ▶ limiteur ──▶ FilteredSignals[cible]
//! ```
//!
//! La matrice par défaut (`default_routing.ron`) reproduit P2 §2.3 ; on peut la remplacer par un fichier
//! ([`RoutingMatrix::load`]), validé au chargement.

mod chain;
mod engine;
mod matrix;
mod plugin;
mod signals;
mod types;

pub use engine::RoutingEngine;
pub use matrix::{MAX_ROUTES, RoutingError, RoutingMatrix};
pub use plugin::{RoutingPlugin, VisualDensity};
pub use signals::{FilteredSignals, KICK_NEVER_S, KickWave, visual_phase};
pub use types::{Curve, Detector, LimiterLane, Quantize, Route, Slew, Smooth, Source, Target};

#[cfg(test)]
mod tests;
