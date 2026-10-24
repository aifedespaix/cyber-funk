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
use bevy::image::Image;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::material::AlphaMode;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::{Shader, ShaderRef};

use super::stars::TINT_COUNT;

macro_rules! shader {
    ($handle:ident, $uuid:literal, $file:literal, $source:ident, $path:ident) => {
        const $handle: Handle<Shader> = uuid_handle!($uuid);
        const $source: &str = include_str!(concat!("../../../../assets/shaders/cf-world-neon/", $file));
        const $path: &str = concat!("cf-world-neon/", $file);
    };
}

shader!(DOME, "3b7f2c10-5a4e-4d86-9c21-7e0a1f5b8d31", "sky_dome.wesl", DOME_SRC, DOME_PATH);
shader!(STARS, "3b7f2c11-5a4e-4d86-9c21-7e0a1f5b8d32", "sky_stars.wesl", STARS_SRC, STARS_PATH);
shader!(AURORA, "3b7f2c12-5a4e-4d86-9c21-7e0a1f5b8d33", "sky_aurora.wesl", AURORA_SRC, AURORA_PATH);
shader!(PLANET, "3b7f2c13-5a4e-4d86-9c21-7e0a1f5b8d34", "sky_planet.wesl", PLANET_SRC, PLANET_PATH);
shader!(RING, "3b7f2c14-5a4e-4d86-9c21-7e0a1f5b8d35", "sky_ring.wesl", RING_SRC, RING_PATH);
shader!(STREAK, "3b7f2c15-5a4e-4d86-9c21-7e0a1f5b8d36", "sky_streak.wesl", STREAK_SRC, STREAK_PATH);

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

// ---------------------------------------------------------------------------------------------------------------------
// Dôme

/// Uniformes du dôme. Mêmes champs que la structure `Dome` de `sky_dome.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct DomeParams {
    /// Encre (`uInk`).
    pub ink: Vec4,
    /// Violet (`uViolet`).
    pub violet: Vec4,
    /// Azur (`uAzure`).
    pub azure: Vec4,
    /// Corail (`uRose`).
    pub rose: Vec4,
    /// `xyz` : axe de la Voie lactée (`uAxis`).
    pub axis: Vec4,
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Fond de l'espace : noir profond, nébuleuses diffuses et bande de Voie lactée.
///
/// Le shader lit la direction dans la position monde du fragment : l'entité reste à l'origine du repère du ciel (c'est la
/// caméra de fond qui se déplace, voir `cf_render::layers`).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct DomeMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: DomeParams,
}
sky_params!(DomeMaterial, DomeParams);

impl Material for DomeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(DOME)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        double_sided(descriptor);
        // `depthWrite: false` de Sillage : le fond est derrière tout, il ne doit rien masquer.
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Étoiles

/// Uniformes du champ d'étoiles.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct StarsParams {
    /// Les six teintes (glace, glace, azur, menthe, ambre, corail), que l'indice du sommet choisit.
    pub tints: [Vec4; TINT_COUNT],
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Champ d'étoiles : taille en pixels, scintillement, éclat additionnel sur les kicks.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct StarsMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: StarsParams,
}
sky_params!(StarsMaterial, StarsParams);

impl Material for StarsMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(STARS)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(STARS)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Les quads sont toujours tournés vers la caméra mais leur enroulement dépend du signe de `w` : on ne cull rien.
        double_sided(descriptor);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Aurore

/// Uniformes d'un rideau d'aurore.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct AuroraParams {
    /// Teinte du bas du rideau (`uA`).
    pub a: Vec4,
    /// Teinte du milieu (`uB`).
    pub b: Vec4,
    /// Teinte du haut (`uC`).
    pub c: Vec4,
    /// `x` : temps (s), `y` : pouls, `z` : graine du rideau (`uSeed`), `w` : mouvement (`uMotion`, 0,3 en mode calme).
    pub frame: Vec4,
    /// `x` : fondu du ciel (`uReveal`).
    pub look: Vec4,
}

/// Rideau d'aurore boréale : un plan vertical très lointain, en bruit animé.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct AuroraMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: AuroraParams,
}
sky_params!(AuroraMaterial, AuroraParams);

impl Material for AuroraMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(AURORA)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        double_sided(descriptor);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Planètes

/// Uniformes de la surface d'un astre.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct PlanetParams {
    /// `xyz` : direction du soleil hors champ (`uLight`).
    pub light: Vec4,
    /// Teinte de l'atmosphère (`uAtmo`).
    pub atmo: Vec4,
    /// `x` : pouls (`uPulse`), `y` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Surface d'un astre : texture, éclairage diurne, liseré de Fresnel et lueur au terminateur.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct PlanetMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: PlanetParams,
    /// La texture de surface (`uMap`), avec ses mips.
    #[texture(1)]
    #[sampler(2)]
    pub map: Option<Handle<Image>>,
}
sky_params!(PlanetMaterial, PlanetParams);

impl Material for PlanetMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(PLANET)
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }
}

/// Uniformes d'un anneau.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct RingParams {
    /// Teinte de l'anneau (`uTint`).
    pub tint: Vec4,
    /// `x` : rayon extérieur en m (`uOuter`), `y` : pouls (`uPulse`), `z` : fondu du ciel (`uReveal`).
    pub geo: Vec4,
}

/// Anneaux concentriques de densités variées, avec une division nette.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct RingMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: RingParams,
}
sky_params!(RingMaterial, RingParams);

impl Material for RingMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(RING)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(RING)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        double_sided(descriptor);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Étoiles filantes

/// Uniformes d'un trait.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct StreakParams {
    /// Teinte (`uColor`).
    pub color: Vec4,
    /// `x` : opacité (`uAlpha`), `y` : fondu du ciel (`uReveal`).
    pub frame: Vec4,
}

/// Un trait à tête brillante qui glisse sur la voûte.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct StreakMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: StreakParams,
}
sky_params!(StreakMaterial, StreakParams);

impl Material for StreakMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(STREAK)
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        double_sided(descriptor);
        Ok(())
    }
}
