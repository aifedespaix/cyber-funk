//! Chargement et préchauffe (J4-07) : tout ce que le show utilisera est chargé et compilé avant `Ready`.
//!
//! - [`AppState`] : `Loading` puis `Ready`. Le spectacle ne tourne qu'en `Ready` ([`show_may_run`]).
//! - [`LoadTracker`] : les étapes de chargement (voiture, musiques, logos…) que les plugins inscrivent ; la fenêtre de
//!   l'opérateur en montre la progression ([`WarmupProgress`]).
//! - [`WarmupRegistry`] / [`WarmupAppExt`] : ce que chaque monde déclare vouloir préchauffer (matériaux, variantes).
//! - [`PipelineWatch`] : le compteur du `PipelineCache` ; armé à `Ready`, il journalise en avertissement tout pipeline
//!   compilé pendant le show ([`LatePipelines`] en donne le compte).
//!
//! Le détail du déroulé est dans la doc de [`WarmupPlugin`].

mod curtain;
mod files;
mod mirror;
mod pipelines;
mod plugin;
mod registry;
mod tracker;

pub use curtain::{CURTAIN_ORDER, Curtain};
pub use files::FileJobs;
pub use mirror::WarmEntity;
pub use pipelines::PipelineWatch;
pub use plugin::{
    ASSET_TIMEOUT_S, AppState, COMPILE_TIMEOUT_S, LatePipelines, WarmupPhase, WarmupPlugin, WarmupProgress,
    show_may_run,
};
pub use registry::{WarmupAppExt, WarmupFn, WarmupRegistry};
pub use tracker::{LoadStep, LoadTracker, StepId, StepStatus};
