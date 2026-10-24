//! Les six matériaux du ciel et leurs shaders WESL (`assets/shaders/cf-world-neon/sky_*.wesl`), portés des
//! `ShaderMaterial` de `sky-materials.ts`, `planets.ts` et `shooting-stars.ts`.
//!
//! | Matériau | Rendu | Sillage |
//! | --- | --- | --- |
//! | [`DomeMaterial`] | opaque, sans écriture de profondeur, vu de l'intérieur | `createDome` |
//! | [`StarsMaterial`] | additif, quads élargis en pixels par le vertex shader | `createStars` |
//! | [`AuroraMaterial`] | additif, double face | `createAurora` |
//! | [`PlanetMaterial`] | opaque, texturé, éclairé par un soleil hors champ | `surfaceMaterial` |
//! | [`RingMaterial`] | mélange alpha, double face, anneaux de densités variées | `ringMaterial` |
//! | [`StreakMaterial`] | additif, double face | matériau d'une étoile filante |
//!
//! Tous ont la même forme : un seul bloc d'uniformes en `Vec4` (aucune règle d'alignement à retenir), qui porte la palette
//! et l'état de l'image, plus `uReveal` : le fondu du ciel de la pré-intro (`SkyReveal` de `show/pre-intro-sky.ts`), qui
//! multiplie la sortie du fragment. Sillage le greffait après coup dans le GLSL de chaque matériau ; ici c'est un champ.
//!
//! Aucun ne reçoit de lumière de la scène ni d'ombre, ni ne passe par le passage de profondeur (le ciel est dans sa propre
//! couche et n'occulte rien) : tout est désactivé pour qu'aucun pipeline inutile ne soit compilé.

use bevy::asset::{Asset, Handle, uuid_handle};
use bevy::render::render_resource::RenderPipelineDescriptor;
use bevy::shader::Shader;

macro_rules! shader {
    ($handle:ident, $uuid:literal, $file:literal, $source:ident, $path:ident) => {
        const $handle: Handle<Shader> = uuid_handle!($uuid);
        const $source: &str = include_str!(concat!("../../../../../assets/shaders/cf-world-neon/", $file));
        const $path: &str = concat!("cf-world-neon/", $file);
    };
}

shader!(
    DOME,
    "3b7f2c10-5a4e-4d86-9c21-7e0a1f5b8d31",
    "sky_dome.wesl",
    DOME_SRC,
    DOME_PATH
);
shader!(
    STARS,
    "3b7f2c11-5a4e-4d86-9c21-7e0a1f5b8d32",
    "sky_stars.wesl",
    STARS_SRC,
    STARS_PATH
);
shader!(
    AURORA,
    "3b7f2c12-5a4e-4d86-9c21-7e0a1f5b8d33",
    "sky_aurora.wesl",
    AURORA_SRC,
    AURORA_PATH
);
shader!(
    PLANET,
    "3b7f2c13-5a4e-4d86-9c21-7e0a1f5b8d34",
    "sky_planet.wesl",
    PLANET_SRC,
    PLANET_PATH
);
shader!(
    RING,
    "3b7f2c14-5a4e-4d86-9c21-7e0a1f5b8d35",
    "sky_ring.wesl",
    RING_SRC,
    RING_PATH
);
shader!(
    STREAK,
    "3b7f2c15-5a4e-4d86-9c21-7e0a1f5b8d36",
    "sky_streak.wesl",
    STREAK_SRC,
    STREAK_PATH
);

/// Enregistre les shaders du ciel dans `Assets<Shader>` : embarqués dans le binaire, rien à lire pendant un show.
pub(crate) fn embed_shaders(shaders: &mut bevy::asset::Assets<Shader>) {
    for (handle, source, path) in [
        (&DOME, DOME_SRC, DOME_PATH),
        (&STARS, STARS_SRC, STARS_PATH),
        (&AURORA, AURORA_SRC, AURORA_PATH),
        (&PLANET, PLANET_SRC, PLANET_PATH),
        (&RING, RING_SRC, RING_PATH),
        (&STREAK, STREAK_SRC, STREAK_PATH),
    ] {
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path)) {
            // Impossible avec un identifiant UUID ; si cela arrivait, cet élément du ciel resterait invisible plutôt que
            // de couper le show.
            tracing::error!(%error, path, "shader du ciel non enregistré");
        }
    }
}

/// Les deux faces sont dessinées : le dôme est vu de l'intérieur, les rideaux et les traits de n'importe quel côté
/// (`DoubleSide` / `BackSide` de Sillage).
fn double_sided(descriptor: &mut RenderPipelineDescriptor) {
    descriptor.primitive.cull_mode = None;
}

/// Accès commun aux uniformes d'un matériau du ciel : le système d'image compare avant d'écrire, pour que Bevy ne
/// re-prépare pas un matériau qui n'a pas changé.
pub trait SkyParams: Asset {
    /// Le bloc d'uniformes.
    type Params: PartialEq + Copy;
    /// Les uniformes actuels.
    fn params(&self) -> &Self::Params;
    /// Remplace les uniformes.
    fn set_params(&mut self, params: Self::Params);
}

macro_rules! sky_params {
    ($material:ty, $params:ty) => {
        impl SkyParams for $material {
            type Params = $params;
            fn params(&self) -> &$params {
                &self.params
            }
            fn set_params(&mut self, params: $params) {
                self.params = params;
            }
        }
    };
}

// Après les macros : les sous-modules s'en servent.
mod background;
mod bodies;

pub use background::{AuroraMaterial, AuroraParams, DomeMaterial, DomeParams, StarsMaterial, StarsParams};
pub use bodies::{PlanetMaterial, PlanetParams, RingMaterial, RingParams, StreakMaterial, StreakParams};
