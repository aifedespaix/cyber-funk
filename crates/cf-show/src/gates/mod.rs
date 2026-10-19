//! Les portes de l'image : fondu entre deux looks (`GateBlender` de Sillage, `engine/show/gates.ts`) et variations de
//! l'après-set (`Variation`).
//!
//! Un look fixe ce que le réalisateur a le droit de faire ; entre deux looks, les nombres glissent (jamais de saut)
//! et les listes basculent d'un coup (ou à mi-fondu). Les six niveaux de Sillage sont les looks
//! `assets/looks/niveau-*.ron` ; la montée automatique est `assets/shows/montee-auto.ron`.

mod blender;
mod variation;

pub use blender::{Blend, GateBlender, ListSource, SMOOTH_SECONDS, open_gates};
pub use variation::Variation;
