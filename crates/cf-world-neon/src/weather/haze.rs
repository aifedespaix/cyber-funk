//! La brume basse : trois voiles de vapeur superposés à ras de la chaussée, qui donnent de l'épaisseur à l'air (la route
//! n'est plus un tracé net). La nappe de [`mist`](super::mist) cache le vide *sous* la route ; celle-ci vit *au-dessus*.
//!
//! Trois plans carrés qui suivent la voiture comme la brume du dessous, à des hauteurs différentes : le motif de chacun
//! dérive à sa vitesse (voir `assets/shaders/cf-world-neon/weather_haze.wesl`), donc ils se croisent et la parallaxe fait
//! le volume. Blend alpha, sans écriture de profondeur : un plan coupé par un mât ne dessine qu'un trait très pâle.
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Pas de `NoFrustumCulling` : plans statiques centrés sur leur origine, `Aabb` exact (épaisseur nulle). Ils suivent la
//! voiture par `AbsolutePosition`.
//!
//! # Qualité adaptative
//!
//! `QualityLevers::particle_density` baisse la densité et allège le bruit ; sous 0,5, le voile du haut est caché.

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};
use cf_render::palette::{LinearRgb, SceneColor};
use cf_render::{AbsolutePosition, ScenePalette, WorldLayer};

use super::mist::TIME_WRAP_S;

/// Côté d'un plan (m) : assez pour que le bord soit déjà fondu dans la distance (voir `far` du shader).
pub const SIZE_M: f32 = 900.0;
/// Part du plan qui est devant la voiture.
pub const AHEAD_FRACTION: f64 = 0.3;
/// Hauteurs des voiles au-dessus de la chaussée (m) et opacité maximale de chacun : un voile rasant, un voile à hauteur de
/// pare-chocs, un voile au-dessus du toit qui passe dans les phares.
pub const LAYERS: [(f64, f32); 3] = [(0.35, 1.0), (1.1, 0.75), (2.6, 0.5)];

const HAZE_SHADER: Handle<Shader> = uuid_handle!("2f8b6c40-9d17-4a3e-b5c2-71e0d4a68f93");
const HAZE_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/weather_haze.wesl");
const HAZE_PATH: &str = "cf-world-neon/weather_haze.wesl";

/// Enregistre le shader de la brume basse dans `Assets<Shader>`.
pub(super) fn embed_shader(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(HAZE_SHADER.id(), Shader::from_wesl(HAZE_SOURCE, HAZE_PATH)) {
        // Impossible avec un identifiant UUID ; sinon la brume basse resterait invisible plutôt que de couper le show.
        tracing::error!(%error, "shader de la brume basse non enregistré");
    }
}

/// Uniformes d'un voile. Même ordre que la structure `Haze` de `weather_haze.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct HazeParams {
    /// Couleur de la brume.
    pub tint: Vec4,
    /// Teinte d'ambiance vive.
    pub glow: Vec4,
    /// `x` : temps (s), `y` : pouls, `z` : densité, `w` : 1 = bruit allégé.
    pub frame: Vec4,
    /// `x` : numéro de la couche, `y` : opacité maximale, `z` : hauteur (m), `w` : vitesse de la voiture (m/s).
    pub layer: Vec4,
}

/// Matériau d'un voile de brume basse.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct HazeMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: HazeParams,
}

impl Material for HazeMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(HAZE_SHADER)
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

    /// Deux faces : la caméra passe parfois sous un voile (vol, plans bas).
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        Ok(())
    }
}

/// Marqueur d'un voile : son indice dans [`LAYERS`].
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct Haze(pub usize);

/// Les matériaux des voiles, pour les réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct HazeRig {
    pub(super) materials: [Handle<HazeMaterial>; LAYERS.len()],
}

/// Où se pose un voile quand la voiture est en `car` : à la hauteur du voile, devant elle.
#[must_use]
pub fn haze_position(car: bevy::math::DVec3, layer: usize) -> bevy::math::DVec3 {
    let height = LAYERS.get(layer).map_or(0.0, |l| l.0);
    bevy::math::DVec3::new(car.x, height, car.z - f64::from(SIZE_M) * AHEAD_FRACTION)
}

/// Ce qui change d'une image à l'autre pour les voiles.
#[derive(Debug, Clone, Copy)]
pub struct HazeFrame {
    /// Teinte d'ambiance du moment.
    pub mood: LinearRgb,
    /// Horloge de la scène, s.
    pub time_s: f64,
    /// Pouls du kick, 0 à 1.
    pub pulse: f32,
    /// Densité, 0 à 1.
    pub density: f32,
    /// Bruit allégé (qualité adaptative).
    pub cheap: bool,
    /// Vitesse de la voiture, m/s.
    pub speed_mps: f32,
}

/// Les uniformes du voile `layer`.
#[must_use]
pub fn haze_params(palette: &ScenePalette, frame: &HazeFrame, layer: usize) -> HazeParams {
    let (height, opacity) = LAYERS.get(layer).copied().unwrap_or((0.0, 0.0));
    let mood = frame.mood;
    // Une brume claire, teintée par les néons : sombre, elle serait invisible sur la chaussée noire.
    let ink = palette.get(SceneColor::Ink);
    let tint = LinearRgb {
        r: ink.r * 0.3 + mood.r * 0.5 + 0.05,
        g: ink.g * 0.3 + mood.g * 0.5 + 0.05,
        b: ink.b * 0.3 + mood.b * 0.5 + 0.08,
    };
    let glow = LinearRgb {
        r: mood.r * 1.1 + 0.05,
        g: mood.g * 1.1 + 0.05,
        b: mood.b * 1.1 + 0.08,
    };
    let v = |c: LinearRgb| Vec4::new(c.r, c.g, c.b, 1.0);
    HazeParams {
        tint: v(tint),
        glow: v(glow),
        frame: Vec4::new(
            (frame.time_s % TIME_WRAP_S) as f32,
            frame.pulse,
            frame.density.clamp(0.0, 1.0),
            if frame.cheap { 1.0 } else { 0.0 },
        ),
        layer: Vec4::new(layer as f32, opacity, height as f32, frame.speed_mps),
    }
}

/// Crée les voiles, une fois, au démarrage.
pub(super) fn spawn_haze(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<HazeMaterial>>,
) {
    let mesh = meshes.add(Plane3d::new(Vec3::Y, Vec2::splat(SIZE_M / 2.0)).mesh().build());
    let handles: [Handle<HazeMaterial>; LAYERS.len()] = std::array::from_fn(|_| materials.add(HazeMaterial::default()));
    for (index, handle) in handles.iter().enumerate() {
        commands.spawn((
            Name::new(format!("brume basse {index}")),
            Haze(index),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(handle.clone()),
        ));
    }
    commands.insert_resource(HazeRig { materials: handles });
}

#[cfg(test)]
mod tests {
    use bevy::math::DVec3;

    use super::*;

    #[test]
    fn les_voiles_sont_a_des_hauteurs_croissantes_au_dessus_de_la_route() {
        let car = DVec3::new(2.0, 0.0, -5_000.0);
        let heights: Vec<f64> = (0..LAYERS.len()).map(|i| haze_position(car, i).y).collect();
        assert!(heights.windows(2).all(|w| w[0] < w[1]) && heights[0] > 0.0);
        assert!((haze_position(car, 0).x - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn la_densite_est_bornee_et_le_bruit_allege_suit_la_qualite() {
        let palette = ScenePalette::default();
        let frame = HazeFrame {
            mood: LinearRgb::default(),
            time_s: 7200.0,
            pulse: 0.3,
            density: 3.0,
            cheap: true,
            speed_mps: 40.0,
        };
        let p = haze_params(&palette, &frame, 1);
        assert!((p.frame.z - 1.0).abs() < f32::EPSILON && p.frame.w > 0.5);
        assert!(p.frame.x < TIME_WRAP_S as f32);
    }
}
