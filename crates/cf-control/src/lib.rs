//! Contrôles de cyber-funk, sans dépendance à Bevy.
//!
//! Toute entrée (clavier, notes et CC MIDI, MIDI Clock, Ableton Link, OSC) est traduite en [`Action`] par une table de
//! correspondance chargée depuis des profils RON. Le reste de l'application ne connaît que les `Action`, définies
//! ici et consommées par `cf-show` (voir skill `midi-control` et `docs/ARCHITECTURE.md` §2.4).
//!
//! Le chemin d'une entrée :
//!
//! ```text
//! pilote (pont clavier Bevy, thread MIDI, serveur OSC)
//!   └─▶ RawInput { time_s, event }          entrée brute horodatée, sans allocation pour le MIDI
//!         └─▶ Mapper::map                   table effective = pile de ControlProfile fusionnés
//!               └─▶ TimedAction { time_s, action }   ─▶ cf-show
//! ```
//!
//! ```
//! use cf_control::{Action, ControlProfile, KeyEvent, Mapper, Modifiers, RawInput};
//!
//! let keyboard = ControlProfile::sillage_keyboard();
//! let (mut mapper, _overrides) = Mapper::from_profiles(&[&keyboard])?;
//! let drop = mapper.map(&RawInput::key(12.5, KeyEvent::char_press('d', Modifiers::default())));
//! assert_eq!(drop.map(|a| a.action), Some(Action::ForceDrop));
//! # Ok::<(), cf_control::Error>(())
//! ```

mod action;
mod defaults;
mod error;
mod keyboard;
mod mapper;
mod profile;
mod raw;
mod source;
mod template;

pub use action::{
    Action, ActionFamily, CueIndex, DjIndex, Layer, Level, MACRO_COUNT, MacroIndex, Switch, TitleId, TriggerId,
};
pub use defaults::SILLAGE_KEYBOARD_NAME;
pub use error::{Error, Result};
pub use keyboard::{Key, KeyCode, KeyEvent, Modifiers, ShiftRule};
pub use mapper::{Mapper, Override, TimedAction};
pub use profile::{Binding, ControlProfile, ProfileIssue};
pub use raw::{PortId, RawEvent, RawInput};
pub use source::{InputSource, MIDI_CHANNEL_MAX, MIDI_DATA_MAX, RelativeEncoding};
pub use template::{ActionKind, ActionTemplate, Mode, RELATIVE_LEVEL_STEP};
