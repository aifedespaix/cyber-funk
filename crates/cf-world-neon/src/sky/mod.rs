//! Le ciel de Neon City (`sky.ts`, `sky-materials.ts`, `planets.ts`, `shooting-stars.ts` et le fondu de
//! `show/pre-intro-sky.ts` de Sillage) : voûte à nébuleuses et Voie lactée, 6 370 étoiles sur trois profondeurs, deux
//! rideaux d'aurore, sept astres texturés qui tournent dont une géante à anneau, étoiles filantes.
//!
//! # Couche arrière-plan, à l'infini
//!
//! Tout le ciel est dans la couche [`BackgroundLayer`](cf_render::BackgroundLayer) (index 0), vue par la caméra de fond du
//! rig de `cf-render`. Sillage faisait suivre la caméra à un groupe (`group.position = caméra - skyDrift`) ; ici les
//! entités ne bougent jamais, **c'est la caméra de fond qui se place** : même orientation que celle du monde, position =
//! `CameraPose::sky_drift` (`CameraPose::view_for`). Dans le repère du ciel, la caméra est donc à l'origine tant que le
//! réalisateur ne dérive pas, quelle que soit sa position dans le monde (à 400 km comme au départ) : le ciel est « à
//! l'infini en translation » et ne tremble pas, puisqu'aucune grande coordonnée n'y entre. En faisant tendre
//! `sky_drift` vers le centre d'une planète ([`RingPlanet::center`]), le réalisateur « voyage » jusqu'à elle.
//!
//! # Culling
//!
//! Pas de `NoFrustumCulling`, pas de volume forcé : le ciel est statique et centré sur l'origine de son repère, l'`Aabb`
//! que Bevy calcule de chaque mesh est exact. Les étoiles sont des quads dont l'élargissement se fait en pixels dans le
//! vertex shader : leurs sommets restent aux centres, l'`Aabb` des centres suffit. Les tests le vérifient
//! (`aabb_encloses_mesh`). La caméra de fond a un plan lointain de 20 000 m, au-delà du rayon de la voûte (1 000 m).
//!
//! # Coût
//!
//! Aucune entité n'est créée ni détruite en régime établi : le ciel naît au démarrage (une vingtaine d'entités, huit
//! dessins en tout : dôme, étoiles, deux aurores, sept astres partageant un mesh, deux anneaux, cinq traits au pool).
//! Chaque image réécrit les uniformes du dôme, des étoiles et des aurores (le temps change) ; le reste (astres, anneaux,
//! traits) n'est réécrit que lorsque le pouls, la palette ou l'opacité d'un trait change, jamais sinon.
//!
//! # Qui écrit quoi
//!
//! - [`SkyLook`] : le fondu de la pré-intro (`reveal`), le mode calme, l'horloge scriptée ; écrit par la mise en scène.
//! - [`SkyState`] : les étoiles filantes, la rotation des astres ; écrit par le seul `step_sky` (`CfSet::Simulate`).
//! - [`RingPlanet`] : la géante à anneau, pour le réalisateur ; écrite une fois au démarrage.
//! - les matériaux : écrits par le seul `update_materials` (`CfSet::Present`).

mod material;
mod params;
mod shooting;
mod spec;
mod stars;
mod systems;
mod textures;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{BackgroundLayer, CfSet, WorldSet};

pub use material::{
    AuroraMaterial, AuroraParams, DomeMaterial, DomeParams, PlanetMaterial, PlanetParams, RingMaterial, RingParams,
    SkyParams, StarsMaterial, StarsParams, StreakMaterial, StreakParams,
};
pub use params::{SkyFrame, build_dome, build_planet, build_stars, build_streak};
pub use shooting::{MAX_PER_SECOND, MIN_GAP_S, POOL, ShootingStars, Streak};
pub use spec::{PLANETS, PlanetSpec, RADIUS_M, RING_PLANET_TEXTURE, RingSpec, STAR_LAYERS, StarLayer, galaxy_axis};
pub use stars::{Star, star_field, star_mesh};
pub use systems::SkyState;

/// Tout ce qui fait partie du ciel : à interroger pour le compter ou le cacher en bloc.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(BackgroundLayer)]
pub struct SkyPart;

/// Un astre : son indice dans [`PLANETS`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(SkyPart)]
pub struct SkyPlanet(pub usize);

/// Un trait du pool d'étoiles filantes : son indice dans [`ShootingStars::streaks`].
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
#[require(SkyPart)]
pub struct SkyStreak(pub usize);

/// Les réglages du ciel que la mise en scène écrit.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct SkyLook {
    /// Fondu du ciel (`SkyReveal` de la pré-intro) : 0, ciel noir ; 1, ciel entier. Multiplie la sortie de tous les
    /// matériaux.
    pub reveal: f32,
    /// Mode calme (`reducedMotion` de Sillage) : aurores à 30 % de leur vitesse, étoiles filantes plus rares.
    pub calm: bool,
    /// Horloge imposée, en s (captures scriptées) ; `None` : l'horloge de l'application.
    pub clock_s: Option<f64>,
}

impl Default for SkyLook {
    fn default() -> Self {
        Self {
            reveal: 1.0,
            calm: false,
            clock_s: None,
        }
    }
}

/// Ordre de lancer une étoile filante tout de suite (`Sky.shoot()`). Soumis à la cadence maximale de
/// [`ShootingStars::launch`] : un ordre de trop est ignoré.
#[derive(Message, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ShootingStarRequest;

/// La géante à anneau du ciel (`RingPlanet` de `cinema/types.ts`), exposée au réalisateur : les plans de voyage
/// (`ringworld`, `ascension`) visent `center + décalage` avec `CameraPose::sky_drift`.
///
/// Coordonnées dans le repère du ciel (le ciel n'est ni tourné ni mis à l'échelle). Ressource absente tant que sa texture
/// manque (l'astre est alors omis).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct RingPlanet {
    /// Centre, en m.
    pub center: Vec3,
    /// Rayon de la planète, en m.
    pub radius: f32,
    /// Rayon intérieur de l'anneau, en m.
    pub ring_inner: f32,
    /// Rayon extérieur de l'anneau, en m.
    pub ring_outer: f32,
    /// Normale du plan de l'anneau, unitaire.
    pub ring_normal: Vec3,
}

impl RingPlanet {
    /// La géante décrite par `spec`, ou `None` si l'astre n'a pas d'anneau.
    #[must_use]
    pub fn from_spec(spec: &PlanetSpec) -> Option<Self> {
        let ring = spec.ring?;
        // Normale du disque (+Z), pivotée comme sa géométrie (basculement autour de x) puis comme l'astre (inclinaison
        // autour de z).
        let ring_normal =
            (Quat::from_rotation_z(spec.tilt_rad) * Quat::from_rotation_x(spec::RING_TILT_X_RAD)) * Vec3::Z;
        Some(Self {
            center: spec.center(),
            radius: spec.radius_m,
            ring_inner: spec.radius_m * ring.inner,
            ring_outer: spec.radius_m * ring.outer,
            ring_normal,
        })
    }
}

/// Les textures des astres chargés, pour les astres qui tombent (`planetTextures` de Sillage).
#[derive(Resource, Debug, Clone, Default)]
pub struct PlanetTextures(pub Vec<Handle<Image>>);

/// Installe le ciel : matériaux et shaders, entités, et les deux systèmes qui l'animent.
///
/// À ajouter après les plugins de rendu de Bevy : les shaders sont embarqués dans `Assets<Shader>` à la construction. Sans
/// rendu (tests), seuls les matériaux sont déclarés et la logique tourne. Demande `Assets<Mesh>` et `Assets<Image>`.
#[derive(Debug, Default, Clone, Copy)]
pub struct SkyPlugin;

impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins((
                MaterialPlugin::<DomeMaterial>::default(),
                MaterialPlugin::<StarsMaterial>::default(),
                MaterialPlugin::<AuroraMaterial>::default(),
                MaterialPlugin::<PlanetMaterial>::default(),
                MaterialPlugin::<RingMaterial>::default(),
                MaterialPlugin::<StreakMaterial>::default(),
            ));
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<DomeMaterial>()
                .init_asset::<StarsMaterial>()
                .init_asset::<AuroraMaterial>()
                .init_asset::<PlanetMaterial>()
                .init_asset::<RingMaterial>()
                .init_asset::<StreakMaterial>();
        }
        app.init_resource::<SkyLook>()
            .init_resource::<SkyState>()
            .add_message::<ShootingStarRequest>()
            .add_systems(Startup, systems::spawn_sky)
            .add_systems(
                Update,
                systems::step_sky
                    .in_set(CfSet::Simulate)
                    .after(WorldSet::Chunks)
                    .run_if(resource_exists::<systems::SkyRig>),
            )
            .add_systems(
                Update,
                systems::update_materials
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<systems::SkyRig>),
            );
    }
}
