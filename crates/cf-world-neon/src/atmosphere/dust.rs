//! Les poussières d'ambiance (DA §3.2) : trois coquilles autour de la caméra, un seul maillage statique, un seul
//! matériau, tout dans le vertex shader (`assets/shaders/cf-world-neon/atmosphere_dust.wesl`). Un draw call, aucune mise
//! à jour du maillage, aucun état, aucune allocation par image. Calqué sur la pluie ([`crate::weather::rain`]).
//!
//! # Culling (règle 6 de `CLAUDE.md`)
//!
//! Pas de `NoFrustumCulling`. Les sommets sont tous nuls : on pose un `Aabb` explicite, le cube de la coquille lointaine.
//! L'entité suit la caméra par son `AbsolutePosition` ; la caméra est donc toujours dans l'`Aabb`. Densité nulle (palier
//! bas, mode calme) : l'entité est cachée.

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

use super::light::{ATMOSPHERE_WESL, SceneLightParams};

/// Nombre de grains par coquille au palier haut (proche, médiane, lointaine).
pub const SHELL_COUNTS: [usize; 3] = [200, 800, 1500];
/// Nombre total de grains du maillage.
pub const GRAINS: usize = SHELL_COUNTS[0] + SHELL_COUNTS[1] + SHELL_COUNTS[2];
/// Part de grains affichés par coquille, par palier de poussières (haut, moyen, bas) : 200/800/1500, 100/300/500,
/// 0/100/0.
pub const TIER_SHARES: [[f32; 3]; 3] = [[1.0, 1.0, 1.0], [0.5, 0.375, 1.0 / 3.0], [0.0, 0.125, 0.0]];
/// Rayons intérieurs des coquilles, m.
pub const INNER_M: [f32; 3] = [0.5, 3.0, 20.0];
/// Rayons extérieurs des coquilles, m.
pub const OUTER_M: [f32; 3] = [3.0, 20.0, 80.0];
/// Taille à l'écran (px) : minimum et maximum par coquille.
pub const SIZE_PX: [(f32, f32); 3] = [(4.0, 12.0), (1.5, 4.0), (1.0, 2.0)];
/// Le temps du shader repart de zéro au bout de cette période (précision des `f32`, comme la pluie).
pub const TIME_WRAP_S: f64 = 600.0;
/// Gain de luminance d'un grain par rapport à la brume locale.
pub const GAIN: f32 = 1.6;

const DUST_SHADER: Handle<Shader> = uuid_handle!("5e1b8c2a-94d7-4f36-a0e2-7c3d9b6f1a48");
const DUST_SOURCE: &str = concat!(
    include_str!("../../../../assets/shaders/cf-world-neon/atmosphere_dust.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/atmosphere.wesl")
);
const DUST_PATH: &str = "cf-world-neon/atmosphere_dust.wesl";

/// Enregistre le shader des poussières dans `Assets<Shader>`.
pub(super) fn embed_shader(shaders: &mut Assets<Shader>) {
    debug_assert!(DUST_SOURCE.ends_with(ATMOSPHERE_WESL));
    if let Err(error) = shaders.insert(DUST_SHADER.id(), Shader::from_wesl(DUST_SOURCE, DUST_PATH)) {
        // Impossible avec un identifiant UUID ; au pire les poussières restent invisibles, le show continue.
        tracing::error!(%error, "shader des poussières non enregistré");
    }
}

/// Uniformes des poussières. Même ordre que la structure `Dust` de `atmosphere_dust.wesl`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct DustParams {
    /// `x` : temps (s, replié), `y` : vent (m/s), `z` : enveloppe des transitoires, `w` : vitesse du sujet (m/s).
    pub frame: Vec4,
    /// `xyz` : part de grains affichés par coquille, `w` : gain de luminance.
    pub density: Vec4,
    /// Rayons intérieurs ; `w` : 1 si le scintillement est permis.
    pub inner: Vec4,
    /// Rayons extérieurs.
    pub outer: Vec4,
    /// Tailles minimales, px.
    pub size_min: Vec4,
    /// Tailles maximales, px.
    pub size_max: Vec4,
}

/// Matériau des poussières : additif, sans lumière, sans écriture de profondeur.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct DustMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: DustParams,
    /// L'éclairage de scène : un grain prend la couleur de la brume locale.
    #[uniform(9)]
    pub scene: SceneLightParams,
}

impl Material for DustMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(DUST_SHADER)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(DUST_SHADER)
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
        // Quads face caméra : rien à éliminer ; un grain additif ne masque rien derrière lui.
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false.into();
        }
        Ok(())
    }
}

/// Marqueur de l'entité des poussières.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(WorldLayer, AbsolutePosition)]
pub struct Dust;

/// Un grain : sa position dans la boîte de sa coquille, son seuil de densité, sa coquille et son identifiant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grain {
    /// `xyz` : position 0..1 dans le cube de la coquille, `w` : seuil de densité 0..1.
    pub seed: [f32; 4],
    /// Coquille (0, 1, 2).
    pub shell: u8,
    /// Identifiant 0..1 (taille, phase de dérive, scintillement).
    pub id: f32,
}

/// Les grains, tirés d'une graine. Le seuil de densité d'un grain est son rang dans sa coquille : au palier moyen, ce
/// sont toujours les mêmes grains qui restent (pas de papillotement quand la qualité change).
#[must_use]
pub fn grains(seed: u64) -> Vec<Grain> {
    let mut rng = Pcg32::seed_from_u64(seed);
    let mut unit = || (rng.next_u32() >> 8) as f32 / (1u32 << 24) as f32;
    let mut out = Vec::with_capacity(GRAINS);
    for (shell, count) in SHELL_COUNTS.into_iter().enumerate() {
        for i in 0..count {
            out.push(Grain {
                seed: [unit(), unit(), unit(), i as f32 / count as f32],
                shell: shell as u8,
                id: unit(),
            });
        }
    }
    out
}

/// Le maillage : un quad par grain, sommets nuls, coin en `UV_0`, coquille et identifiant en `UV_1`, graine en `COLOR`.
#[must_use]
pub fn dust_mesh(grains: &[Grain]) -> Mesh {
    let corners = [[-1.0_f32, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];
    let mut uv = Vec::with_capacity(grains.len() * 4);
    let mut info = Vec::with_capacity(grains.len() * 4);
    let mut color = Vec::with_capacity(grains.len() * 4);
    let mut indices = Vec::with_capacity(grains.len() * 6);
    for (i, grain) in grains.iter().enumerate() {
        uv.extend(corners);
        info.extend([[f32::from(grain.shell), grain.id]; 4]);
        color.extend([grain.seed; 4]);
        let base = (i * 4) as u32;
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; grains.len() * 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, info)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, color)
        .with_inserted_indices(Indices::U32(indices))
}

/// L'`Aabb` des poussières, en repère de l'entité (posée sur la caméra) : le cube de la coquille lointaine.
#[must_use]
pub fn dust_aabb() -> Aabb {
    Aabb {
        center: Vec3::ZERO.into(),
        half_extents: Vec3::splat(OUTER_M[2] + 1.0).into(),
    }
}

/// Part de grains affichés par coquille : le palier vient de `QualityLevers::particle_density` (1 : haut, ≥ 0,5 :
/// moyen, en dessous : bas). Mode calme : aucune poussière.
#[must_use]
pub fn shell_shares(particle_density: f32, calm: bool) -> [f32; 3] {
    if calm || !particle_density.is_finite() {
        return [0.0; 3];
    }
    let tier = if particle_density >= 0.95 {
        0
    } else if particle_density >= 0.5 {
        1
    } else {
        2
    };
    TIER_SHARES[tier]
}

/// Les uniformes d'une image.
#[must_use]
pub fn dust_params(
    time_s: f64,
    wind_mps: f32,
    transient: f32,
    speed_mps: f32,
    shares: [f32; 3],
    twinkle: bool,
) -> DustParams {
    let v3 = |a: [f32; 3], w: f32| Vec4::new(a[0], a[1], a[2], w);
    DustParams {
        frame: Vec4::new(
            (time_s % TIME_WRAP_S) as f32,
            wind_mps,
            transient.clamp(0.0, 1.0),
            speed_mps.max(0.0),
        ),
        density: v3(shares, GAIN),
        inner: v3(INNER_M, if twinkle { 1.0 } else { 0.0 }),
        outer: v3(OUTER_M, 0.0),
        size_min: v3([SIZE_PX[0].0, SIZE_PX[1].0, SIZE_PX[2].0], 0.0),
        size_max: v3([SIZE_PX[0].1, SIZE_PX[1].1, SIZE_PX[2].1], 0.0),
    }
}

/// Poignée du matériau des poussières, pour le réécrire sans rien créer.
#[derive(Resource, Debug)]
pub struct DustRig {
    pub(super) material: Handle<DustMaterial>,
}

/// Crée l'entité des poussières, une fois, au démarrage.
pub(super) fn spawn_dust(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<DustMaterial>>,
) {
    let material = materials.add(DustMaterial::default());
    commands.spawn((
        Name::new("poussières"),
        Dust,
        Mesh3d(meshes.add(dust_mesh(&grains(0x00d0_57c3)))),
        MeshMaterial3d(material.clone()),
        dust_aabb(),
        Visibility::Hidden,
    ));
    commands.insert_resource(DustRig { material });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_maillage_a_un_quad_par_grain_et_les_bons_attributs() {
        let g = grains(1);
        assert_eq!(g.len(), GRAINS);
        let mesh = dust_mesh(&g);
        assert_eq!(mesh.count_vertices(), GRAINS * 4);
        for attribute in [Mesh::ATTRIBUTE_UV_0, Mesh::ATTRIBUTE_UV_1, Mesh::ATTRIBUTE_COLOR] {
            assert!(mesh.attribute(attribute).is_some());
        }
    }

    #[test]
    fn les_paliers_affichent_les_nombres_de_la_direction_artistique() {
        let g = grains(1);
        let shown = |shares: [f32; 3]| {
            let mut n = [0usize; 3];
            for grain in &g {
                if grain.seed[3] < shares[usize::from(grain.shell)] {
                    n[usize::from(grain.shell)] += 1;
                }
            }
            n
        };
        assert_eq!(shown(shell_shares(1.0, false)), [200, 800, 1500]);
        assert_eq!(shown(shell_shares(0.6, false)), [100, 300, 500]);
        assert_eq!(shown(shell_shares(0.3, false)), [0, 100, 0]);
        assert_eq!(shown(shell_shares(1.0, true)), [0, 0, 0]);
    }

    #[test]
    fn les_grains_sont_deterministes_et_dans_leur_boite() {
        assert_eq!(grains(7), grains(7));
        for grain in grains(7) {
            assert!(grain.seed.iter().all(|v| (0.0..1.0).contains(v)));
            assert!(grain.shell < 3 && (0.0..1.0).contains(&grain.id));
        }
    }

    #[test]
    fn le_shader_declare_le_bloc_dans_l_ordre_des_uniformes() {
        let mut last = 0;
        for field in [
            "frame: vec4",
            "density: vec4",
            "inner: vec4",
            "outer: vec4",
            "size_min: vec4",
            "size_max: vec4",
        ] {
            let at = DUST_SOURCE.find(field).unwrap_or(0);
            assert!(at > last, "champ `{field}` absent ou hors d'ordre");
            last = at;
        }
        // Hachage entier : aucun `sin` dans le scintillement (le seul `sin` autorisé est celui de la dérive).
        assert!(DUST_SOURCE.contains("fn dust_hash(x_in: u32)"));
    }

    #[test]
    fn le_temps_se_replie_et_les_entrees_sont_bornees() {
        let p = dust_params(TIME_WRAP_S + 1.5, 1.2, 7.0, -3.0, [1.0; 3], true);
        assert!((p.frame.x - 1.5).abs() < 1e-4);
        assert!((p.frame.z - 1.0).abs() < f32::EPSILON && p.frame.w.abs() < f32::EPSILON);
    }
}
