//! Sauvegarde continue et reprise (`engine/show/persist.ts` de Sillage), plus l'ouverture et l'enregistrement des
//! fichiers d'événement (`.cfshow`).
//!
//! Deux fichiers, deux rôles :
//! * l'**état courant** (créneau, cue, temps écoulé, macros, graine, DJ armé) : écrit toutes les [`SAVE_EVERY_S`]
//!   secondes, atomiquement, relu au démarrage s'il a moins de [`RESUME_WINDOW_S`] (skill `live-safety`) ;
//! * l'**événement** : enregistré atomiquement après une copie de secours (`.bak`) de la version précédente.
//!
//! Tout le temps est injecté (`wall_now_s`, secondes Unix) : rien n'est lu sur une horloge ici. Les fonctions qui
//! écrivent touchent le disque : à appeler hors du fil d'image (la capture [`ShowSave::capture`], elle, est pure).

mod document;
mod error;
mod io;
mod restore;
mod save;

pub use document::EventDocument;
pub use error::PersistError;
pub use io::{backup_path, clear_save, read_save, write_atomic, write_save};
pub use save::{RESUME_WINDOW_S, SAVE_EVERY_S, SAVE_VERSION, SavePacer, SavedKnob, ShowSave};
