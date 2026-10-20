//! Entrées et sorties audio de cyber-funk, sans dépendance à Bevy.
//!
//! - Capture d'un périphérique (micro, entrée ligne d'une carte son) avec `cpal`.
//! - Lecture des pistes de démo (`assets/music/`, `assets/music/local/`) décodées par `symphonia`.
//! - Transport sans verrou ni allocation du callback temps réel vers le thread d'analyse (`rtrb`).
//!
//! Règle absolue : rien dans un callback `cpal` n'alloue, ne verrouille, ne journalise ni ne fait d'I/O
//! (voir skill `realtime-audio`).

pub mod capture;
pub mod feed;
pub mod player;
pub mod playlist;
pub mod source;
