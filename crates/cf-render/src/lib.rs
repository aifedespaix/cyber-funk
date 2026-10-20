//! Socle de rendu de cyber-funk : ordre des systèmes dans l'image, puis (jalon J4) couches de rendu,
//! post-process, streaming du monde, culling, qualité adaptative et palettes.

pub mod audio;
pub mod diagnostics;
pub mod layers;
pub mod origin;
mod schedule;
pub mod streaming;
pub mod world;

pub use audio::{
    AudioCommand, AudioConfig, AudioFeatures, AudioSource, AudioStatus, CfAudioPlugin, InputBackendChoice, Kick,
    OutputChoice, SectionChanged, VisualLatency,
};
pub use diagnostics::{
    DiagnosticsPlugin, DiagnosticsSnapshot, ExportError, MeasurementSession, PassTiming, RenderScale, SessionRow,
};
pub use layers::{
    BackgroundLayer, COMPOSITOR_ORDER, CameraPose, Compositor, ForegroundLayer, Layer, LayerCamera, LayerOpacity,
    LayerTargets, LayersPlugin, OFF_BELOW, OverlayLayer, WorldLayer,
};
pub use origin::{AbsolutePosition, FloatingOrigin, ORIGIN_STEP, OriginPlugin, OriginTracker, StreamFocus};
pub use schedule::{CfSet, CorePlugin};
pub use streaming::{ChunkInfo, ChunkPlan, ChunkStream, EntityPool, PieceRegistry, StreamLayout, StreamStats};
pub use world::{
    BoxWorld, CurrentGates, Replan, Subject, SubjectKind, Subjects, World, WorldCorePlugin, WorldError, WorldPlugin,
    WorldSet,
};
