//! Le show : modèle de données et chef du spectacle, sans dépendance à Bevy.
//!
//! Événement → créneaux DJ → listes de cues → looks (fichiers RON). Le chef du spectacle fait avancer les cues,
//! prépare les drops, applique les portes du look courant et publie un `ShowState` en lecture seule pour tous les
//! systèmes, comme le `ShowDirector` de Sillage. Le limiteur photosensible (WCAG 2.3.1) et la sauvegarde continue
//! vivent aussi ici (voir skills `show-model` et `live-safety`).

pub mod freshness;
pub mod load;
pub mod model;
pub mod time_scale;
pub mod validate;
pub mod wait;
