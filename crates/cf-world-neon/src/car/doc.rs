//! Les types du JSON glTF que la voiture lit (`serde`). Seuls les champs utiles sont déclarés.

use serde::Deserialize;

/// Un tampon (`buffers[i]`).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BufferDef {
    /// Taille en octets.
    #[serde(default)]
    pub byte_length: usize,
}

/// Les extensions d'une vue de tampon : seule `EXT_meshopt_compression` nous intéresse.
#[derive(Debug, Deserialize, Default)]
pub(super) struct ViewExtensions {
    #[serde(rename = "EXT_meshopt_compression")]
    pub(super) meshopt: Option<MeshoptDef>,
}

/// `EXT_meshopt_compression` d'une vue : où sont les octets compressés, et comment les décoder.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct MeshoptDef {
    pub(super) buffer: usize,
    #[serde(default)]
    pub(super) byte_offset: usize,
    pub(super) byte_length: usize,
    pub(super) byte_stride: usize,
    pub(super) count: usize,
    pub(super) mode: String,
    #[serde(default)]
    pub(super) filter: Option<String>,
}

/// Une vue de tampon (`bufferViews[i]`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewDef {
    /// Tampon porteur.
    pub buffer: usize,
    /// Début dans le tampon (octets).
    #[serde(default)]
    pub byte_offset: usize,
    /// Taille (octets).
    pub byte_length: usize,
    /// Pas entre deux éléments, 0 : les éléments se suivent.
    #[serde(default)]
    pub byte_stride: usize,
    #[serde(default)]
    pub(super) extensions: ViewExtensions,
}

/// Un accesseur (`accessors[i]`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccessorDef {
    /// Vue de tampon (absente : un accesseur de zéros, que la voiture n'utilise pas).
    pub buffer_view: Option<usize>,
    /// Début dans la vue (octets).
    #[serde(default)]
    pub byte_offset: usize,
    /// Type de composante glTF (5120 `i8`, 5121 `u8`, 5122 `i16`, 5123 `u16`, 5125 `u32`, 5126 `f32`).
    pub component_type: u32,
    /// Les entiers valent `valeur / max` (−1..1 ou 0..1).
    #[serde(default)]
    pub normalized: bool,
    /// Nombre d'éléments.
    pub count: usize,
    /// `SCALAR`, `VEC2`, `VEC3`, `VEC4`.
    #[serde(rename = "type")]
    pub kind: String,
}

/// Une image (`images[i]`) : des octets WebP dans une vue de tampon.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageDef {
    /// Nom (diagnostic).
    #[serde(default)]
    pub name: String,
    /// Vue de tampon qui porte le fichier.
    pub buffer_view: Option<usize>,
    /// Type MIME.
    #[serde(default)]
    pub mime_type: String,
}

/// `EXT_texture_webp` d'une texture : l'image à utiliser à la place de `source`.
#[derive(Debug, Deserialize, Default)]
pub(super) struct TextureExtensions {
    #[serde(rename = "EXT_texture_webp")]
    webp: Option<WebpDef>,
}

#[derive(Debug, Deserialize)]
pub(super) struct WebpDef {
    source: usize,
}

/// Une texture (`textures[i]`).
#[derive(Debug, Deserialize, Default)]
pub struct TextureDef {
    /// Échantillonneur.
    pub sampler: Option<usize>,
    /// Image PNG/JPEG (absente quand c'est une image WebP).
    pub source: Option<usize>,
    #[serde(default)]
    extensions: TextureExtensions,
}

impl TextureDef {
    /// L'image de la texture : celle de `EXT_texture_webp`, sinon `source`.
    #[must_use]
    pub fn image(&self) -> Option<usize> {
        self.extensions.webp.as_ref().map(|w| w.source).or(self.source)
    }
}

/// Un échantillonneur (`samplers[i]`) : seul le mode de répétition compte ici.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SamplerDef {
    /// 10497 : répétition (défaut de glTF), 33071 : bord, 33648 : miroir.
    pub wrap_s: Option<u32>,
    /// Voir `wrap_s`.
    pub wrap_t: Option<u32>,
}

/// Référence à une texture dans un matériau.
#[derive(Debug, Deserialize, Clone, Copy)]
pub struct TextureRef {
    /// Indice dans `textures`.
    pub index: usize,
}

/// Le bloc PBR d'un matériau.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PbrDef {
    /// Couleur de base (linéaire, RGBA).
    pub base_color_factor: Option<[f32; 4]>,
    /// Texture de couleur de base.
    pub base_color_texture: Option<TextureRef>,
    /// Facteur de métal.
    pub metallic_factor: Option<f32>,
    /// Facteur de rugosité.
    pub roughness_factor: Option<f32>,
    /// Texture métal (B) / rugosité (G).
    pub metallic_roughness_texture: Option<TextureRef>,
}

/// `KHR_materials_emissive_strength`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmissiveStrength {
    /// Multiplicateur de l'émissif.
    pub emissive_strength: f32,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct MaterialExtensions {
    #[serde(rename = "KHR_materials_emissive_strength")]
    emissive_strength: Option<EmissiveStrength>,
}

/// Un matériau (`materials[i]`).
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDef {
    /// Nom : c'est lui qui désigne le rôle (`neon_primary`, `light_head`, `body_paint`…).
    #[serde(default)]
    pub name: String,
    /// Bloc PBR.
    #[serde(default)]
    pub pbr_metallic_roughness: PbrDef,
    /// Texture de normales.
    pub normal_texture: Option<TextureRef>,
    /// Texture d'occlusion : sur `body_paint`, son canal R est le masque de peinture.
    pub occlusion_texture: Option<TextureRef>,
    /// Couleur émissive (linéaire).
    pub emissive_factor: Option<[f32; 3]>,
    /// `OPAQUE`, `MASK` ou `BLEND`.
    pub alpha_mode: Option<String>,
    /// Seuil de `MASK`.
    pub alpha_cutoff: Option<f32>,
    /// Faces des deux côtés.
    #[serde(default)]
    pub double_sided: bool,
    #[serde(default)]
    extensions: MaterialExtensions,
}

impl MaterialDef {
    /// Force de l'émissif (`KHR_materials_emissive_strength`, 1 par défaut).
    #[must_use]
    pub fn emissive_strength(&self) -> f32 {
        self.extensions.emissive_strength.as_ref().map_or(1.0, |e| e.emissive_strength)
    }
}

/// Une primitive d'un maillage.
#[derive(Debug, Deserialize)]
pub struct PrimitiveDef {
    /// Accesseurs par attribut (`POSITION`, `NORMAL`, `TEXCOORD_0`).
    pub attributes: std::collections::BTreeMap<String, usize>,
    /// Accesseur des indices.
    pub indices: Option<usize>,
    /// Matériau.
    pub material: Option<usize>,
    /// Mode de dessin : 4 (triangles) par défaut.
    pub mode: Option<u32>,
}

/// Un maillage (`meshes[i]`).
#[derive(Debug, Deserialize)]
pub struct MeshDef {
    /// Ses primitives, une par matériau.
    pub primitives: Vec<PrimitiveDef>,
}

/// Un nœud de la scène (`nodes[i]`).
#[derive(Debug, Deserialize, Default)]
pub struct NodeDef {
    /// Nom : les pivots (`spin_FL`, `steer_FR`…) et les ancres (`anchor_headlight_L`…) se retrouvent par lui.
    #[serde(default)]
    pub name: String,
    /// Enfants (indices de nœuds).
    #[serde(default)]
    pub children: Vec<usize>,
    /// Maillage porté.
    pub mesh: Option<usize>,
    /// Position locale.
    pub translation: Option<[f32; 3]>,
    /// Rotation locale (quaternion x, y, z, w).
    pub rotation: Option<[f32; 4]>,
    /// Échelle locale.
    pub scale: Option<[f32; 3]>,
}

/// Une scène (`scenes[i]`).
#[derive(Debug, Deserialize, Default)]
pub struct SceneDef {
    /// Nœuds racines.
    #[serde(default)]
    pub nodes: Vec<usize>,
}

/// Le document glTF : ce que la voiture lit du JSON.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(super) struct Document {
    #[serde(default)]
    pub(super) buffers: Vec<BufferDef>,
    #[serde(default)]
    pub(super) buffer_views: Vec<ViewDef>,
    #[serde(default)]
    pub(super) accessors: Vec<AccessorDef>,
    #[serde(default)]
    pub(super) images: Vec<ImageDef>,
    #[serde(default)]
    pub(super) textures: Vec<TextureDef>,
    #[serde(default)]
    pub(super) samplers: Vec<SamplerDef>,
    #[serde(default)]
    pub(super) materials: Vec<MaterialDef>,
    #[serde(default)]
    pub(super) meshes: Vec<MeshDef>,
    #[serde(default)]
    pub(super) nodes: Vec<NodeDef>,
    #[serde(default)]
    pub(super) scenes: Vec<SceneDef>,
    pub(super) scene: Option<usize>,
}
