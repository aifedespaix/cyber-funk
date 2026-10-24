//! Questions posées à la ville affichée : collision, toit sous un point, façade la plus proche (`collides`, `roofAt` et
//! `facade` de `scenery.ts`).
//!
//! Sillage les répondait avec les boîtes de la fenêtre courante. Ici la ville est une fonction pure de l'index de rangée
//! : on régénère les quelques rangées voisines du point demandé (« ce qu'on voit est ce qu'on percute », sans état à
//! garder d'accord avec les entités). Aucune allocation ; un appel coûte quelques dizaines de tirages.

use bevy::math::DVec3;

use super::layout::{BASE_DEPTH_M, Hitbox, Row, SceneryConfig};

/// Une sphère (centre `at`, rayon `radius`, positions absolues) touche-t-elle un immeuble ? Les tours qui plongent sous
/// la route ont leur pied bien en dessous : seul le sommet compte.
#[must_use]
pub fn collides(config: &SceneryConfig, at: DVec3, radius: f64) -> bool {
    let reach = radius * 0.7;
    for index in Row::rows_near(at.z, reach) {
        let row = Row::generate(index, config);
        for hit in row.hitboxes() {
            if at.y < f64::from(hit.top) + reach
                && at.x > f64::from(hit.x[0]) - reach
                && at.x < f64::from(hit.x[1]) + reach
                && at.z > hit.z[0] - reach
                && at.z < hit.z[1] + reach
            {
                // Comme Sillage : la première tour touchée tranche (sous la nappe de brouillard il n'y a rien à toucher).
                return at.y > -f64::from(BASE_DEPTH_M);
            }
        }
    }
    false
}

/// Hauteur du toit sous le point `(x, z)`, ou `None` s'il n'y a pas d'immeuble : de quoi rouler sur les toits.
#[must_use]
pub fn roof_at(config: &SceneryConfig, x: f64, z: f64) -> Option<f32> {
    let mut top: Option<f32> = None;
    for index in Row::rows_near(z, 0.0) {
        for hit in Row::generate(index, config).hitboxes() {
            if x > f64::from(hit.x[0]) && x < f64::from(hit.x[1]) && z > hit.z[0] && z < hit.z[1] {
                top = Some(top.map_or(hit.top, |t| t.max(hit.top)));
            }
        }
    }
    top
}

/// Distance à l'axe de la route de la façade la plus proche d'un côté (`side` : −1 gauche, 1 droite) entre `z − depth`
/// et `z + depth`, ou `None` s'il n'y a rien. Les tours trop basses (moins de 5 m) ou trop lointaines (26 m) ne comptent
/// pas.
#[must_use]
pub fn facade(config: &SceneryConfig, side: i8, z: f64, depth: f64) -> Option<f32> {
    let mut near = f32::INFINITY;
    for index in Row::rows_near(z, depth) {
        for &Hitbox { x, z: span, top } in Row::generate(index, config).hitboxes() {
            if top < 5.0 || span[1] < z - depth || span[0] > z + depth {
                continue;
            }
            // Une tour à cheval sur l'axe n'est ni à gauche ni à droite.
            let (inner, straddles) = if side < 0 { (-x[1], x[1] > 0.0) } else { (x[0], x[0] < 0.0) };
            if straddles {
                continue;
            }
            if inner < near && inner < 26.0 {
                near = inner;
            }
        }
    }
    near.is_finite().then_some(near)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> SceneryConfig {
        SceneryConfig::default()
    }

    /// Une tour du premier plan, la première trouvée sur un tronçon de route.
    fn first_hit(config: &SceneryConfig) -> (Row, Hitbox) {
        (0..200)
            .map(|i| Row::generate(i, config))
            .find_map(|row| row.hitboxes().first().copied().map(|h| (row.clone(), h)))
            .expect("une tour sur 200 rangées")
    }

    #[test]
    fn le_centre_d_une_tour_touche_et_la_route_non() {
        let config = config();
        let (_, hit) = first_hit(&config);
        let centre = DVec3::new(
            f64::from(hit.x[0] + hit.x[1]) / 2.0,
            f64::from(hit.top) / 2.0,
            (hit.z[0] + hit.z[1]) / 2.0,
        );
        assert!(collides(&config, centre, 1.0));
        // Au-dessus du toit, plus loin que la portée (0,7 × rayon) : rien.
        assert!(!collides(&config, DVec3::new(centre.x, f64::from(hit.top) + 5.0, centre.z), 1.0));
        // L'axe de la route est libre à toute hauteur modeste.
        for row in 0..400 {
            let z = -f64::from(row) * 26.0;
            assert!(!collides(&config, DVec3::new(0.0, 3.0, z), 2.0), "route libre à z = {z}");
        }
    }

    #[test]
    fn le_toit_est_celui_de_la_tour() {
        let config = config();
        let (_, hit) = first_hit(&config);
        let x = f64::from(hit.x[0] + hit.x[1]) / 2.0;
        let z = (hit.z[0] + hit.z[1]) / 2.0;
        let roof = roof_at(&config, x, z).expect("un toit");
        assert!(roof >= hit.top - 1e-3, "{roof} < {}", hit.top);
        assert_eq!(roof_at(&config, 0.0, z), None, "rien sur la route");
    }

    #[test]
    fn la_facade_la_plus_proche_est_du_bon_cote() {
        let config = config();
        let mut seen = [false; 2];
        for row in 0..300 {
            let z = -f64::from(row) * 26.0;
            for (i, side) in [-1_i8, 1].into_iter().enumerate() {
                if let Some(distance) = facade(&config, side, z, 10.0) {
                    assert!((13.4..26.0).contains(&distance), "{distance}");
                    seen[i] = true;
                }
            }
        }
        assert_eq!(seen, [true, true]);
    }

    #[test]
    fn une_zone_degagee_libere_la_facade() {
        use super::super::layout::Clearing;
        let mut config = config();
        let z = -26.0 * 40.0;
        let before = facade(&config, 1, z, 20.0);
        assert!(before.is_some());
        config.set_clearings(&[Clearing { z0: z - 200.0, z1: z + 200.0, side: 1 }]);
        assert_eq!(facade(&config, 1, z, 20.0), None);
        assert!(facade(&config, -1, z, 20.0).is_some(), "l'autre côté reste");
    }
}
