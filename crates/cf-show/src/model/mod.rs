//! Modèle de données du show : Événement → Créneaux DJ → Cues → Looks.
//!
//! Ce sont des données pures (`serde`), lues depuis des fichiers RON par [`crate::load`] et vérifiées par
//! [`crate::validate`]. Les noms des portes reprennent ceux de Sillage (`ShowGates`, `CutGrid`…) pour garder la
//! correspondance lisible avec `engine/show/types.ts` et `engine/show/acts.ts`.
//!
//! Exemple de look (`assets/looks/<id>.ron`) :
//!
//! ```ron
//! Look(
//!     version: 1,
//!     id: "ville-qui-monte",
//!     name: "La ville qui monte",
//!     world: "neon",
//!     layers: (background: 1.0, world: 1.0, foreground: 0.3, overlay: 1.0),
//!     palette: Inherit,
//!     hue_shift_deg: 0.0,
//!     gates: ( max_floor: 4, /* … tous les champs de ShowGates … */ ),
//!     macros: [ Macro(name: "Vitesse", drives: [("gates.cruise", 0.9, 1.2)]) ],
//! )
//! ```

pub mod cue;
pub mod event;
pub mod ids;
pub mod look;
pub mod overrides;
pub mod params;

pub use cue::{Cue, CueAdvance, CueEntry, Trigger};
pub use event::{
    AudioProfile, DjIdentity, DjSlot, Event, EventSettings, FlashPolicy, MonitorSelect, Schedule, SlotMode,
    TempoPreference,
};
pub use cf_analysis::TempoSource;
pub use ids::{
    AssetPath, ClockTime, ControlProfileId, CueId, EventName, HexColor, InvalidValue, LookId, ShotId, ShotTier, SlotId,
    StuntKind, StuntTier, WorldId,
};
pub use look::{CustomTheme, CutGrid, FxGates, LayerMix, Look, PaletteRef, PresetTheme, ShowGates, Theme};
pub use overrides::{FxOverrides, GateOverrides, LookOverrides};
pub use params::{Macro, MacroDrive, ParamPath, UnknownParam};

/// Version courante du format des fichiers d'événement et de look (champ `version` en tête de chaque fichier).
///
/// À incrémenter à chaque changement incompatible du format, avec une migration depuis la version précédente
/// (voir `load::version`).
pub const FORMAT_VERSION: u32 = 1;
