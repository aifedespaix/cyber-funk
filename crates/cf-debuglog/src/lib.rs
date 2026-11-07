//! Journal structuré d'une session de cyber-funk, pensé pour être relu par un outil (ou un agent) sans tout charger.
//!
//! Tout est écrit dans `<racine>/session-<date>-<pid>/` :
//!
//! | Fichier          | Contenu                                                                                  |
//! | ---------------- | ---------------------------------------------------------------------------------------- |
//! | `session.json`   | Identité de la session. `status` vaut `running` tant qu'elle vit, `closed` à la fermeture propre : un `running` sur une session terminée **est un plantage**. |
//! | `heartbeat.json` | Dernier état connu (image, horloge, cue, audio, durée d'image), réécrit chaque seconde.   |
//! | `events.jsonl`   | Une ligne JSON par fait : horodatage, horloge de simulation, image, phase, nom, référence. |
//!
//! Aucune dépendance à Bevy (règle 5 du dépôt). L'écriture se fait sur un thread dédié : le rendu n'attend jamais le
//! disque, et un fichier non tamponné survit à un plantage brutal du processus.

mod buffer;
mod layer;
mod recorder;
mod time;

pub use buffer::{DEFAULT_CAPACITY, LogBuffer, LogFilter, LogLine, SessionHeader, format_for_agent};
pub use layer::RecorderLayer;
pub use recorder::{Guard, Level, Phase, Recorder, Span, StartOptions, relaunch_self};
pub use time::{compact_utc, epoch_ms, iso_utc};
