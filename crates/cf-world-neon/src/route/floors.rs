//! Les étages de vol (`flight/floors.ts`), ce que le plan de route en lit : le pilote automatique (J5-06) en reprendra
//! le reste (bandes occupées, marges de la voiture).

/// Les étages de vol : des altitudes fixes au-dessus de la route (0 = au sol, roues sur la chaussée).
///
/// Elles sont choisies pour laisser passer la voiture entre les obstacles les plus courants :
/// - le haut des portiques néon est à 11,5 m (étages 2 et 3 de part et d'autre) ;
/// - le tablier des immeubles-arches occupe 24–35 m (seul l'étage 5 le heurte : la voiture l'esquive) ;
/// - l'arche de voxels passe à 32 m et plus, la route croisée surélevée à 19 m.
pub const FLOORS: [f64; 6] = [3.5, 8.0, 15.0, 21.0, 29.5, 38.5];

/// Nombre d'étages de vol.
pub const FLOOR_COUNT: usize = FLOORS.len();

/// Altitude (m) de l'étage `floor` (1 à 6), 0 pour le sol.
#[must_use]
pub fn floor_altitude(floor: i32) -> f64 {
    if floor <= 0 {
        return 0.0;
    }
    let index = usize::try_from(floor).map_or(FLOOR_COUNT, |f| f.min(FLOOR_COUNT)) - 1;
    FLOORS.get(index).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn altitudes_des_etages() {
        assert!(floor_altitude(0).abs() < f64::EPSILON);
        assert!(floor_altitude(-3).abs() < f64::EPSILON);
        assert!((floor_altitude(1) - 3.5).abs() < f64::EPSILON);
        assert!((floor_altitude(6) - 38.5).abs() < f64::EPSILON);
        assert!(
            (floor_altitude(99) - 38.5).abs() < f64::EPSILON,
            "borné au dernier étage"
        );
    }
}
