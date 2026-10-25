//! Les compteurs de coût de la pluie, calculés sur le processeur : pas de GPU dans l'environnement de développement (ni de
//! carte son : voir `CLAUDE.md`, §5), donc pas de millisecondes GPU mesurées. Ce module rejoue la géométrie du vertex shader
//! (`weather_rain.wesl`) pour une caméra donnée et compte ce que le GPU aura à faire : sommets, triangles, **fragments**
//! couverts (le vrai coût d'un voile additif) et leur part qui passe par la boucle des six lumières de la ville.
//!
//! C'est une estimation : elle projette chaque traînée comme un rectangle de face (la traînée est tournée vers la caméra) et
//! ignore le recouvrement, le rejet précoce et le batching des fragments en quads 2 × 2 (qui gonfle le coût des traînées
//! fines, de 0,014 m de large : un fragment sur deux est un fragment d'aide). Les mesures GPU restent à faire sur la machine
//! du spectacle (J4-06, surcouche de diagnostic).

use bevy::math::Vec3;

use super::{AHEAD_FRACTION, BOX_M, DROPS};

/// Ce que la pluie coûte au GPU pour une image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RainCost {
    /// Gouttes affichées (graine sous la densité).
    pub drops_shown: usize,
    /// Sommets passés au vertex shader : tous, affichés ou non (8 800 : négligeable).
    pub vertices: usize,
    /// Triangles que le rasterizer reçoit : ceux des gouttes affichées seulement (les autres sont rejetés par le clipping).
    pub triangles: usize,
    /// Gouttes dans le champ de la caméra et à moins de 150 m (au-delà, la distance les éteint).
    pub drops_visible: usize,
    /// Fragments couverts, en pixels (somme des rectangles projetés, sans recouvrement).
    pub fragments_px: f64,
    /// Même nombre, en part de l'écran (1,0 = autant de fragments que de pixels).
    pub overdraw: f64,
}

/// Les compteurs de la pluie vue de `camera`, la boîte au repos à l'instant `time_s`. La caméra regarde vers −z (le
/// repère de Sillage), à `fov_y_rad` de champ vertical, dans un écran de `viewport_px`.
#[must_use]
pub fn estimate(
    seeds: &[[f32; 4]],
    density: f32,
    camera: Vec3,
    time_s: f32,
    viewport_px: (u32, u32),
    fov_y_rad: f32,
) -> RainCost {
    let (width, height) = (viewport_px.0 as f32, viewport_px.1 as f32);
    let tan_half = (fov_y_rad * 0.5).tan();
    let focal_px = height * 0.5 / tan_half;
    let center = Vec3::new(camera.x, 0.0, camera.z - BOX_M.z * AHEAD_FRACTION);
    let wrap = |x: f32, y: f32| x - y * (x / y).floor();
    let (mut shown, mut visible, mut fragments) = (0, 0, 0.0_f64);
    for seed in seeds.iter().take(DROPS) {
        if seed[3] > density {
            continue;
        }
        shown += 1;
        let speed = 34.0 + seed[2] * 16.0;
        let mut p = Vec3::new(
            seed[0] * BOX_M.x,
            BOX_M.y - wrap(seed[2] * BOX_M.y + time_s * speed, BOX_M.y),
            seed[1] * BOX_M.z,
        );
        p.x = center.x + wrap(p.x - center.x + BOX_M.x * 0.5, BOX_M.x) - BOX_M.x * 0.5;
        p.z = center.z + wrap(p.z - center.z + BOX_M.z * 0.5, BOX_M.z) - BOX_M.z * 0.5;
        let depth = camera.z - p.z;
        let dist = (p - camera).length();
        if depth < 0.3 || dist >= 150.0 {
            continue;
        }
        let (x, y) = ((p.x - camera.x) / depth * focal_px, (p.y - camera.y) / depth * focal_px);
        if x.abs() > width * 0.5 || y.abs() > height * 0.5 {
            continue;
        }
        visible += 1;
        let len = 1.2 + seed[2] * 1.4;
        let fade = 1.0 - smoothstep(60.0, 150.0, dist);
        // Largeur 2 × 0,014 m, longueur `len`, vues à la distance `dist` ; le sinus de l'alpha n'en laisse qu'une part.
        let area = (0.028 * focal_px / dist).max(1.0) * (len * focal_px / dist) * fade.min(1.0);
        fragments += f64::from(area);
    }
    RainCost {
        drops_shown: shown,
        vertices: seeds.len() * 4,
        triangles: shown * 2,
        drops_visible: visible,
        fragments_px: fragments,
        overdraw: fragments / f64::from(width * height),
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::super::rain_seeds;
    use super::*;

    const VIEW: (u32, u32) = (1920, 1080);
    const FOV: f32 = 62.0 * std::f32::consts::PI / 180.0;

    fn cost(density: f32) -> RainCost {
        estimate(&rain_seeds(3), density, Vec3::new(0.0, 3.0, -1000.0), 12.5, VIEW, FOV)
    }

    #[test]
    fn la_densite_retire_des_gouttes_et_des_fragments_proportionnellement() {
        let (full, half, none) = (cost(1.0), cost(0.5), cost(0.0));
        assert_eq!(full.drops_shown, DROPS);
        assert_eq!(full.triangles, DROPS * 2);
        assert!(
            (half.drops_shown as f32 / DROPS as f32 - 0.5).abs() < 0.05,
            "{}",
            half.drops_shown
        );
        let ratio = half.fragments_px / full.fragments_px;
        assert!(
            (0.35..0.65).contains(&ratio),
            "la moitié des gouttes : {ratio} des fragments"
        );
        assert_eq!(none.drops_shown, 0);
        assert!(none.fragments_px == 0.0 && none.triangles == 0);
        assert_eq!(
            full.vertices, half.vertices,
            "les sommets passent tous, le clipping fait le tri"
        );
    }

    #[test]
    fn la_pluie_a_pleine_densite_reste_un_voile_leger() {
        let full = cost(1.0);
        println!("{full:?}");
        // Quelques centaines de gouttes dans le champ, moins de 15 % de l'écran en fragments : un voile, pas un mur.
        assert!(
            full.drops_visible > 100 && full.drops_visible < DROPS,
            "{}",
            full.drops_visible
        );
        assert!(full.overdraw < 0.15, "surcharge {:.3}", full.overdraw);
    }
}
