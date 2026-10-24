//! Les structures que la voiture esquive en vol (`flight/obstacles.rs`, `flight/floors.ts`) : tranches d'altitude
//! interdites, part d'esquive selon la distance, choix du côté (par dessus ou par dessous).

use crate::route::bridges::{CELL, bridge_span, bridge_z};

/// Étendue verticale de la voiture sous son altitude : sous la coque (patins compris), en m.
pub const CAR_BELOW: f64 = 0.7;
/// Étendue verticale de la voiture au-dessus de son altitude : au-dessus du toit, en m.
pub const CAR_ABOVE: f64 = 1.9;
/// Marge de sécurité visuelle entre la voiture et ce qu'elle esquive, en m.
pub const CLEARANCE: f64 = 1.0;

/// Une tranche d'altitude occupée par quelque chose (véhicule, nuée), en m.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    /// Bas de la tranche.
    pub lo: f64,
    /// Haut de la tranche.
    pub hi: f64,
}

/// Vrai si la voiture, à l'altitude `altitude`, partagerait de la place avec la tranche `band` (marge `extra` en plus).
#[must_use]
pub fn overlaps(altitude: f64, band: Band, extra: f64) -> bool {
    altitude + CAR_ABOVE + extra > band.lo && altitude - CAR_BELOW - extra < band.hi
}

/// Une structure qui enjambe la route : elle occupe, sur toute la largeur de la chaussée, la tranche d'altitude
/// `lo`–`hi` entre `z_far` et `z_near` (z monde ; la voiture avance vers les z négatifs, donc elle atteint `z_near` en
/// premier). Ce n'est pas un objet de collision : on s'en sert uniquement pour décider quelle esquive jouer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Obstruction {
    /// Bord proche (atteint en premier), z.
    pub z_near: f64,
    /// Bord lointain, z.
    pub z_far: f64,
    /// Dessous, m.
    pub lo: f64,
    /// Dessus, m.
    pub hi: f64,
}

/// Anticipation : à quelle distance de l'obstacle commence l'esquive (selon la vitesse), en m.
#[must_use]
pub fn ramp_in(speed: f64) -> f64 {
    (speed * 1.4).clamp(50.0, 150.0)
}

/// Distance après l'obstacle sur laquelle l'esquive se relâche, en m.
pub const RAMP_OUT: f64 = 26.0;
/// Altitude minimale en passant dessous : la voiture ne rase pas la chaussée, en m.
pub const MIN_UNDER: f64 = 2.2;
/// Altitude maximale, en m.
pub const MAX_ALTITUDE: f64 = 62.0;

/// Les immeubles-arches proches de `car_z`, déterministes : ils ne dépendent d'aucun événement. Ajoute à `out`.
pub fn bridge_obstructions(car_z: f64, out: &mut Vec<Obstruction>) {
    let cell = ((-car_z / CELL).floor() as i64).max(0);
    let span = bridge_span();
    for c in (cell - 1).max(0)..=cell + 1 {
        let z = bridge_z(c);
        if z > car_z + 60.0 || z < car_z - 700.0 {
            continue;
        }
        out.push(Obstruction {
            z_near: z + span.half_depth,
            z_far: z - span.half_depth,
            lo: span.lo,
            hi: span.hi,
        });
    }
}

/// Part de l'esquive à appliquer, de 0 (rien) à 1 (plein), selon la distance à l'obstacle.
#[must_use]
pub fn weight(o: &Obstruction, car_z: f64, speed: f64) -> f64 {
    let ahead = car_z - o.z_near;
    if ahead > 0.0 {
        let ramp = ramp_in(speed);
        if ahead >= ramp {
            return 0.0;
        }
        let t = 1.0 - ahead / ramp;
        return t * t * (3.0 - 2.0 * t);
    }
    if car_z >= o.z_far {
        return 1.0;
    }
    let gone = o.z_far - car_z;
    if gone >= RAMP_OUT {
        return 0.0;
    }
    let t = gone / RAMP_OUT;
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// Part d'esquive minimale pour qu'une structure compte dans le calcul : en dessous, elle est encore trop loin.
const MIN_WEIGHT: f64 = 0.02;
/// Structures prises en compte en même temps (arches et trafic : bien moins en pratique).
const MAX_BANDS: usize = 16;

/// Côté de l'esquive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DodgeSide {
    /// Pas d'esquive.
    #[default]
    None,
    /// Par dessous.
    Under,
    /// Par dessus.
    Over,
}

/// Résultat de [`resolve_dodge`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Dodge {
    /// Décalage à ajouter à l'altitude de l'étage (m), déjà pondéré par la proximité.
    pub offset: f64,
    /// Côté de l'esquive.
    pub side: DodgeSide,
    /// Une esquive est en cours.
    pub busy: bool,
}

/// L'esquive de toutes les structures à la fois. Chacune interdit une tranche d'altitude ; celles qui se recouvrent
/// forment un seul bloc, qu'on contourne d'un bloc (sinon passer au-dessus de l'une pourrait mener dans l'autre). Seul le
/// bloc qui contient l'altitude de départ compte, et sa part d'esquive est celle de sa structure la plus proche.
#[must_use]
pub fn resolve_dodge(obstructions: &[Obstruction], base: f64, car_z: f64, speed: f64) -> Dodge {
    let mut band_lo = [0.0_f64; MAX_BANDS];
    let mut band_hi = [0.0_f64; MAX_BANDS];
    let mut band_w = [0.0_f64; MAX_BANDS];
    let mut n = 0;
    for o in obstructions {
        let w = weight(o, car_z, speed);
        if w < MIN_WEIGHT || n == MAX_BANDS {
            continue;
        }
        band_lo[n] = o.lo - CAR_ABOVE - CLEARANCE;
        band_hi[n] = o.hi + CAR_BELOW + CLEARANCE;
        band_w[n] = w;
        n += 1;
    }
    // Le bloc de départ : les tranches qui contiennent `base`, puis tout ce qui les recouvre, de proche en proche.
    let (mut lo, mut hi, mut w) = (f64::INFINITY, f64::NEG_INFINITY, 0.0_f64);
    for i in 0..n {
        if base <= band_lo[i] || base >= band_hi[i] {
            continue;
        }
        lo = lo.min(band_lo[i]);
        hi = hi.max(band_hi[i]);
        w = w.max(band_w[i]);
    }
    if w <= 0.0 {
        return Dodge::default();
    }
    let mut grown = true;
    while grown {
        grown = false;
        for i in 0..n {
            if band_lo[i] >= hi || band_hi[i] <= lo || (band_lo[i] >= lo && band_hi[i] <= hi) {
                continue;
            }
            lo = lo.min(band_lo[i]);
            hi = hi.max(band_hi[i]);
            w = w.max(band_w[i]);
            grown = true;
        }
    }
    let can_under = lo >= MIN_UNDER;
    let can_over = hi <= MAX_ALTITUDE;
    let (target, side) = if can_under && (!can_over || base - lo < hi - base) {
        (lo, DodgeSide::Under)
    } else if can_over {
        (hi, DodgeSide::Over)
    } else if can_under {
        (lo, DodgeSide::Under)
    } else {
        return Dodge::default();
    };
    Dodge {
        offset: (target - base) * w,
        side,
        busy: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::route::floor_altitude;

    fn deck() -> Obstruction {
        let span = bridge_span();
        Obstruction {
            z_near: -1000.0 + span.half_depth,
            z_far: -1000.0 - span.half_depth,
            lo: span.lo,
            hi: span.hi,
        }
    }

    #[test]
    fn la_part_d_esquive_monte_puis_retombe() {
        let o = deck();
        let at = |z: f64| weight(&o, z, 50.0);
        assert!(at(-800.0) < f64::EPSILON, "trop loin");
        assert!(at(-950.0) > 0.0 && at(-950.0) < 1.0, "en approche");
        assert!((at(-1000.0) - 1.0).abs() < f64::EPSILON, "sous le tablier");
        assert!(
            at(-1000.0 - 8.0 - 13.0) > 0.0 && at(-1000.0 - 8.0 - 13.0) < 1.0,
            "dégagement"
        );
        assert!(at(-1100.0) < f64::EPSILON, "passée");
    }

    #[test]
    fn seul_l_etage_cinq_heurte_le_tablier() {
        let o = [deck()];
        for floor in 1..=6 {
            let dodge = resolve_dodge(&o, floor_altitude(floor), -1000.0, 50.0);
            assert_eq!(dodge.busy, floor == 5, "étage {floor}");
        }
    }

    #[test]
    fn l_esquive_prend_le_chemin_le_plus_court() {
        let o = [deck()];
        let dodge = resolve_dodge(&o, floor_altitude(5), -1000.0, 50.0);
        // 29,5 m : le tablier va de 24 à 35 m, le haut est à 35 + 0,7 + 1 = 36,7 m (7,2 m) contre 24 − 2,9 = 21,1 m (8,4 m).
        assert_eq!(dodge.side, DodgeSide::Over);
        assert!(dodge.offset > 7.0 && dodge.offset < 7.4, "{}", dodge.offset);
    }

    #[test]
    fn les_arches_proches_sont_listees_sans_doublon() {
        let mut out = Vec::new();
        let z = bridge_z(0);
        bridge_obstructions(z + 30.0, &mut out);
        assert!(!out.is_empty() && out.len() <= 3);
        assert!(out[0].z_near > out[0].z_far);
    }
}
