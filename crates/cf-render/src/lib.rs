//! Socle de rendu de cyber-funk : ordre des systèmes dans l'image, puis (jalon J4) couches de rendu,
//! post-process, streaming du monde, culling, qualité adaptative et palettes.

pub mod audio;
pub mod controls;
pub mod culling;
pub mod diagnostics;
pub mod layers;
pub mod origin;
pub mod palette;
pub mod postfx;
pub mod quality;
mod schedule;
pub mod settings;
pub mod show;
pub mod streaming;
pub mod titles;
pub mod warmup;
pub mod world;

pub use audio::{
    AudioCommand, AudioConfig, AudioFeatures, AudioSource, AudioStatus, CfAudioPlugin, InputBackendChoice, Kick,
    OutputChoice, SectionChanged, VisualLatency,
};
pub use controls::{
    ControlAction, ControlBridge, ControllerLibrary, ControlsConfig, ControlsPlugin, ControlsStatus, LastInput,
    LearnCommand, LibraryError, MidiChoice, TextInputFocus, submit_action,
};
pub use culling::{
    Cadence, CullingPlugin, CullingSet, CullingSettings, CullingView, FOG_FULL_M, LodBands, LodClock, SimulationLod,
    aabb_encloses_mesh, displaced_bounds, follow_camera_bounds, lod_ranges, mesh_aabb, padded, until_fog,
};
pub use diagnostics::{
    DiagnosticsPlugin, DiagnosticsSnapshot, ExportError, MeasurementSession, PassTiming, RenderScale, SessionRow,
};
pub use layers::{
    BackgroundLayer, COMPOSITOR_ORDER, CameraPose, Compositor, ForegroundLayer, Layer, LayerCamera, LayerOpacity,
    LayerTargets, LayerWarmup, LayersPlugin, OFF_BELOW, OVERLAY_COMPOSITOR_ORDER, OverlayCompositor, OverlayLayer,
    WorldLayer,
};
pub use origin::{AbsolutePosition, FloatingOrigin, ORIGIN_STEP, OriginPlugin, OriginTracker, StreamFocus};
pub use palette::{
    PaletteChanged, PaletteDriver, PaletteError, PalettePlugin, PaletteRequest, PaletteTiming, PaletteWave, SceneColor,
    SceneMood, ScenePalette, ThemeLibrary,
};
pub use postfx::{CurrentShowFx, LensWeather, PostFxPlugin, PostFxSettings, PostFxState, ShowFxInput};
pub use quality::{
    BLOOM_FADE_S, MAX_LEVEL, PIXEL_SCALE, Quality, QualityChanged, QualityIdle, QualityLevers, QualityMode,
    QualityPlugin, levers_for,
};
pub use schedule::{CfSet, CorePlugin};
pub use settings::{Settings, SettingsPlugin};
pub use show::{CurrentShow, SheetCue, SheetSlot, ShowControls, ShowEventMessage, ShowPlugin, ShowRuntime, ShowSheet};
pub use streaming::{ChunkInfo, ChunkPlan, ChunkStream, EntityPool, PieceRegistry, StreamLayout, StreamStats};
pub use titles::{ActiveTitle, TitleCard, TitleKind, TitleSettings, TitlesPlugin};
pub use warmup::{
    AppState, LatePipelines, LoadTracker, WarmupAppExt, WarmupPlugin, WarmupProgress, WarmupRegistry, show_may_run,
};
pub use world::{
    BoxWorld, CurrentGates, Replan, Subject, SubjectKind, Subjects, World, WorldCorePlugin, WorldError, WorldPlugin,
    WorldSet,
};
