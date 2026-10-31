//! Les cartons-titres sur la surcouche 2D (J7-11) : niveau, DJ à la passation, titres de drop.
//!
//! ```text
//! ShowEventMessage::Title(texte)          ─▶ carton « drop »
//! ShowEventMessage::Handover / ShowDj     ─▶ carton « DJ »  (nom lu dans ShowSheet, « Aux platines »)
//! CurrentShow.level qui change (> 0)      ─▶ carton « niveau »  (« Niveau N », nom du niveau)
//! ```
//!
//! Le niveau 0 n'a pas de carton : « le monde se construit sans annonce » (Sillage). Un nouveau carton remplace
//! l'ancien. Les cartons sont de simples `Text2d` de la couche [`OverlayLayer`](crate::OverlayLayer) : ni bloom, ni
//! étalonnage, ni flash ne les touchent. Leurs entrées et sorties sont des fondus d'au moins un demi-temps (jamais une
//! variation de luminance brusque, règle 7 de `CLAUDE.md`).
//!
//! Écarts avec Sillage : pas d'espacement des lettres (non géré par `TextFont`), pas de halo lumineux sur le texte ; le
//! voile sombre derrière le texte est un dégradé radial cuit une fois à l'ouverture.

mod card;
mod plugin;

pub use card::{Look, Timing, TitleCard, TitleKind, beat_seconds, font_sizes, look_at};
pub use plugin::{ActiveTitle, TitleSettings, TitlesPlugin};
