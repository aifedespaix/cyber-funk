//! Les quatre matériaux du trafic : véhicules, traînées, points lumineux, murs de lumière. Leurs shaders
//! (`assets/shaders/cf-world-neon/traffic_*.wesl`) sont embarqués dans le binaire : aucun fichier lu pendant un show
//! (règle 8 de `CLAUDE.md`).

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};
use cf_render::ScenePalette;
use cf_render::palette::{LinearRgb, SceneColor};

const VEHICLE_SHADER: Handle<Shader> = uuid_handle!("5c9a1e47-3b82-4d06-a7f1-8e2d6b04c391");
const VEHICLE_SOURCE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/traffic_vehicle.wesl");
const VEHICLE_PATH: &str = "cf-world-neon/traffic_vehicle.wesl";
const TRAIL_SHADER: Handle<Shader> = uuid_handle!("e1d47b92-6a05-4c38-9f2e-b3a8c7d15f60");
const TRAIL_SOURCE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/traffic_trail.wesl");
const TRAIL_PATH: &str = "cf-world-neon/traffic_trail.wesl";
const GLINT_SHADER: Handle<Shader> = uuid_handle!("7f30c8a5-d142-4e9b-8b67-1a5e9d2c4f08");
const GLINT_SOURCE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/traffic_glint.wesl");
const GLINT_PATH: &str = "cf-world-neon/traffic_glint.wesl";
const WALL_SHADER: Handle<Shader> = uuid_handle!("a2b6d9f1-0e73-4584-b1c9-3f7a5e8d2c46");
const WALL_SOURCE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/traffic_wall.wesl");
const WALL_PATH: &str = "cf-world-neon/traffic_wall.wesl";

/// Enregistre les shaders du trafic dans `Assets<Shader>`.
pub(crate) fn embed_shaders(shaders: &mut Assets<Shader>) {
    for (handle, source, path) in [
        (&VEHICLE_SHADER, VEHICLE_SOURCE, VEHICLE_PATH),
        (&TRAIL_SHADER, TRAIL_SOURCE, TRAIL_PATH),
        (&GLINT_SHADER, GLINT_SOURCE, GLINT_PATH),
        (&WALL_SHADER, WALL_SOURCE, WALL_PATH),
    ] {
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path)) {
            // Impossible avec un identifiant UUID ; si cela arrivait, le trafic resterait sans ce shader (invisible)
            // plutôt que de couper le show.
            tracing::error!(%error, path, "shader du trafic non enregistré");
        }
    }
}

fn vec4(color: LinearRgb) -> Vec4 {
    Vec4::new(color.r, color.g, color.b, 1.0)
}

/// Les uniformes du shader des véhicules. Même ordre et mêmes types que la structure `Vehicle` de
/// `traffic_vehicle.wesl` : tout en `Vec4`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct VehicleParams {
    /// Encre : le fond, la brume (`uInk`).
    pub ink: Vec4,
    /// Glace : reflets, phares (`uIce`).
    pub ice: Vec4,
    /// Bleu clair : vitres, gyrophare droit (`uAzure`).
    pub azure: Vec4,
    /// Alarme : gyrophare gauche (`uAlarm`).
    pub alarm: Vec4,
    /// Coque claire : la glace à 8 % (`uHull`).
    pub hull: Vec4,
    /// Coque sombre : l'encre, un peu de glace (`uHullDark`).
    pub hull_dark: Vec4,
    /// Voile violet de la brume (`uHaze`).
    pub haze: Vec4,
    /// Nappe de brouillard sous la route (`uMist`).
    pub mist: Vec4,
    /// `x` : pouls des néons, `y` : gyrophare gauche, `z` : gyrophare droit : les niveaux (0..1) du limiteur.
    pub anim: Vec4,
}

impl VehicleParams {
    /// Les uniformes de la palette de l'image (`derived` de `vehicle-material.ts`), l'animation inchangée.
    #[must_use]
    pub fn from_palette(palette: &ScenePalette, anim: Vec4) -> Self {
        let get = |c| palette.get(c);
        let (ink, ice, azure, violet) = (
            get(SceneColor::Ink),
            get(SceneColor::Ice),
            get(SceneColor::Azure),
            get(SceneColor::Violet),
        );
        let scaled = |c: LinearRgb, k: f32| LinearRgb {
            r: c.r * k,
            g: c.g * k,
            b: c.b * k,
        };
        let lerp = |a: LinearRgb, b: LinearRgb, t: f32| LinearRgb {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
        };
        Self {
            ink: vec4(ink),
            ice: vec4(ice),
            azure: vec4(azure),
            alarm: vec4(get(SceneColor::Alarm)),
            hull: vec4(scaled(ice, 0.08)),
            hull_dark: vec4(lerp(ink, ice, 0.04)),
            haze: vec4(violet),
            mist: vec4(scaled(lerp(azure, violet, 0.5), 0.16)),
            anim,
        }
    }
}

/// Matériau des véhicules : un seul exemplaire pour tous les modèles. Les entités ne portent que leur `MeshTag` (teinte),
/// donc tous les véhicules d'un modèle se regroupent en un seul lot de dessin.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct VehicleMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: VehicleParams,
}

impl Material for VehicleMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(VEHICLE_SHADER)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(VEHICLE_SHADER)
    }

    /// Pas de passage de profondeur : un véhicule n'occulte rien (l'occlusion GPU est celle de la ville).
    fn enable_prepass() -> bool {
        false
    }

    /// Pas de lumières ni d'ombres : le véhicule est entièrement émissif, comme son `ShaderMaterial` de Sillage.
    fn enable_shadows() -> bool {
        false
    }
}

/// Une lumière additive sans uniformes : traînées, points, murs. Un type par shader (Bevy en tire un pipeline).
macro_rules! additive_material {
    ($(#[$meta:meta])* $name:ident, $shader:ident) => {
        $(#[$meta])*
        #[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
        pub struct $name {
            /// Réservé : un matériau sans champ n'a pas de groupe de liaison utile.
            #[uniform(0)]
            pub gain: Vec4,
        }

        impl Material for $name {
            fn vertex_shader() -> ShaderRef {
                ShaderRef::Handle($shader)
            }

            fn fragment_shader() -> ShaderRef {
                ShaderRef::Handle($shader)
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

            /// Deux faces : les plans en croix, les carrés et les rubans se voient de n'importe quel côté
            /// (`DoubleSide` de Sillage).
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
    };
}

additive_material!(
    /// Traînée lumineuse des moteurs : deux plans en croix qui s'effilent vers l'arrière.
    TrailMaterial,
    TRAIL_SHADER
);
additive_material!(
    /// Points lumineux additifs (gyrophares, drones, feux de position, lampadaires).
    GlintMaterial,
    GLINT_SHADER
);
additive_material!(
    /// Murs de lumière des motos.
    WallMaterial,
    WALL_SHADER
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_uniformes_derivent_de_la_palette_comme_sillage() {
        let mut palette = ScenePalette::default();
        palette.set(
            SceneColor::Ice,
            LinearRgb {
                r: 1.0,
                g: 0.5,
                b: 0.25,
            },
        );
        palette.set(SceneColor::Ink, LinearRgb { r: 0.0, g: 0.0, b: 0.0 });
        let params = VehicleParams::from_palette(&palette, Vec4::new(0.1, 0.2, 0.3, 0.0));
        // `uHull` = glace × 0,08 ; `uHullDark` = encre → glace à 4 %.
        assert!((params.hull.x - 0.08).abs() < 1e-6 && (params.hull.y - 0.04).abs() < 1e-6);
        assert!((params.hull_dark.x - 0.04).abs() < 1e-6);
        assert_eq!(params.anim, Vec4::new(0.1, 0.2, 0.3, 0.0));
    }
}
