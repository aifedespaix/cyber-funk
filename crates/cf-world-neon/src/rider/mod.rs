//! Les pilotes et les motos du trafic (X-10) : la teinte de carrosserie ([`tint`]), puis les motos et le conducteur.
//!
//! Les GLB (`assets/models/motos/moto-N-*.glb`, `characters/driver-car.glb`) ont le pilote figé dans leur géométrie
//! (le lecteur de GLB du jeu ne lit que des maillages statiques) ; voir
//! `docs/superpowers/specs/2026-11-01-motos-pilotes-design.md`.

pub mod driver;
pub mod moto;
pub mod spawn;
pub mod tint;

pub use tint::{RiderPaintMaterial, RiderTintExt, RiderTintPlugin, tinted, variant_color};
