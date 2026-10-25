//! L'onde de choc du drop (`Ring` de `fx.ts`) et les constantes de réglage du paquet, reprises de Sillage (écarts : voir
//! `logic`).

use bevy::math::DVec3;

use super::safety::ATTACK_S;

/// Ondes de choc simultanées (`RINGS`).
pub const RINGS: usize = 6;
/// Ondes d'un même drop au plus.
pub const DROP_RINGS_MAX: usize = 3;
/// Écart entre les ondes d'un même drop (s).
pub const DROP_RING_GAP_S: f32 = 0.34;
/// Vie d'une onde (s).
pub const RING_LIFE_S: f32 = 1.1;
/// Vie d'une particule (s).
pub const PARTICLE_LIFE_S: f32 = 1.4;
/// Particules vivantes au plus (anneau : la plus ancienne cède la place).
pub const PARTICLES: usize = 240;
/// Hauteur de l'onde au sol (m).
pub const RING_Y_M: f64 = 0.06;
/// Hauteur de départ de la gerbe au-dessus du point du drop (m).
pub const BURST_Y_M: f64 = 0.5;
/// Les ondes grandissent de 1 à `1 + RING_GROWTH` fois leur taille de départ (`1 + t × 16`).
pub const RING_GROWTH: f32 = 16.0;
/// Teintes du cycle prismatique.
pub const CYCLE_LEN: usize = 6;

/// Une onde de choc.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ring {
    /// Âge (s) ; libre à partir de [`RING_LIFE_S`].
    pub age_s: f32,
    /// Centre (absolu, m).
    pub at: DVec3,
    /// Rang de la teinte dans le cycle prismatique.
    pub tint: usize,
    /// Le limiteur a accordé le départ de cette onde.
    pub granted: bool,
}

impl Ring {
    pub(super) const FREE: Self = Self {
        age_s: RING_LIFE_S,
        at: DVec3::ZERO,
        tint: 0,
        granted: false,
    };

    /// L'onde est éteinte.
    #[must_use]
    pub fn is_free(&self) -> bool {
        self.age_s >= RING_LIFE_S
    }

    /// Avancement, 0 → 1.
    #[must_use]
    pub fn t(&self) -> f32 {
        (self.age_s / RING_LIFE_S).min(1.0)
    }

    /// Facteur d'échelle de l'anneau : `1 + t × 16` (`ring.mesh.scale`).
    #[must_use]
    pub fn scale(&self) -> f32 {
        1.0 + self.t() * RING_GROWTH
    }

    /// Éclat relatif, 0 → 1 : montée en [`ATTACK_S`] puis `(1 − t)²` (l'opacité de Sillage). À multiplier par le gain.
    #[must_use]
    pub fn level(&self) -> f32 {
        let t = self.t();
        (self.age_s / ATTACK_S).clamp(0.0, 1.0) * (1.0 - t) * (1.0 - t)
    }
}
