//! Socle de rendu de cyber-funk : ordre des systèmes dans l'image, puis (jalon J4) couches de rendu,
//! post-process, streaming du monde, culling, qualité adaptative et palettes.

pub mod diagnostics;
pub mod origin;
mod schedule;
pub mod streaming;
pub mod world;

pub use diagnostics::{
    DiagnosticsPlugin, DiagnosticsSnapshot, ExportError, MeasurementSession, PassTiming, RenderScale, SessionRow,
};
pub use origin::{AbsolutePosition, FloatingOrigin, ORIGIN_STEP, OriginPlugin, OriginTracker, StreamFocus};
pub use schedule::{CfSet, CorePlugin};
pub use streaming::{ChunkInfo, ChunkPlan, ChunkStream, EntityPool, PieceRegistry, StreamLayout, StreamStats};
pub use world::{
    BoxWorld, CurrentGates, Replan, Subject, SubjectKind, Subjects, World, WorldCorePlugin, WorldError, WorldPlugin,
    WorldSet,
};
