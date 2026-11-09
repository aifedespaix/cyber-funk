//! Socle de rendu de cyber-funk : ordre des systèmes dans l'image, puis (jalon J4) couches de rendu,
//! post-process, streaming du monde, culling, qualité adaptative et palettes.

mod active_events;
mod affinity;
pub mod audio;
mod automation;
mod car_selection;
pub mod controls;
pub mod culling;
mod debuglog;
pub mod diagnostics;
pub mod district;
mod event_journal;
mod events_catalog;
mod graffiti_tuning;
pub mod layers;
pub mod origin;
pub mod palette;
pub mod postfx;
pub mod quality;
pub mod run;
mod sandbox;
mod schedule;
pub mod screens;
pub mod settings;
pub mod show;
mod sim;
pub mod streaming;
mod timeline;
pub mod titles;
mod tuning;
pub mod warmup;
pub mod world;

pub use active_events::{
    ActiveEvent, ActiveEvents, ActiveEventsPlugin, ActiveId, EventCategory, EventOrigin, expire_active_events,
};
pub use affinity::DistrictAffinity;
pub use audio::{
    AudioCommand, AudioConfig, AudioFeatures, AudioSequenceGain, AudioSource, AudioStatus, CfAudioPlugin,
    InputBackendChoice, Kick, OutputChoice, SectionChanged, VisualLatency,
};
pub use automation::WorldAutomation;
pub use car_selection::{CarOption, CarSelection};
pub use controls::{
    ControlAction, ControlBridge, ControllerLibrary, ControlsConfig, ControlsPlugin, ControlsStatus, LastInput,
    LearnCommand, LibraryError, MidiChoice, MidiEnabled, TextInputFocus, submit_action,
};
pub use culling::{
    Cadence, CullingPlugin, CullingSet, CullingSettings, CullingView, FOG_FULL_M, LodBands, LodClock, SimulationLod,
    aabb_encloses_mesh, displaced_bounds, follow_camera_bounds, lod_ranges, mesh_aabb, padded, until_fog,
};
pub use debuglog::{DebugLog, DebugLogPlugin};
pub use diagnostics::{
    DiagnosticsPlugin, DiagnosticsSnapshot, ExportError, MeasurementSession, PassTiming, RenderScale, SessionRow,
};
pub use district::{DISTRICT_HISTORY, DistrictOrder, DistrictState};
pub use event_journal::{
    ARMED_MAX, ArmedTriggers, EventJournal, EventJournalPlugin, JournalEntry, JournalState, RefusalReason,
    UNCLAIMED_AFTER_S,
};
pub use events_catalog::{WorldEvent, WorldEventAppExt, WorldEventCatalog};
pub use graffiti_tuning::{GraffitiTuning, GraffitiWords, MAX_GRAFFITI_DENSITY};
pub use layers::{
    BackgroundLayer, COMPOSITOR_ORDER, CameraPose, Compositor, ForegroundLayer, Layer, LayerCamera, LayerOpacity,
    LayerTargets, LayerWarmup, LayersPlugin, OFF_BELOW, OVERLAY_COMPOSITOR_ORDER, OverlayCompositor, OverlayLayer,
    WorldLayer,
};
pub use origin::{AbsolutePosition, FloatingOrigin, ORIGIN_STEP, OriginPlugin, OriginTracker, StreamFocus};
pub use palette::{
    DistrictGrade, DistrictGradeState, GradeLibrary, PaletteChanged, PaletteDriver, PaletteError, PalettePlugin,
    PaletteRequest, PaletteTiming, PaletteWave, SceneColor, SceneMood, ScenePalette, ThemeLibrary,
};
pub use postfx::{CurrentShowFx, LensWeather, PostFxPlugin, PostFxSettings, PostFxState, ShowFxInput};
pub use quality::{
    BLOOM_FADE_S, MAX_LEVEL, PIXEL_SCALE, Quality, QualityChanged, QualityIdle, QualityLevers, QualityMode,
    QualityPlugin, levers_for,
};
pub use run::{ARM_AT, Phase, RunHandover, RunOrder, RunPhase, RunPlugin, RunSequenced, SceneIdle};
pub use sandbox::{DebugSandbox, SPEED_PRESETS};
pub use schedule::{CfSet, CorePlugin};
pub use screens::{ScreenInfo, ScreenList, ScreenOrder, describe_screens};
pub use settings::{Settings, SettingsPlugin};
pub use show::{
    CurrentShow, SheetCue, SheetSlot, ShowControls, ShowEventMessage, ShowPlugin, ShowRuntime, ShowSheet, StartSlot,
};
pub use sim::{SimClock, SimControl, SimReset, sim_running};
pub use streaming::{ChunkInfo, ChunkPlan, ChunkStream, EntityPool, PieceRegistry, StreamLayout, StreamStats};
pub use timeline::{DebugTimeline, MAX_REQUESTS, MAX_SPANS, TimelinePlugin, TimelineReplay, TimelineSpan};
pub use titles::{ActiveTitle, TitleCard, TitleKind, TitleSettings, TitlesPlugin};
pub use tuning::{CAR_FOLLOW_KEY, FacadeTuning, WINDOW_LIFE_KEY, store_tuning, tuning_from_entries};
pub use warmup::{
    AppState, LatePipelines, LoadTracker, WarmupAppExt, WarmupPlugin, WarmupProgress, WarmupRegistry, show_may_run,
};
pub use world::{
    BoxWorld, CurrentGates, Replan, Subject, SubjectKind, Subjects, World, WorldCorePlugin, WorldError, WorldPlugin,
    WorldSet,
};
