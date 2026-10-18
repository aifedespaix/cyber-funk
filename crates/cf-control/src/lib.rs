//! Contrôles de cyber-funk, sans dépendance à Bevy.
//!
//! Toute entrée (clavier, notes et CC MIDI, MIDI Clock, Ableton Link, OSC) est traduite en `Action` par une table de
//! correspondance chargée depuis des profils RON. Le reste de l'application ne connaît que les `Action`, définies
//! ici et consommées par `cf-show` (voir skill `midi-control`).
