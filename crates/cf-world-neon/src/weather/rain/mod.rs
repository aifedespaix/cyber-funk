//! La pluie (`rain.ts` de Sillage) : [`DROPS`] gouttes dans une boîte qui suit la caméra, animées entièrement par le vertex
//! shader (`assets/shaders/cf-world-neon/weather_rain.wesl`). Une seule entité, un seul maillage statique, un seul matériau :
//! un draw call, aucune mise à jour du maillage, aucune allocation par image.
//!
//! # Maillage
//!
//! [`DROPS`] quads de quatre sommets, tous à l'origine : la position vient du shader. Le coin du quad est dans `UV_0`,
//! la graine de la goutte (position, vitesse, seuil de densité) dans `COLOR` (comme la gerbe de débris des astres).
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Pas de `NoFrustumCulling`. Les sommets étant tous nuls, l'`Aabb` calculé par Bevy serait un point : on pose un `Aabb`
//! explicite, celui de la boîte de pluie (marge de [`AABB_MARGIN_M`] pour la longueur des traînées et l'inclinaison). Il est
//! statique : c'est l'entité qui suit la caméra, par sa `AbsolutePosition` (le centre de la boîte). Boîte hors champ ou
//! pluie nulle : l'entité est cachée, le GPU ne la voit pas.
//!
//! # Densité
//!
//! `uDensity` = intensité de l'averse × densité de la qualité adaptative (`QualityLevers::rain_density`). Une goutte dont
//! la graine dépasse la densité envoie ses sommets derrière le plan lointain : zéro fragment (voir [`cost`]).

use bevy::asset::{Asset, Assets, Handle, RenderAssetUsages, uuid_handle};
use bevy::camera::primitives::Aabb;
use bevy::material::AlphaMode;
use bevy::math::{Vec3, Vec4};
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};
use cf_render::{AbsolutePosition, WorldLayer};
use rand_core::{Rng, SeedableRng};
use rand_pcg::Pcg32;

use crate::landmarks::field::FieldParams;

pub mod cost;

/// Nombre de gouttes (`DROPS` de `rain.ts`).
pub const DROPS: usize = 2200;
/// Boîte de pluie autour de la caméra, en m : la pluie y est recyclée, hors du champ elle n'existe pas (`BOX`).
pub const BOX_M: Vec3 = Vec3::new(70.0, 46.0, 110.0);
/// Part de la profondeur de la boîte qui est devant la caméra : le centre est à `0,3 × BOX_M.z` devant elle.
pub const AHEAD_FRACTION: f32 = 0.3;
/// Marge de l'`Aabb` (m) : traînée de 2,6 m au plus, inclinée de ~35° vers l'arrière à pleine vitesse.
pub const AABB_MARGIN_M: f32 = 3.0;
/// Au-delà de cette période (s), le temps du shader repart de zéro : `temps × vitesse` reste petit, donc précis au
/// centimètre même après six heures. Le saut décale la phase de toutes les gouttes, ce qu'une pluie tirée au hasard ne
/// montre pas.
pub const TIME_WRAP_S: f64 = 600.0;

const RAIN_SHADER: Handle<Shader> = uuid_handle!("6d1c8a30-7b34-4e59-8f0a-2c9e5d7b1a50");
const RAIN_SOURCE: &str = concat!(
    include_str!("../../../../../assets/shaders/cf-world-neon/weather_rain.wesl"),
    "\n",
    include_str!("../../../../../assets/shaders/cf-world-neon/field.wesl")
);
const RAIN_PATH: &str = "cf-world-neon/weather_rain.wesl";

/// Enregistre le shader de la pluie dans `Assets<Shader>`.
pub(super) fn embed_shader(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(RAIN_SHADER.id(), Shader::from_wesl(RAIN_SOURCE, RAIN_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, la pluie resterait invisible plutôt que de couper le show.
        tracing::error!(%error, "shader de la pluie non enregistré");
    }
}

/// Uniformes de la pluie. Même ordre que la structure `Rain` de `weather_rain.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct RainParams {
    /// `xyz` : la voiture en repère rendu (`uCar`), `w` : densité de gouttes affichées, 0 → 1 (`uDensity`).
    pub car: Vec4,
    /// `x` : temps (s, replié sur [`TIME_WRAP_S`]), `y` : inclinaison (`uTilt`).
    pub frame: Vec4,
    /// `xyz` : la boîte de pluie, m (`uBox`).
    pub box_size: Vec4,
    /// Glace (`uIce`).
    pub ice: Vec4,
    /// Teinte d'ambiance (`uMood`).
    pub mood: Vec4,
}

/// Matériau de la pluie : additif, sans lumière, sans écriture de profondeur.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct RainMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: RainParams,
    /// Le champ de la ville : les enseignes et les immeubles-vitrines éclairent les gouttes.
    #[uniform(1)]
    pub field: FieldParams,
}

impl Material for RainMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(RAIN_SHADER)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(RAIN_SHADER)
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
        // Le ruban est tourné vers la caméra, mais son enroulement dépend de la vue : on ne cull rien. `depthWrite: false`
        // de Sillage : une goutte additive ne masque rien derrière elle.
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}

/// Marqueur de l'entité de la pluie.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct Rain;

/// Les graines des gouttes (`Math.random` de Sillage, ici un RNG à graine) : `xy` position dans la boîte, `z` vitesse et
/// phase de chute, `w` seuil de densité.
#[must_use]
pub fn rain_seeds(seed: u64) -> Vec<[f32; 4]> {
    let mut rng = Pcg32::seed_from_u64(seed);
    let mut unit = || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
    (0..DROPS).map(|_| [unit(), unit(), unit(), unit()]).collect()
}

/// Le maillage de la pluie : un quad par goutte, sommets nuls, coin en `UV_0`, graine en `COLOR`.
#[must_use]
pub fn rain_mesh(seeds: &[[f32; 4]]) -> Mesh {
    let corners = [[-1.0_f32, 0.0], [1.0, 0.0], [1.0, 1.0], [-1.0, 1.0]];
    let mut uv = Vec::with_capacity(seeds.len() * 4);
    let mut color = Vec::with_capacity(seeds.len() * 4);
    let mut indices = Vec::with_capacity(seeds.len() * 6);
    for (i, seed) in seeds.iter().enumerate() {
        uv.extend(corners);
        color.extend([*seed; 4]);
        let base = (i * 4) as u32;
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; seeds.len() * 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(indices))
}

/// L'`Aabb` de la boîte de pluie, en repère de l'entité (posée au sol, au centre de la boîte en x et z).
#[must_use]
pub fn rain_aabb() -> Aabb {
    let half = BOX_M * 0.5 + Vec3::splat(AABB_MARGIN_M);
    Aabb {
        center: Vec3::new(0.0, BOX_M.y * 0.5, 0.0).into(),
        half_extents: half.into(),
    }
}

/// Poignée du matériau de la pluie, pour le réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct RainRig {
    pub(super) material: Handle<RainMaterial>,
}

/// Crée l'entité de la pluie, une fois, au démarrage. Cachée au repos.
pub(super) fn spawn_rain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<RainMaterial>>,
) {
    let material = materials.add(RainMaterial::default());
    commands.spawn((
        Name::new("pluie"),
        Rain,
        Mesh3d(meshes.add(rain_mesh(&rain_seeds(0x5a1_11a6e)))),
        MeshMaterial3d(material.clone()),
        rain_aabb(),
        Visibility::Hidden,
    ));
    commands.insert_resource(RainRig { material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_maillage_a_un_quad_par_goutte() {
        let mesh = rain_mesh(&rain_seeds(1));
        assert_eq!(mesh.count_vertices(), DROPS * 4);
        assert_eq!(mesh.indices().map(Indices::len), Some(DROPS * 6));
    }

    #[test]
    fn les_graines_sont_deterministes_et_dans_zero_un() {
        let (a, b) = (rain_seeds(9), rain_seeds(9));
        assert_eq!(a, b);
        assert!(a.iter().flatten().all(|v| (0.0..1.0).contains(v)));
        assert_ne!(a, rain_seeds(10));
    }

    #[test]
    fn l_aabb_couvre_la_boite_et_sa_marge() {
        let aabb = rain_aabb();
        let max = Vec3::from(aabb.center) + Vec3::from(aabb.half_extents);
        let min = Vec3::from(aabb.center) - Vec3::from(aabb.half_extents);
        assert!(min.x <= -BOX_M.x * 0.5 - 2.5 && max.x >= BOX_M.x * 0.5 + 2.5);
        assert!(
            min.y <= -2.5 && max.y >= BOX_M.y + 2.5,
            "de 0 à 46 m en y, marge comprise"
        );
        assert!(min.z <= -BOX_M.z * 0.5 - 2.5 && max.z >= BOX_M.z * 0.5 + 2.5);
    }
}
