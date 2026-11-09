//! Les astres qui bougent dans le ciel : la Lune de l'océan et une géante lointaine qui tourne autour de notre monde.
//!
//! Ces deux corps n'existent pas dans Sillage : ils s'ajoutent aux sept astres fixes de [`PLANETS`](super::spec::PLANETS)
//! (dont les données restent celles de la référence) au lieu de les modifier. Le sens de rotation de chaque astre est
//! lui aussi ici ([`spin_direction`]), pour ne pas toucher aux données de référence.

use std::f32::consts::TAU;

use bevy::math::{Quat, Vec3};
use cf_render::SceneColor;

use super::spec::{PLANETS, PlanetSpec};

/// Sens de rotation de chaque astre de [`PLANETS`] sur son axe : `1` comme la Terre, `-1` à l'envers (rotation
/// rétrograde, comme Vénus). Mêmes rangs que [`PLANETS`].
///
/// Les mondes rapprochés tournent à l'endroit, trois géantes et le monde glacé à l'envers : le ciel cesse de ressembler
/// à un seul mécanisme.
pub const SPIN_DIRECTION: [f32; PLANETS.len()] = [1.0, 1.0, 1.0, -1.0, -1.0, -1.0, 1.0];

/// Sens de rotation de l'astre `index` de [`PLANETS`] : `1` ou `-1` (`1` hors tableau).
#[must_use]
pub fn spin_direction(index: usize) -> f32 {
    SPIN_DIRECTION.get(index).copied().unwrap_or(1.0)
}

/// La trajectoire d'un astre en orbite : un cercle dans un plan basculé autour de x.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Orbit {
    /// Rang de l'astre de [`PLANETS`] autour duquel on tourne, ou `None` pour tourner autour du centre du ciel (notre
    /// monde, vu de la voiture).
    pub around: Option<usize>,
    /// Décalage du centre de l'orbite par rapport à ce foyer, en m. Relève une orbite autour de notre monde pour que
    /// l'astre ne passe jamais franchement sous l'horizon.
    pub offset: Vec3,
    /// Rayon de l'orbite, en m.
    pub radius_m: f32,
    /// Durée d'un tour, en s.
    pub period_s: f32,
    /// Basculement du plan de l'orbite autour de x, en rad (0 : à plat).
    pub inclination_rad: f32,
    /// Angle de départ, en rad (0 : à droite du foyer ; π/2 : devant lui).
    pub phase_rad: f32,
}

/// Un astre en orbite : un corps (texture, taille, rotation) et sa trajectoire.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbiterSpec {
    /// Le corps. Son azimut, son élévation et sa distance ne servent pas : la position vient de [`Orbit`].
    pub body: PlanetSpec,
    /// Sens de rotation sur l'axe, `1` ou `-1`.
    pub spin_direction: f32,
    /// La trajectoire.
    pub orbit: Orbit,
}

/// Les deux astres en orbite.
///
/// - **La Lune** tourne autour de l'océan (la « Terre » de droite) en 75 s et lui montre toujours la même face : sa
///   rotation sur l'axe dure aussi 75 s, à contre-sens de son orbite.
/// - **La géante glacée** est grosse et lointaine (110 m à ~700 m) ; elle tourne autour de notre monde en 7 minutes, sur
///   une orbite inclinée et relevée : elle traverse le champ, passe dans le dos de la voiture puis revient. Tout reste
///   sous le rayon de la voûte (voir le test `les_orbites_restent_sous_la_voute`).
pub const ORBITERS: [OrbiterSpec; 2] = [
    OrbiterSpec {
        body: PlanetSpec {
            texture: "moon",
            azimuth_deg: 0.0,
            elevation_deg: 0.0,
            distance_m: 0.0,
            radius_m: 9.0,
            tilt_rad: 0.05,
            rpm: 0.8,
            atmosphere: SceneColor::Ice,
            ring: None,
        },
        spin_direction: -1.0,
        orbit: Orbit {
            around: Some(2),
            offset: Vec3::ZERO,
            radius_m: 130.0,
            period_s: 75.0,
            inclination_rad: 0.5,
            phase_rad: 0.0,
        },
    },
    OrbiterSpec {
        body: PlanetSpec {
            texture: "ice",
            azimuth_deg: 0.0,
            elevation_deg: 0.0,
            distance_m: 0.0,
            radius_m: 110.0,
            tilt_rad: -0.35,
            rpm: 0.35,
            atmosphere: SceneColor::Azure,
            ring: None,
        },
        spin_direction: -1.0,
        orbit: Orbit {
            around: None,
            offset: Vec3::new(0.0, 260.0, 0.0),
            radius_m: 700.0,
            period_s: 420.0,
            inclination_rad: 0.3,
            phase_rad: 1.0,
        },
    },
];

impl Orbit {
    /// Position de l'astre `elapsed_s` secondes après le début, dans le repère du ciel, en m.
    #[must_use]
    pub fn position(&self, elapsed_s: f64) -> Vec3 {
        let focus = self.around.map_or(Vec3::ZERO, |i| PLANETS[i].center());
        // Reste du temps modulo la période en f64 : l'angle garde sa précision après des heures de show.
        let turns = (elapsed_s / f64::from(self.period_s)).fract() as f32;
        let angle = self.phase_rad + turns * TAU;
        let flat = Vec3::new(angle.cos(), 0.0, -angle.sin()) * self.radius_m;
        focus + self.offset + Quat::from_rotation_x(self.inclination_rad) * flat
    }
}

impl OrbiterSpec {
    /// Angle dont l'astre tourne sur son axe en `dt_s` secondes, en rad, signe compris.
    #[must_use]
    pub fn spin_step_rad(&self, dt_s: f32) -> f32 {
        self.body.spin_step_rad(dt_s) * self.spin_direction
    }
}

#[cfg(test)]
mod tests {
    use super::super::spec::RADIUS_M;
    use super::*;

    #[test]
    fn les_orbites_restent_sous_la_voute() {
        for orbiter in &ORBITERS {
            for step in 0..2000 {
                let at = orbiter
                    .orbit
                    .position(f64::from(step) * f64::from(orbiter.orbit.period_s) / 2000.0);
                assert!(
                    at.length() + orbiter.body.radius_m < RADIUS_M,
                    "{} sort de la voûte à {at}",
                    orbiter.body.texture
                );
            }
        }
    }

    #[test]
    fn la_geante_passe_devant_puis_derriere_sans_passer_sous_l_horizon() {
        let orbit = ORBITERS[1].orbit;
        let (mut front, mut back) = (false, false);
        for step in 0..2000 {
            let at = orbit.position(f64::from(step) * f64::from(orbit.period_s) / 2000.0);
            front |= at.z < -300.0;
            back |= at.z > 300.0;
            assert!(at.y > ORBITERS[1].body.radius_m * 0.4, "centre sous l'horizon : {at}");
        }
        assert!(front && back, "elle fait le tour de la voiture");
    }

    #[test]
    fn la_lune_tourne_autour_de_l_ocean() {
        let ocean = PLANETS[2].center();
        let orbit = ORBITERS[0].orbit;
        for step in 0..200 {
            let at = orbit.position(f64::from(step) * 0.5);
            assert!((at.distance(ocean) - orbit.radius_m).abs() < 0.01);
        }
        let (a, b) = (orbit.position(0.0), orbit.position(f64::from(orbit.period_s)));
        assert!(a.distance(b) < 0.01, "un tour exact ramène au départ");
    }

    #[test]
    fn des_astres_tournent_a_l_envers() {
        assert!(SPIN_DIRECTION.iter().any(|d| *d < 0.0) && SPIN_DIRECTION.iter().any(|d| *d > 0.0));
        assert!(SPIN_DIRECTION.iter().all(|d| (d.abs() - 1.0).abs() < f32::EPSILON));
        assert!((spin_direction(99) - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn la_lune_montre_toujours_la_meme_face() {
        // 0,8 tr/min = un tour en 75 s, comme son orbite.
        let moon = ORBITERS[0];
        let step = moon.body.spin_step_rad(moon.orbit.period_s).abs();
        assert!((step - TAU).abs() < 1e-3, "{step}");
    }
}
