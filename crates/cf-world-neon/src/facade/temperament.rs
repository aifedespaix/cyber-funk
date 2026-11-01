//! Le tempérament d'un immeuble : la manière dont sa façade s'allume (teintes, taux d'allumage, forme des lumières,
//! mouvement). Huit tempéraments dans une table ; chaque immeuble en reçoit un, tiré de sa rangée et de sa graine, et
//! n'en transporte que l'indice (bits 24..26 du tag, voir [`crate::scenery::Part::tag`]).
//!
//! L'ambiance par défaut est calme et dense : la majorité des immeubles sont « calmes » (sans étincelles, mouvement
//! stable ou lent) et les rafales se concentrent sur une minorité.

use crate::rng::hash;

/// Nombre de tempéraments (3 bits dans le tag).
pub const TEMPER_COUNT: usize = 8;

/// Forme des lumières d'une façade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightShape {
    /// Fenêtres une à une (rendu actuel).
    Panes,
    /// Bandes horizontales.
    Bands,
    /// Colonnes verticales.
    Columns,
    /// Enseigne.
    Sign,
    /// Écran.
    Screen,
    /// Teinte d'étage.
    FloorTint,
}

/// Mouvement des lumières d'une façade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotionKind {
    /// Immobile.
    Steady,
    /// Vague montante.
    WaveUp,
    /// Vague descendante.
    WaveDown,
    /// Dérive lente.
    Drift,
    /// Balayage.
    Sweep,
    /// Pulsation.
    Pulse,
}

/// Un tempérament.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Temperament {
    /// Étendue des teintes autour de celle de l'immeuble, en degrés (0..40).
    pub hue_span_deg: f32,
    /// Décalage de la part de fenêtres allumées (−0,15..+0,2).
    pub lit_bias: f32,
    /// Part de fenêtres ambre.
    pub amber_share: f32,
    /// Part de fenêtres glace.
    pub ice_share: f32,
    /// Forme des lumières.
    pub shape: LightShape,
    /// Mouvement des lumières.
    pub motion: MotionKind,
    /// Taux d'étincelles (0 : aucune).
    pub spark_rate: f32,
}

const fn temper(
    hue_span_deg: f32,
    lit_bias: f32,
    amber_share: f32,
    ice_share: f32,
    shape: LightShape,
    motion: MotionKind,
    spark_rate: f32,
) -> Temperament {
    Temperament {
        hue_span_deg,
        lit_bias,
        amber_share,
        ice_share,
        shape,
        motion,
        spark_rate,
    }
}

/// La table. L'indice 0 (« bureau stable ») reproduit le rendu d'avant les tempéraments : aucune dispersion de teinte,
/// pas de biais, 30 % d'ambre, 10 % de glace.
pub const TEMPERAMENTS: [Temperament; TEMPER_COUNT] = [
    temper(0.0, 0.0, 0.3, 0.1, LightShape::Panes, MotionKind::Steady, 0.0),
    temper(12.0, 0.05, 0.25, 0.15, LightShape::Panes, MotionKind::Drift, 0.0),
    temper(20.0, 0.1, 0.2, 0.2, LightShape::Bands, MotionKind::Steady, 0.0),
    temper(8.0, -0.1, 0.35, 0.1, LightShape::Columns, MotionKind::Drift, 0.0),
    temper(25.0, 0.0, 0.2, 0.2, LightShape::FloorTint, MotionKind::WaveUp, 0.1),
    temper(40.0, 0.2, 0.1, 0.3, LightShape::Sign, MotionKind::Pulse, 0.3),
    temper(35.0, 0.15, 0.15, 0.25, LightShape::Screen, MotionKind::Sweep, 0.25),
    temper(30.0, -0.15, 0.3, 0.15, LightShape::Panes, MotionKind::WaveDown, 0.2),
];

/// Bornes cumulées du tirage : 40 % de bureau stable, 32 % d'autres calmes (1 à 3), 28 % de rafales (4 à 7).
const CUMULATIVE: [f64; TEMPER_COUNT] = [0.40, 0.52, 0.62, 0.72, 0.80, 0.86, 0.92, 1.0];

/// L'indice de tempérament (`0..TEMPER_COUNT`) d'un immeuble : fonction pure de sa rangée et de sa graine.
#[must_use]
pub fn temper_of(row_index: i64, seed: u16) -> u8 {
    let t = hash(row_index as f64 * 7.31 + f64::from(seed) * 0.37, 91.0);
    let index = CUMULATIVE.iter().position(|&bound| t < bound).unwrap_or(0);
    index as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temper_is_deterministic() {
        let first = temper_of(1234, 77);
        for _ in 0..10_000 {
            assert_eq!(temper_of(1234, 77), first);
        }
    }

    #[test]
    fn temper_stays_in_range() {
        for row in -500..500 {
            assert!((temper_of(row, 3) as usize) < TEMPER_COUNT);
        }
    }

    #[test]
    fn neighbours_differ() {
        let rows: Vec<u8> = (0..200).map(|r| temper_of(r, 0)).collect();
        let mut seen = [false; TEMPER_COUNT];
        for &t in &rows {
            seen[t as usize] = true;
        }
        assert!(seen.iter().filter(|&&s| s).count() >= 5, "au moins 5 tempéraments");
        let same = rows.windows(2).filter(|w| w[0] == w[1]).count();
        assert!(
            same * 3 < rows.len() - 1,
            "{same} voisins identiques sur {}",
            rows.len() - 1
        );
    }

    #[test]
    fn distribution_is_calm_by_default() {
        let calm = (0..10_000)
            .filter(|&r| {
                let t = &TEMPERAMENTS[temper_of(r, 0) as usize];
                t.spark_rate == 0.0 && matches!(t.motion, MotionKind::Steady | MotionKind::Drift)
            })
            .count();
        assert!(calm >= 6_000, "{calm} rangées calmes sur 10 000");
    }

    #[test]
    fn tag_keeps_legacy_fields() {
        use crate::scenery::{Row, SceneryConfig};
        let config = SceneryConfig::default();
        let mut checked = 0;
        for index in 0..300 {
            for part in Row::generate(index, &config).parts() {
                let legacy = u32::from(part.seed) & 0x3FF
                    | (u32::from(part.tint) & 0xF) << 10
                    | (part.tier as u32) << 14
                    | u32::from(part.spot_floor) << 16;
                assert_eq!(part.tag() & 0xFF_FFFF, legacy);
                assert_eq!(part.tag() >> 24, u32::from(part.temper));
                assert!(usize::from(part.temper) < TEMPER_COUNT);
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn temperament_zero_is_the_legacy_look() {
        let t = TEMPERAMENTS[0];
        assert_eq!((t.hue_span_deg, t.lit_bias, t.spark_rate), (0.0, 0.0, 0.0));
        assert_eq!((t.amber_share, t.ice_share), (0.3, 0.1));
        assert_eq!((t.shape, t.motion), (LightShape::Panes, MotionKind::Steady));
    }
}
