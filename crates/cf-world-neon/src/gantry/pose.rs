//! La géométrie pure du portique : sa montée et sa descente, la taille du nom, les pièces métalliques. Aucune
//! dépendance à Bevy ni au GPU : testable seule.

use bevy::math::Vec3;

use crate::scenery::BASE_DEPTH_M;

/// Largeur de la travée entre les pieds (m) : la route et un peu plus.
pub const SPAN_M: f32 = 34.0;
/// Hauteur du dessous de la poutre au-dessus de la route (m).
pub const CLEARANCE_M: f32 = 9.0;
/// Épaisseur de la poutre (m).
pub const BEAM_H_M: f32 = 6.0;
/// Largeur d'un pied (m).
pub const PYLON_W_M: f32 = 1.8;
/// Distance devant la voiture à laquelle le portique se pose (m).
pub const AHEAD_M: f64 = 160.0;
/// Hauteur des lettres, en part de la poutre (le nom occupe le panneau, pas la poutre entière).
pub const NAME_H_M: f32 = 3.4;
/// Marge de chaque côté du nom sur la poutre (m).
const NAME_MARGIN_M: f32 = 3.0;

/// Part de la montée de l'étape `gantry` (0 absent, 1 replié) : le portique sort de la route en tout début, reste en
/// place, puis s'y enfonce à la fin. 0 = entièrement enterré, 1 = entièrement sorti.
#[must_use]
pub fn rise(stage: f32) -> f32 {
    let s = stage.clamp(0.0, 1.0);
    let up = (s / 0.2).min(1.0);
    let down = ((1.0 - s) / 0.2).min(1.0);
    let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
    smooth(up.min(down))
}

/// Décalage vertical (m) du portique pour une montée `rise` : enterré (pieds, poutre et nom sous la route) à 0.
#[must_use]
pub fn buried_offset_m(rise: f32) -> f32 {
    -(1.0 - rise.clamp(0.0, 1.0)) * (CLEARANCE_M + BEAM_H_M + BASE_DEPTH_M + 1.0)
}

/// Facteur d'échelle du nom pour qu'il tienne sur la poutre : 1 s'il tient, plus petit sinon. Un nom vide ou une largeur
/// non valable donne 1.
#[must_use]
pub fn name_fit(name_width_m: f32) -> f32 {
    let room = SPAN_M - 2.0 * NAME_MARGIN_M;
    if name_width_m.is_finite() && name_width_m > room {
        room / name_width_m
    } else {
        1.0
    }
}

/// Une pièce métallique : centre et dimensions, dans le repère du portique (origine sur la route, au milieu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    /// Centre.
    pub center: Vec3,
    /// Dimensions complètes.
    pub size: Vec3,
}

/// Les trois pièces : pied gauche, pied droit, poutre. Les pieds descendent jusqu'à `-BASE_DEPTH_M`.
#[must_use]
pub fn pieces() -> [Piece; 3] {
    let top = CLEARANCE_M + BEAM_H_M;
    let height = top + BASE_DEPTH_M;
    let x = SPAN_M / 2.0 + PYLON_W_M / 2.0;
    let pylon = |side: f32| Piece {
        center: Vec3::new(side * x, top - height / 2.0, 0.0),
        size: Vec3::new(PYLON_W_M, height, PYLON_W_M),
    };
    [
        pylon(-1.0),
        pylon(1.0),
        Piece {
            center: Vec3::new(0.0, CLEARANCE_M + BEAM_H_M / 2.0, 0.0),
            size: Vec3::new(SPAN_M + 2.0 * PYLON_W_M, BEAM_H_M, PYLON_W_M),
        },
    ]
}

/// Décalage horizontal (m) et inclinaison (rad) du glitch d'une lettre. Géométrique : jamais de variation de lumière, donc
/// rien à passer par le limiteur. Quantifié à 8 Hz, nul sans glitch.
#[must_use]
pub fn glitch_offset(glitch: f32, letter: usize, time_s: f64) -> (f32, f32) {
    if glitch <= 0.0 {
        return (0.0, 0.0);
    }
    let step = (time_s * 8.0).floor() as i64;
    let h = (i64::from(letter as u32) * 7919 + step * 104_729).rem_euclid(1000) as f32 / 1000.0 - 0.5;
    (h * 1.6 * glitch.clamp(0.0, 1.0), h * 0.25 * glitch.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rise_is_zero_when_absent_or_folded_and_one_in_between() {
        assert_eq!(rise(0.0), 0.0);
        assert_eq!(rise(1.0), 0.0);
        assert!((rise(0.5) - 1.0).abs() < 1e-6);
        assert!((0..=100).all(|k| (0.0..=1.0).contains(&rise(k as f32 / 100.0))));
    }

    #[test]
    fn a_buried_gantry_is_entirely_below_the_road() {
        let lowest_top = pieces()
            .iter()
            .map(|p| p.center.y + p.size.y / 2.0 + buried_offset_m(0.0))
            .fold(f32::MIN, f32::max);
        assert!(lowest_top < -BASE_DEPTH_M * 0.0 - 0.5, "{lowest_top}");
        assert_eq!(buried_offset_m(1.0), 0.0);
    }

    #[test]
    fn the_pylons_go_down_to_the_buried_base_and_never_above_the_road_at_their_foot() {
        for p in &pieces()[..2] {
            let foot = p.center.y - p.size.y / 2.0;
            assert!((foot + BASE_DEPTH_M).abs() < 1e-4, "{foot}");
        }
    }

    #[test]
    fn the_beam_clears_the_road_by_the_wanted_height_and_spans_it() {
        let beam = pieces()[2];
        assert!((beam.center.y - beam.size.y / 2.0 - CLEARANCE_M).abs() < 1e-5);
        assert!(beam.size.x > SPAN_M);
    }

    #[test]
    fn a_long_name_is_shrunk_to_fit_and_an_empty_or_bad_one_is_left_alone() {
        assert_eq!(name_fit(10.0), 1.0);
        let wide = name_fit(80.0);
        assert!(80.0 * wide <= SPAN_M - 2.0 * NAME_MARGIN_M + 1e-4 && wide < 1.0);
        assert_eq!(name_fit(0.0), 1.0);
        assert_eq!(name_fit(f32::NAN), 1.0);
    }

    #[test]
    fn the_glitch_is_nil_without_intensity_and_bounded_otherwise() {
        assert_eq!(glitch_offset(0.0, 3, 1.0), (0.0, 0.0));
        for letter in 0..20 {
            for k in 0..50 {
                let (dx, tilt) = glitch_offset(1.0, letter, f64::from(k) * 0.05);
                assert!(dx.abs() <= 0.8 + 1e-4 && tilt.abs() <= 0.125 + 1e-4);
            }
        }
    }

    #[test]
    fn the_glitch_is_quantised_at_8_hz() {
        assert_eq!(glitch_offset(1.0, 2, 1.01), glitch_offset(1.0, 2, 1.10));
    }
}
