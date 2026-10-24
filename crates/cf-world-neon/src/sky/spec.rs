//! Les constantes réglées à la main du ciel : couches d'étoiles, astres, axe de la Voie lactée (`sky.ts`,
//! `sky-materials.ts`, `planets.ts` de Sillage).

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::math::{Quat, Vec3};
use cf_render::SceneColor;

/// Rayon de la voûte, en m (`RADIUS` de `sky.ts`). Reste sous le plan lointain de la caméra de fond (20 000 m).
pub const RADIUS_M: f32 = 1000.0;

/// Axe du plan de la Voie lactée, unitaire : partagé entre le dôme et la répartition des étoiles (`GALAXY_AXIS`).
#[must_use]
pub fn galaxy_axis() -> Vec3 {
    Vec3::new(0.32, 0.82, -0.47).normalize()
}

/// Une couche d'étoiles (`Layer` de `sky.ts`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StarLayer {
    /// Nombre d'étoiles.
    pub count: u32,
    /// Taille en pixels : `[min, max]`, tirée avec un exposant pour rendre les grosses plus rares.
    pub size_px: [f32; 2],
    /// Exposant du tirage de la taille.
    pub power: f32,
    /// Part des étoiles resserrées autour de la Voie lactée, 0..1.
    pub galactic: f32,
}

/// Les trois profondeurs d'étoiles, de la poussière lointaine aux astres brillants proches.
pub const STAR_LAYERS: [StarLayer; 3] = [
    // poussière d'étoiles lointaines
    StarLayer {
        count: 5200,
        size_px: [1.0, 1.7],
        power: 1.0,
        galactic: 0.55,
    },
    StarLayer {
        count: 1100,
        size_px: [1.8, 3.0],
        power: 2.0,
        galactic: 0.3,
    },
    // astres brillants proches
    StarLayer {
        count: 70,
        size_px: [4.5, 8.5],
        power: 1.5,
        galactic: 0.0,
    },
];

/// Un anneau de planète : teinte, rayons intérieur et extérieur en multiples du rayon de la planète.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingSpec {
    /// Rôle de couleur de la scène.
    pub tint: SceneColor,
    /// Rayon intérieur, en rayons de planète.
    pub inner: f32,
    /// Rayon extérieur, en rayons de planète.
    pub outer: f32,
}

/// Un astre (`PlanetSpec` de `planets.ts`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanetSpec {
    /// Nom du fichier de texture, sans extension (`assets/textures/planets/<nom>.jpg`).
    pub texture: &'static str,
    /// Direction depuis la caméra : azimut (0 = droit devant), en degrés.
    pub azimuth_deg: f32,
    /// Direction depuis la caméra : élévation, en degrés.
    pub elevation_deg: f32,
    /// Distance au centre du ciel, en m.
    pub distance_m: f32,
    /// Rayon, en m.
    pub radius_m: f32,
    /// Inclinaison de l'axe (rotation autour de z), en rad.
    pub tilt_rad: f32,
    /// Vitesse de rotation sur l'axe, en tours par minute.
    pub rpm: f32,
    /// Teinte de l'atmosphère.
    pub atmosphere: SceneColor,
    /// Anneau éventuel.
    pub ring: Option<RingSpec>,
}

/// Du plus proche au plus lointain : lune voisine, géantes gazeuses, mondes glacés et océaniques qui ne sont plus que
/// des billes. Textures CC0 (voir `assets/textures/planets/LICENCE.md`).
pub const PLANETS: [PlanetSpec; 7] = [
    PlanetSpec {
        texture: "moon",
        azimuth_deg: 16.0,
        elevation_deg: 30.0,
        distance_m: 300.0,
        radius_m: 20.0,
        tilt_rad: 0.1,
        rpm: 0.8,
        atmosphere: SceneColor::Ice,
        ring: None,
    },
    PlanetSpec {
        texture: "gas-ember",
        azimuth_deg: -30.0,
        elevation_deg: 21.0,
        distance_m: 560.0,
        radius_m: 120.0,
        tilt_rad: 0.38,
        rpm: 0.5,
        atmosphere: SceneColor::Amber,
        ring: Some(RingSpec {
            tint: SceneColor::Amber,
            inner: 1.45,
            outer: 2.35,
        }),
    },
    PlanetSpec {
        texture: "ocean",
        azimuth_deg: 38.0,
        elevation_deg: 13.0,
        distance_m: 640.0,
        radius_m: 66.0,
        tilt_rad: 0.3,
        rpm: 1.2,
        atmosphere: SceneColor::Azure,
        ring: None,
    },
    PlanetSpec {
        texture: "gas-azure",
        azimuth_deg: -62.0,
        elevation_deg: 36.0,
        distance_m: 820.0,
        radius_m: 50.0,
        tilt_rad: -0.2,
        rpm: 0.9,
        atmosphere: SceneColor::Mint,
        ring: None,
    },
    PlanetSpec {
        texture: "ice",
        azimuth_deg: -4.0,
        elevation_deg: 8.0,
        distance_m: 900.0,
        radius_m: 17.0,
        tilt_rad: 0.5,
        rpm: 1.6,
        atmosphere: SceneColor::Ice,
        ring: None,
    },
    PlanetSpec {
        texture: "gas-rose",
        azimuth_deg: 72.0,
        elevation_deg: 28.0,
        distance_m: 760.0,
        radius_m: 40.0,
        tilt_rad: 0.15,
        rpm: 0.7,
        atmosphere: SceneColor::Rose,
        ring: Some(RingSpec {
            tint: SceneColor::Rose,
            inner: 1.4,
            outer: 1.9,
        }),
    },
    PlanetSpec {
        texture: "gas-teal",
        azimuth_deg: 160.0,
        elevation_deg: 22.0,
        distance_m: 600.0,
        radius_m: 70.0,
        tilt_rad: 0.25,
        rpm: 0.6,
        atmosphere: SceneColor::Mint,
        ring: None,
    },
];

/// La géante à anneau exposée au réalisateur : la texture de cette planète porte l'anneau que les plans visent.
pub const RING_PLANET_TEXTURE: &str = "gas-ember";

/// Basculement de l'anneau autour de x, en rad : `rotateX(-π/2 + 0.28)` de `planets.ts` couche le disque à plat, avec
/// 0,28 rad d'inclinaison vers l'observateur.
pub const RING_TILT_X_RAD: f32 = -FRAC_PI_2 + 0.28;

/// Direction du soleil hors champ qui éclaire les astres (`LIGHT` de `planets.ts`), unitaire.
#[must_use]
pub fn light_dir() -> Vec3 {
    Vec3::new(-0.38, 0.35, 0.86).normalize()
}

impl PlanetSpec {
    /// Centre de l'astre dans le repère du ciel, en m.
    #[must_use]
    pub fn center(&self) -> Vec3 {
        let az = self.azimuth_deg * PI / 180.0;
        let el = self.elevation_deg * PI / 180.0;
        Vec3::new(az.sin() * el.cos(), el.sin(), -az.cos() * el.cos()) * self.distance_m
    }

    /// Rotation de l'astre : son inclinaison (autour de z) composée à l'angle de rotation sur son axe (autour de y).
    #[must_use]
    pub fn rotation(&self, spin_rad: f32) -> Quat {
        Quat::from_rotation_z(self.tilt_rad) * Quat::from_rotation_y(spin_rad)
    }

    /// Angle dont l'astre tourne en `dt_s` secondes, en rad.
    #[must_use]
    pub fn spin_step_rad(&self, dt_s: f32) -> f32 {
        self.rpm * std::f32::consts::TAU / 60.0 * dt_s
    }
}
