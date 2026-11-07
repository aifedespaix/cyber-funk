//! La qualité des graffitis et des projecteurs. Ordre de retrait (spec, décision 12) : d'abord les petits projecteurs
//! lointains, puis le halo flou, puis les graffitis muraux lointains. Qualité minimale : seuls les projecteurs proches et les
//! graffitis proches restent. La scène reste complète : on retire du coût, jamais un morceau de la ville, et aucune entité
//! n'est détruite (les emplacements du pool sont seulement cachés).

use bevy::prelude::Resource;

use super::ProjectorSize;

/// Au-delà de cette distance à la voiture, un petit projecteur est retiré (niveau 1 et au-delà), en m.
pub const SMALL_FAR_M: f64 = 120.0;
/// Au-delà de cette distance, plus aucun projecteur au niveau minimal, en m.
pub const MIN_QUALITY_RANGE_M: f64 = 200.0;
/// Distance où les graffitis muraux ont disparu, en m, à pleine qualité.
pub const WALL_REACH_M: f32 = 140.0;
/// La même, quand les graffitis lointains sont retirés : seuls les murs proches restent peints.
pub const WALL_REACH_NEAR_M: f32 = 70.0;
/// Niveau de qualité (de `cf-render`) à partir duquel les graffitis muraux lointains sont retirés : le niveau minimal.
const WALL_LEVEL: usize = 3;
/// Niveau à partir duquel le halo flou est retiré.
const HALO_LEVEL: usize = 2;
/// Niveau à partir duquel les petits projecteurs lointains sont retirés.
const HOLO_LEVEL: usize = 1;

/// Ce que le niveau de qualité garde des graffitis et des projecteurs. Écrit par `apply_quality` seul.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraffitiQuality {
    /// Les petits projecteurs lointains sont gardés.
    pub holo_far: bool,
    /// Le halo flou autour des lettres est dessiné.
    pub halo: bool,
    /// Les graffitis muraux lointains sont peints.
    pub wall_far: bool,
}

impl Default for GraffitiQuality {
    fn default() -> Self {
        Self::for_level(0)
    }
}

impl GraffitiQuality {
    /// Ce que garde le niveau `level` (0 : tout ; plus haut : moins).
    #[must_use]
    pub const fn for_level(level: usize) -> Self {
        Self {
            holo_far: level < HOLO_LEVEL,
            halo: level < HALO_LEVEL,
            wall_far: level < WALL_LEVEL,
        }
    }

    /// Niveau minimal (les graffitis lointains sont retirés en dernier) : seuls les projecteurs proches restent, quelle que
    /// soit leur taille.
    #[must_use]
    pub const fn minimal(&self) -> bool {
        !self.wall_far
    }

    /// Un projecteur de classe `size` à `distance_m` de la voiture est-il affiché ?
    #[must_use]
    pub fn keeps_projector(&self, size: ProjectorSize, distance_m: f64) -> bool {
        if self.minimal() {
            return distance_m <= MIN_QUALITY_RANGE_M;
        }
        self.holo_far || size != ProjectorSize::Small || distance_m <= SMALL_FAR_M
    }

    /// Distance où les graffitis muraux ont disparu, en m.
    #[must_use]
    pub fn wall_reach_m(&self) -> f32 {
        if self.wall_far { WALL_REACH_M } else { WALL_REACH_NEAR_M }
    }
}
