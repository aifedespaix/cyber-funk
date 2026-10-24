//! Les uniformes du ciel : ce que chaque matériau lit de la palette et de l'image.

use bevy::math::Vec4;
use bevy::prelude::*;
use cf_render::{SceneColor, ScenePalette};

use super::material::{AuroraParams, DomeParams, PlanetParams, RingParams, StarsParams, StreakParams};
use super::spec::{PLANETS, galaxy_axis, light_dir};

/// Les deux rideaux d'aurore : position, taille `(largeur, hauteur)`, graine et teintes `(bas, milieu, haut)`.
pub(super) const AURORAS: [AuroraSpec; 2] = [
    AuroraSpec {
        at: Vec3::new(-80.0, 85.0, -820.0),
        size: Vec2::new(1900.0, 230.0),
        seed: 0.0,
        tints: [SceneColor::Mint, SceneColor::Azure, SceneColor::Violet],
    },
    AuroraSpec {
        at: Vec3::new(260.0, 70.0, -940.0),
        size: Vec2::new(1700.0, 190.0),
        seed: 1.7,
        tints: [SceneColor::Rose, SceneColor::Violet, SceneColor::Azure],
    },
];

/// Teintes des étoiles : glace, glace, azur, menthe, ambre, corail (`tints` de `sky.ts`).
pub(super) const STAR_TINTS: [SceneColor; 6] = [
    SceneColor::Ice,
    SceneColor::Ice,
    SceneColor::Azure,
    SceneColor::Mint,
    SceneColor::Amber,
    SceneColor::Rose,
];

/// Teintes des étoiles filantes : glace, menthe, azur, corail (`tints` de `shooting-stars.ts`).
pub(super) const STREAK_TINTS: [SceneColor; 4] =
    [SceneColor::Ice, SceneColor::Mint, SceneColor::Azure, SceneColor::Rose];

/// Mouvement des aurores en mode calme (`motion` de `sky.ts`).
pub(super) const CALM_MOTION: f32 = 0.3;

pub(super) struct AuroraSpec {
    pub(super) at: Vec3,
    pub(super) size: Vec2,
    pub(super) seed: f32,
    pub(super) tints: [SceneColor; 3],
}

/// Ce que les matériaux lisent de l'image.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkyFrame {
    /// Temps, en s (`uTime`).
    pub time_s: f32,
    /// Pouls du kick, 0 à 1 (`uPulse`).
    pub pulse: f32,
    /// Fondu du ciel, 0 à 1 (`uReveal`).
    pub reveal: f32,
}

fn rgb(palette: &ScenePalette, color: SceneColor) -> Vec4 {
    let c = palette.get(color);
    Vec4::new(c.r, c.g, c.b, 1.0)
}

/// Uniformes du dôme.
#[must_use]
pub fn build_dome(palette: &ScenePalette, frame: &SkyFrame) -> DomeParams {
    DomeParams {
        ink: rgb(palette, SceneColor::Ink),
        violet: rgb(palette, SceneColor::Violet),
        azure: rgb(palette, SceneColor::Azure),
        rose: rgb(palette, SceneColor::Rose),
        axis: galaxy_axis().extend(0.0),
        frame: Vec4::new(frame.time_s, frame.pulse, frame.reveal, 0.0),
    }
}

/// Uniformes du champ d'étoiles.
#[must_use]
pub fn build_stars(palette: &ScenePalette, frame: &SkyFrame) -> StarsParams {
    StarsParams {
        tints: STAR_TINTS.map(|c| rgb(palette, c)),
        frame: Vec4::new(frame.time_s, frame.pulse, frame.reveal, 0.0),
    }
}

pub(super) fn build_aurora(spec: &AuroraSpec, calm: bool, palette: &ScenePalette, frame: &SkyFrame) -> AuroraParams {
    let [a, b, c] = spec.tints.map(|t| rgb(palette, t));
    AuroraParams {
        a,
        b,
        c,
        frame: Vec4::new(
            frame.time_s,
            frame.pulse,
            spec.seed,
            if calm { CALM_MOTION } else { 1.0 },
        ),
        look: Vec4::new(frame.reveal, 0.0, 0.0, 0.0),
    }
}

/// Uniformes de la surface de l'astre `index` de [`PLANETS`].
#[must_use]
pub fn build_planet(index: usize, palette: &ScenePalette, frame: &SkyFrame) -> PlanetParams {
    PlanetParams {
        light: light_dir().extend(0.0),
        atmo: rgb(palette, PLANETS[index].atmosphere),
        frame: Vec4::new(frame.pulse, frame.reveal, 0.0, 0.0),
    }
}

pub(super) fn build_ring(index: usize, palette: &ScenePalette, frame: &SkyFrame) -> Option<RingParams> {
    let spec = &PLANETS[index];
    let ring = spec.ring?;
    Some(RingParams {
        tint: rgb(palette, ring.tint),
        geo: Vec4::new(spec.radius_m * ring.outer, frame.pulse, frame.reveal, 0.0),
    })
}

/// Uniformes d'un trait : sa teinte et son opacité.
#[must_use]
pub fn build_streak(tint: usize, alpha: f32, palette: &ScenePalette, frame: &SkyFrame) -> StreakParams {
    StreakParams {
        color: rgb(palette, STREAK_TINTS[tint % STREAK_TINTS.len()]),
        frame: Vec4::new(alpha, frame.reveal, 0.0, 0.0),
    }
}
