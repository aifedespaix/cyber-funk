//! Qualité adaptative (J4-06) : portage de `engine/quality.ts` de Sillage, avec la mesure GPU en plus du CPU.
//!
//! # Le principe
//!
//! [`Quality`] (la logique pure, [`logic`]) regarde la durée de chaque image et, quand le pilote les donne, le temps
//! GPU des passes ([`DiagnosticsSnapshot::gpu_total_ms`](crate::DiagnosticsSnapshot)). Sous ~30 i/s, elle descend d'un
//! cran ; elle remonte d'elle-même quand le cran du dessus est estimé tenable, avec une patience qui double à chaque
//! rechute. Chaque niveau règle les leviers de [`levers_for`] :
//!
//! | Niveau | Échelle de rendu | Bloom             | Particules       | Rendus annexes |
//! | ------ | ---------------- | ----------------- | ---------------- | -------------- |
//! | 0      | 1,0              | 1/2 de la hauteur | pleines          | à chaque image |
//! | 1      | 0,9              | 1/4               | pleines          | à chaque image |
//! | 2      | 0,8              | éteint            | pleines          | 1 image sur 2  |
//! | 3      | 0,7              | éteint            | fumée 0,4 pluie 0,5 | 1 image sur 3 |
//!
//! # Les leviers dans le rendu
//!
//! - **Échelle de rendu** : [`RenderScale`](crate::RenderScale). Les textures des couches 3D sont allouées à cette
//!   fraction de la fenêtre (`layers::rig`) ; le compositeur les étire en filtrage linéaire. La surcouche 2D (titres)
//!   reste à pleine résolution.
//! - **Bloom** : [`PostFxSettings`](crate::PostFxSettings) (diviseur, présence, gain).
//! - **Particules et rendus annexes** : [`QualityLevers`], lue par les mondes.
//!
//! # Règles de sécurité
//!
//! - **Règle 8** : aucun levier ne crée de pipeline. Une taille de texture, la taille du premier niveau du bloom
//!   (`max_mip_dimension`) et des densités ne figurent pas dans les clés de spécialisation de Bevy ; le bloom retiré
//!   puis remis reprend les mêmes pipelines, déjà compilés par la préchauffe. Le détecteur de J4-07 reste muet
//!   (vérifié sous Xvfb, voir la PR).
//! - **Règle 7** : aucun levier n'écrit de lumière. Le seul changement d'éclairement est le bloom (additif, 0,3 de
//!   force) : il est **estompé** sur [`BLOOM_FADE_S`] avant d'être retiré, et rétabli de même, pour ne jamais faire un
//!   saut de luminance. Les flashs, eux, ne passent toujours que par le limiteur de `cf-show`.

mod levers;
mod logic;
mod systems;

use bevy::prelude::*;

pub use levers::{QualityLevers, levers_for};
pub use logic::{MAX_LEVEL, PIXEL_SCALE, Quality};
pub use systems::{BLOOM_FADE_S, QualityChanged, QualityIdle, QualityMode};

use crate::{CfSet, PostFxSettings, RenderScale};

/// Branche la qualité adaptative : la logique, les leviers, et le système qui les applique.
///
/// Fonctionne avec ou sans [`DiagnosticsPlugin`](crate::DiagnosticsPlugin) : sans lui, ni mesure GPU ni
/// instantané, la durée d'image seule décide. À ajouter avec le post-process et les couches (il écrit
/// [`RenderScale`] et [`PostFxSettings`]).
#[derive(Debug, Default, Clone, Copy)]
pub struct QualityPlugin;

impl Plugin for QualityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Quality>()
            .init_resource::<QualityLevers>()
            .init_resource::<QualityMode>()
            .init_resource::<QualityIdle>()
            .init_resource::<RenderScale>()
            .init_resource::<PostFxSettings>()
            .add_message::<QualityChanged>()
            .add_systems(Update, systems::update_quality.in_set(CfSet::Present));
    }
}

#[cfg(test)]
mod tests;
