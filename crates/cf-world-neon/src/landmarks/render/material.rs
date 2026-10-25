//! Le matériau des immeubles-vitrines : un seul type, un shader par [`Look`] (fichiers `landmark_*.wesl`), un seul bloc
//! d'uniformes. Les shaders sont **embarqués** dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
//!
//! - **Un type, treize programmes** : le [`Look`] est la clé de spécialisation du pipeline (`bind_group_data`), qui choisit le
//!   shader. Comme la préchauffe générique (`warm_material`) ne distingue pas deux matériaux de même type qui ne diffèrent que
//!   par leur clé, ce module déclare **sa propre préchauffe** ([`warm`]) : une copie de chaque rendu devant la caméra du monde.
//! - **Un bloc d'uniformes** : la palette de la scène (onze couleurs), l'image (temps, pouls, basses…), puis quatre `Vec4` de
//!   paramètres propres au rendu ; chaque shader documente les siens en tête de fichier.
//! - **Additif ou opaque** : les rendus de lumière pure (`Look::additive`) ajoutent leur couleur à la scène, sans écrire la
//!   profondeur ; les deux faces se voient. Les rendus de façade poussent leur profondeur devant le bloc d'immeuble (les
//!   `polygonOffset` de Sillage).

use bevy::asset::{Asset, Assets, Handle, uuid_handle};
use bevy::camera::primitives::MeshAabb;
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey, MeshMaterial3d};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, DepthBiasState, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::Shader;
use cf_render::Layer;
use cf_render::warmup::WarmEntity;

use crate::landmarks::part::Look;

/// Les uniformes d'un rendu. Même ordre et mêmes types que la structure `Params` de `landmark_common.wesl` : tout en `Vec4`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct LandmarkParams {
    /// La palette de la scène, dans l'ordre de `SceneColor::ALL` (lumière linéaire).
    pub palette: [Vec4; 11],
    /// Teinte d'ambiance de la pièce (`uMood`).
    pub mood: Vec4,
    /// `x` : temps (s, borné), `y` : pouls borné par le limiteur, `z` : basses, `w` : médiums.
    pub frame: Vec4,
    /// `x` : aigus, `y` : temps musical continu, `z` : intensité, `w` : phase.
    pub audio: Vec4,
    /// Paramètres propres au rendu.
    pub a: Vec4,
    /// Idem.
    pub b: Vec4,
    /// Idem.
    pub c: Vec4,
    /// Idem.
    pub d: Vec4,
}

/// La clé de spécialisation : le rendu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LandmarkKey {
    /// Le rendu.
    pub look: Look,
}

/// Le matériau d'une pièce d'immeuble-vitrine.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq)]
#[bind_group_data(LandmarkKey)]
pub struct LandmarkMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: LandmarkParams,
    /// La texture de la pièce (atlas ERROR, grille de l'arcade), s'il y en a une.
    #[texture(1)]
    #[sampler(2)]
    pub map: Option<Handle<Image>>,
    /// Le rendu (clé du pipeline).
    pub look: Look,
}

impl LandmarkMaterial {
    /// Un matériau de ce rendu, sans texture, aux uniformes nuls.
    #[must_use]
    pub fn new(look: Look, map: Option<Handle<Image>>) -> Self {
        Self {
            params: LandmarkParams::default(),
            map,
            look,
        }
    }
}

impl From<&LandmarkMaterial> for LandmarkKey {
    fn from(material: &LandmarkMaterial) -> Self {
        Self { look: material.look }
    }
}

/// Les identifiants des shaders, dans l'ordre de [`Look::ALL`].
const SHADERS: [Handle<Shader>; 13] = [
    uuid_handle!("92851024-e7ad-467d-827d-a290abb27245"),
    uuid_handle!("bcadd81e-36a1-4302-b32c-61f762837545"),
    uuid_handle!("98f898f4-be73-4701-b468-bfecaf050b22"),
    uuid_handle!("d3b1c5b3-0dab-44ab-b3d4-1a7aaf8e0033"),
    uuid_handle!("56c35301-1faf-4343-b252-b0be58c11496"),
    uuid_handle!("85492473-877f-47ea-800a-70adae0dd3b2"),
    uuid_handle!("56f3689b-a85e-41bf-8c60-4351ab9decf2"),
    uuid_handle!("664afc80-a3ec-4458-b176-938af09a9179"),
    uuid_handle!("66d16899-e5d4-4104-baa9-8f4e35944315"),
    uuid_handle!("634424bb-ba6a-4297-ade8-399bf740a4c0"),
    uuid_handle!("d3c984ad-a192-441e-a157-15aa86217ddc"),
    uuid_handle!("af4f5097-7046-47f0-ae3d-a969265ad43d"),
    uuid_handle!("26e503c7-09b9-44c5-b75c-b8aeb708d487"),
];

const COMMON: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_common.wesl");
const GLYPH: &str = include_str!("../../../../../assets/shaders/cf-world-neon/glyph.wesl");
const MATRIX_CORE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_matrix_core.wesl");
const MATRIX_WALL: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_matrix_wall.wesl");
const MATRIX_SPROUTS: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_matrix_sprouts.wesl");
const LAVA_CORE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_lava_core.wesl");
const LAVA_FACADE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_lava_facade.wesl");
const LAVA_POOL: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_lava_pool.wesl");
const ERROR_SIGN: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_error_sign.wesl");
const ERROR_FACADE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_error_facade.wesl");
const BEAM: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_beam.wesl");
const ARCADE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_arcade.wesl");
const BOARD: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_board.wesl");
const ARCH: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_arch.wesl");
const MECHA_PANEL: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_mecha_panel.wesl");
const MECHA_FACE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_mecha_face.wesl");
const IRON: &str = include_str!("../../../../../assets/shaders/cf-world-neon/landmark_iron.wesl");

/// Le rang d'un rendu dans [`Look::ALL`].
fn rank(look: Look) -> usize {
    Look::ALL.iter().position(|l| *l == look).unwrap_or(0)
}

/// Les morceaux de texte d'un rendu, dans l'ordre : le socle d'abord (les `import` doivent venir en tête).
#[must_use]
pub fn pieces(look: Look) -> Vec<&'static str> {
    let tail: &[&'static str] = match look {
        Look::MatrixWall => &[MATRIX_CORE, MATRIX_WALL],
        Look::MatrixSprouts => &[MATRIX_CORE, MATRIX_SPROUTS],
        Look::LavaFacade => &[LAVA_CORE, LAVA_FACADE],
        Look::LavaPool => &[LAVA_CORE, LAVA_POOL],
        Look::ErrorSign => &[GLYPH, ERROR_SIGN],
        Look::ErrorFacade => &[ERROR_FACADE],
        Look::Beam => &[BEAM],
        Look::ArcadeHolo => &[ARCADE],
        Look::BoardFacade => &[BOARD],
        Look::ArchVoxels => &[ARCH],
        Look::MechaPanel => &[MECHA_PANEL],
        Look::MechaFace => &[MECHA_FACE],
        Look::Iron => &[IRON],
    };
    std::iter::once(COMMON).chain(tail.iter().copied()).collect()
}

/// Le nom logique d'un shader (messages d'erreur de Bevy).
fn path(look: Look) -> String {
    format!("cf-world-neon/landmark#{look:?}.wesl")
}

/// Enregistre les shaders dans `Assets<Shader>`. Démarrage seulement : le texte est assemblé une fois.
pub(crate) fn embed_shaders(shaders: &mut Assets<Shader>) {
    for look in Look::ALL {
        let source = pieces(look).join("\n");
        let handle = &SHADERS[rank(look)];
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path(look))) {
            // Impossible avec un identifiant UUID ; si cela arrivait, ce rendu resterait invisible plutôt que de couper
            // le show.
            tracing::error!(%error, ?look, "shader d'immeuble-vitrine non enregistré");
        }
    }
}

impl Material for LandmarkMaterial {
    fn alpha_mode(&self) -> AlphaMode {
        if self.look.additive() {
            AlphaMode::Add
        } else {
            AlphaMode::Opaque
        }
    }

    /// Pas de passage de profondeur ni d'ombres : les pièces sont entièrement émissives, et un immeuble-vitrine n'occulte
    /// rien (l'occlusion GPU est celle de la ville).
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
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        let look = key.bind_group_data.look;
        let shader = SHADERS[rank(look)].clone();
        descriptor.vertex.shader = shader.clone();
        if let Some(fragment) = descriptor.fragment.as_mut() {
            fragment.shader = shader;
        }
        if look.double_sided() {
            descriptor.primitive.cull_mode = None;
        }
        if look.decal()
            && let Some(depth) = descriptor.depth_stencil.as_mut()
        {
            // `polygonOffset(-2, -2)` de Sillage : la façade passe devant le bloc d'immeuble (profondeur inversée : un
            // décalage positif rapproche).
            depth.bias = DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            };
        }
        Ok(())
    }
}

/// La préchauffe : une copie minuscule de chaque rendu devant la caméra du monde, pour que son pipeline soit compilé à
/// l'ouverture (règle 8). Les pièces cachées du pool ne sont jamais vues avant leur première apparition en plein show.
pub(crate) fn warm(world: &mut World) -> Vec<Entity> {
    let mut seen = std::collections::HashSet::new();
    let mut picked: Vec<(Handle<Mesh>, Handle<LandmarkMaterial>, f32)> = Vec::new();
    {
        let meshes = world.resource::<Assets<Mesh>>();
        let Some(materials) = world.get_resource::<Assets<LandmarkMaterial>>() else {
            return Vec::new();
        };
        let mut query = world.try_query::<(&Mesh3d, &MeshMaterial3d<LandmarkMaterial>)>();
        let Some(query) = query.as_mut() else {
            return Vec::new();
        };
        for (mesh, material) in query.iter(world) {
            let Some(look) = materials.get(&material.0).map(|m| m.look) else {
                continue;
            };
            if !seen.insert(look) {
                continue;
            }
            let radius = meshes
                .get(&mesh.0)
                .and_then(|m| m.get_aabb())
                .map_or(0.0, |aabb| aabb.half_extents.length());
            picked.push((mesh.0.clone(), material.0.clone(), radius));
        }
    }
    picked
        .into_iter()
        .map(|(mesh, material, radius)| {
            world
                .spawn((
                    Name::new("préchauffe : immeuble-vitrine"),
                    WarmEntity { radius },
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    Layer::World.render_layers(),
                    Transform::default(),
                    Visibility::Visible,
                ))
                .id()
        })
        .collect::<Vec<Entity>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cf_render::SceneColor;

    #[test]
    fn chaque_rendu_a_un_identifiant_unique() {
        let mut ids: Vec<_> = SHADERS.iter().map(|h| h.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), Look::ALL.len());
        assert_eq!(SHADERS.len(), Look::ALL.len());
    }

    #[test]
    fn chaque_shader_commence_par_le_socle_et_declare_ses_entrees() {
        for look in Look::ALL {
            let pieces = pieces(look);
            assert_eq!(pieces[0], COMMON, "{look:?}");
            let text = pieces.join("\n");
            assert!(text.contains("@vertex"), "{look:?} : pas de vertex");
            assert!(text.contains("@fragment"), "{look:?} : pas de fragment");
            // Les imports sont tous en tête : aucun après la première déclaration du socle.
            let first_decl = text.find("struct Params").unwrap_or(0);
            assert!(
                !text[first_decl..].contains("\nimport "),
                "{look:?} : import après une déclaration"
            );
        }
    }

    #[test]
    fn les_rangs_de_la_palette_du_shader_sont_ceux_de_scene_color() {
        for (name, color) in [
            ("C_INK", SceneColor::Ink),
            ("C_VELVET", SceneColor::Velvet),
            ("C_MINT", SceneColor::Mint),
            ("C_AZURE", SceneColor::Azure),
            ("C_VIOLET", SceneColor::Violet),
            ("C_ROSE", SceneColor::Rose),
            ("C_AMBER", SceneColor::Amber),
            ("C_ICE", SceneColor::Ice),
            ("C_ALARM", SceneColor::Alarm),
            ("C_LIME", SceneColor::Lime),
            ("C_SUN", SceneColor::Sun),
        ] {
            let needle = format!("const {name}: u32 = {}u;", color as usize);
            assert!(
                COMMON.contains(&needle),
                "landmark_common.wesl : `{needle}` introuvable"
            );
        }
    }

    #[test]
    fn le_bloc_d_uniformes_a_les_champs_du_shader() {
        for needle in [
            "palette: array<vec4<f32>, 11>",
            "mood: vec4<f32>",
            "frame: vec4<f32>",
            "audio: vec4<f32>",
            "a: vec4<f32>",
            "b: vec4<f32>",
            "c: vec4<f32>",
            "d: vec4<f32>",
        ] {
            assert!(COMMON.contains(needle), "{needle}");
        }
    }

    #[test]
    fn le_mode_de_melange_suit_le_rendu() {
        let additive = LandmarkMaterial::new(Look::Beam, None);
        let opaque = LandmarkMaterial::new(Look::BoardFacade, None);
        assert!(matches!(additive.alpha_mode(), AlphaMode::Add));
        assert!(matches!(opaque.alpha_mode(), AlphaMode::Opaque));
        assert_ne!(LandmarkKey::from(&additive), LandmarkKey::from(&opaque));
    }
}
