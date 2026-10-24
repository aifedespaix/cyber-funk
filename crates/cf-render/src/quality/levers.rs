//! Les leviers que chaque niveau de qualité règle. Une table, pas de logique : [`levers_for`].
//!
//! Sillage, `applyQuality` : échelle de rendu `PIXEL_SCALE[niveau]`, bloom éteint dès le niveau 2, diviseur du bloom 4
//! dès le niveau 1, fumée à 0,4 et pluie à 0,5 au dernier niveau, rendus annexes une image sur deux au niveau 2 et
//! sur trois au niveau 3 (`sideRenderEvery`).
//!
//! Aucun levier n'écrit de lumière : ni flash, ni étalonnage, ni contraste. Les règles 7 (limiteur) et 8 (aucun
//! pipeline en show) tiennent donc par construction : seuls changent une taille de texture, la taille du premier
//! niveau du bloom, la présence du bloom (estompé avant d'être retiré) et des densités de particules.

use bevy::prelude::Resource;

use super::logic::{MAX_LEVEL, PIXEL_SCALE};

/// Les réglages dérivés du niveau de qualité. Écrite par le seul système de la qualité ; lue par les mondes (densité
/// des particules, cadence des rendus annexes).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct QualityLevers {
    /// Niveau, 0 = plein régime.
    pub level: usize,
    /// Échelle de la résolution de rendu des couches 3D (1 = plein). La surcouche 2D reste à pleine résolution.
    pub render_scale: f32,
    /// Bloom voulu. Il s'estompe avant d'être retiré (voir `BLOOM_FADE_S`).
    pub bloom: bool,
    /// Diviseur de résolution du bloom : plus grand, moins cher et plus flou.
    pub bloom_divisor: u32,
    /// Densité de la fumée et des étincelles, 0 → 1 (à multiplier au nombre d'émissions).
    pub particle_density: f32,
    /// Densité de la pluie, 0 → 1.
    pub rain_density: f32,
    /// Les rendus annexes (rétroviseur, caméras de panneaux) se rafraîchissent une image sur N ; la texture garde la
    /// dernière image entre-temps.
    pub side_render_every: u32,
}

impl Default for QualityLevers {
    fn default() -> Self {
        levers_for(0)
    }
}

impl QualityLevers {
    /// C'est le tour des rendus annexes à l'image `frame` ?
    #[must_use]
    pub fn side_turn(&self, frame: u64) -> bool {
        frame.is_multiple_of(u64::from(self.side_render_every.max(1)))
    }
}

/// Les leviers du niveau `level` (borné à [`MAX_LEVEL`]).
#[must_use]
pub fn levers_for(level: usize) -> QualityLevers {
    let level = level.min(MAX_LEVEL);
    QualityLevers {
        level,
        render_scale: PIXEL_SCALE[level],
        bloom: level < 2,
        bloom_divisor: if level >= 1 { 4 } else { 2 },
        particle_density: if level >= MAX_LEVEL { 0.4 } else { 1.0 },
        rain_density: if level >= MAX_LEVEL { 0.5 } else { 1.0 },
        side_render_every: match level {
            3.. => 3,
            2 => 2,
            _ => 1,
        },
    }
}
