//! Le modèle de la voiture côté processeur : maillages, images et arbre de nœuds prêts à être posés dans le monde.
//!
//! Construit à partir d'un [`Gltf`](super::glb::Gltf) décompressé, hors du fil principal (tâche de fond du plugin). Rien
//! n'est envoyé au GPU ici : [`CarModelAsset`] est transformé en entités par `spawn.rs` quand il est prêt, à l'ouverture du
//! show (règle 8 de `CLAUDE.md` : rien ne se charge pendant un show).

use bevy::asset::RenderAssetUsages;
use bevy::color::LinearRgba;
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor, ImageType,
};
use bevy::math::{Quat, Vec3};
use bevy::mesh::{Indices, Mesh, PrimitiveTopology};
use bevy::transform::components::Transform;
use std::collections::HashMap;

use super::doc::{MaterialDef, NodeDef};
use super::glb::{CarModelError, Gltf};

type Result<T> = std::result::Result<T, CarModelError>;

/// Rôle d'un matériau de la voiture : c'est le nom du matériau dans le GLB (`CYBERCAR.md` §6) qui le décide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaterialRole {
    /// `body_paint` : la carrosserie, repeinte à la teinte du thème.
    BodyPaint,
    /// `neon_primary` : sous-caisse, anneaux de roues, liserés (la lueur du moment).
    NeonPrimary,
    /// `neon_secondary` : bandes de portes, accents (la lueur mêlée de corail).
    NeonSecondary,
    /// `light_head` : les barres des phares.
    LightHead,
    /// `light_brake` : les barres des feux stop.
    LightBrake,
    /// `signal_FL`, `signal_FR`, `signal_RL`, `signal_RR` : les clignotants (côté, avant ou arrière).
    Signal(Side, Fore),
    /// `logo_rear_neon` : le logo arrière, qui pulse avec les basses.
    LogoRear,
    /// `glass` : pare-brise et vitres.
    Glass,
    /// `screen_speed`, `screen_radio`, `screen_mirror` : écrans de l'habitacle. Sillage y dessine compteur, autoradio et
    /// rétroviseur (jeu) ; ici ils restent éteints.
    Screen,
    /// `rain_glass` : gouttes sur le pare-brise, dessinées par la pluie du jeu (invisibles ici : alpha nul).
    RainGlass,
    /// Tout le reste : intérieur, chrome, pneus… rendu tel que le GLB le décrit.
    Plain,
}

/// Côté de la voiture, vu de son conducteur (`L` = gauche, volant du modèle à gauche).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// Gauche.
    Left,
    /// Droite.
    Right,
}

/// Avant ou arrière.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Fore {
    /// Avant (`signal_F*`).
    Front,
    /// Arrière (`signal_R*`).
    Rear,
}

impl MaterialRole {
    /// Rôle d'après le nom du matériau.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        match name {
            "body_paint" => Self::BodyPaint,
            "neon_primary" => Self::NeonPrimary,
            "neon_secondary" => Self::NeonSecondary,
            "light_head" => Self::LightHead,
            "light_brake" => Self::LightBrake,
            "signal_FL" => Self::Signal(Side::Left, Fore::Front),
            "signal_FR" => Self::Signal(Side::Right, Fore::Front),
            "signal_RL" => Self::Signal(Side::Left, Fore::Rear),
            "signal_RR" => Self::Signal(Side::Right, Fore::Rear),
            "logo_rear_neon" => Self::LogoRear,
            "glass" => Self::Glass,
            "screen_speed" | "screen_radio" | "screen_mirror" => Self::Screen,
            "rain_glass" => Self::RainGlass,
            _ => Self::Plain,
        }
    }
}

/// Mode de transparence d'un matériau.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alpha {
    /// Opaque.
    Opaque,
    /// Découpé au seuil donné.
    Mask(f32),
    /// Mélangé.
    Blend,
}

/// Un matériau du GLB, résolu : couleurs, indices d'images dans [`CarModelAsset::images`].
#[derive(Debug, Clone)]
pub struct MaterialData {
    /// Nom dans le GLB.
    pub name: String,
    /// Rôle déduit du nom.
    pub role: MaterialRole,
    /// Couleur de base (linéaire).
    pub base_color: LinearRgba,
    /// Métal.
    pub metallic: f32,
    /// Rugosité (perceptuelle).
    pub roughness: f32,
    /// Couleur émissive (linéaire), avant la force.
    pub emissive: LinearRgba,
    /// Force de l'émissif (`KHR_materials_emissive_strength`) : le bloom s'en empare au-delà de 1.
    pub emissive_strength: f32,
    /// Transparence.
    pub alpha: Alpha,
    /// Faces des deux côtés.
    pub double_sided: bool,
    /// Image de couleur (sRGB).
    pub base_color_image: Option<usize>,
    /// Image métal (B) / rugosité (G).
    pub metallic_roughness_image: Option<usize>,
    /// Image de normales.
    pub normal_image: Option<usize>,
    /// Masque de peinture (canal R de l'occlusion) : seules ces zones sont repeintes.
    pub mask_image: Option<usize>,
}

/// Un nœud de l'arbre : pivot, ancre ou pièce portant des maillages.
#[derive(Debug, Clone)]
pub struct NodeData {
    /// Nom dans le GLB : `spin_FL`, `steer_FR`, `anchor_headlight_L`, `logo_rear`, `body`…
    pub name: String,
    /// Transformation locale.
    pub transform: Transform,
    /// Enfants (indices dans [`CarModelAsset::nodes`]).
    pub children: Vec<usize>,
    /// Pièces : (indice dans `meshes`, indice dans `materials`).
    pub pieces: Vec<(usize, usize)>,
}

/// Le modèle décodé, rempli hors du fil principal (tâche de fond du plugin).
#[derive(Debug)]
pub struct CarModelAsset {
    /// Un maillage par primitive du GLB.
    pub meshes: Vec<Mesh>,
    /// Les images décodées (WebP), à l'indice que donnent les matériaux.
    pub images: Vec<Image>,
    /// Les matériaux.
    pub materials: Vec<MaterialData>,
    /// Les nœuds, aux indices du GLB.
    pub nodes: Vec<NodeData>,
    /// Les nœuds racines.
    pub roots: Vec<usize>,
}

impl CarModelAsset {
    /// Décode un GLB (`car-high.glb` ou `car-low.glb`).
    ///
    /// # Errors
    ///
    /// Si le fichier n'est pas un GLB lisible (voir [`CarModelError`]).
    pub fn from_glb(bytes: &[u8]) -> Result<Self> {
        let gltf = Gltf::from_glb(bytes)?;
        let mut images = ImageTable::default();
        let materials = gltf
            .materials()
            .iter()
            .map(|def| material(&gltf, def, &mut images))
            .collect::<Result<Vec<_>>>()?;
        let mut meshes = Vec::new();
        let mut pieces: Vec<Vec<(usize, usize)>> = Vec::new();
        for mesh in gltf.meshes() {
            let mut mine = Vec::new();
            for primitive in &mesh.primitives {
                if primitive.mode.unwrap_or(4) != 4 {
                    return Err(CarModelError::Unsupported("primitive qui n'est pas en triangles".into()));
                }
                let material = primitive
                    .material
                    .filter(|&m| m < materials.len())
                    .ok_or_else(|| CarModelError::Invalid("primitive sans matériau".into()))?;
                mine.push((meshes.len(), material));
                let mut mesh = build_mesh(&gltf, primitive)?;
                // Les normales de la peinture demandent des tangentes (sans elles, Bevy ignore la texture).
                if materials[material].normal_image.is_some()
                    && let Err(error) = mesh.generate_tangents()
                {
                    tracing::warn!(%error, "tangentes non calculées : texture de normales ignorée sur cette pièce");
                }
                meshes.push(mesh);
            }
            pieces.push(mine);
        }
        let nodes = gltf
            .nodes()
            .iter()
            .map(|node| NodeData {
                name: node.name.clone(),
                transform: transform(node),
                children: node.children.clone(),
                pieces: node.mesh.and_then(|m| pieces.get(m).cloned()).unwrap_or_default(),
            })
            .collect();
        Ok(Self {
            meshes,
            images: images.images,
            materials,
            nodes,
            roots: gltf.root_nodes().to_vec(),
        })
    }

    /// Indice du premier nœud de ce nom.
    #[must_use]
    pub fn node(&self, name: &str) -> Option<usize> {
        self.nodes.iter().position(|n| n.name == name)
    }
}

fn transform(node: &NodeDef) -> Transform {
    Transform {
        translation: node.translation.map_or(Vec3::ZERO, Vec3::from),
        rotation: node.rotation.map_or(Quat::IDENTITY, |[x, y, z, w]| Quat::from_xyzw(x, y, z, w)),
        scale: node.scale.map_or(Vec3::ONE, Vec3::from),
    }
}

fn build_mesh(gltf: &Gltf, primitive: &super::doc::PrimitiveDef) -> Result<Mesh> {
    let attribute = |name: &str| {
        primitive
            .attributes
            .get(name)
            .copied()
            .ok_or_else(|| CarModelError::Invalid(format!("attribut {name} absent")))
    };
    let triples = |values: Vec<f32>| values.as_chunks::<3>().0.to_vec();
    let positions = triples(gltf.floats(attribute("POSITION")?)?);
    let normals = triples(gltf.floats(attribute("NORMAL")?)?);
    // Les maillages gardent leur copie du monde principal : le culling (`Aabb`) et les mesures la relisent.
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    let count = positions.len();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    if let Ok(uv) = attribute("TEXCOORD_0") {
        let uvs: Vec<[f32; 2]> = gltf.floats(uv)?.as_chunks::<2>().0.to_vec();
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    let indices = gltf.indices(
        primitive
            .indices
            .ok_or_else(|| CarModelError::Invalid("primitive sans indices".into()))?,
    )?;
    let in_bounds = indices.iter().all(|&i| (i as usize) < count);
    if !in_bounds {
        return Err(CarModelError::Invalid("indice hors du maillage".into()));
    }
    mesh.insert_indices(Indices::U32(indices));
    Ok(mesh)
}

/// Les images décodées, dédoublonnées par (image du GLB, sRGB ou non) : une image peut servir de couleur et de masque.
#[derive(Default)]
struct ImageTable {
    images: Vec<Image>,
    known: HashMap<(usize, bool), usize>,
}

impl ImageTable {
    fn get(&mut self, gltf: &Gltf, texture: usize, srgb: bool) -> Result<usize> {
        let image = gltf
            .textures()
            .get(texture)
            .and_then(super::doc::TextureDef::image)
            .ok_or_else(|| CarModelError::Invalid(format!("texture {texture} sans image")))?;
        if let Some(&index) = self.known.get(&(image, srgb)) {
            return Ok(index);
        }
        let def = gltf
            .images()
            .get(image)
            .ok_or_else(|| CarModelError::Invalid(format!("image {image} absente")))?;
        let view = def
            .buffer_view
            .ok_or_else(|| CarModelError::Unsupported("image hors d'une vue de tampon".into()))?;
        let bytes = gltf.view_bytes(view)?;
        let decoded = Image::from_buffer(
            bytes,
            ImageType::MimeType(&def.mime_type),
            CompressedImageFormats::NONE,
            srgb,
            sampler(gltf, texture),
            // Le GPU suffit : la copie du monde principal serait 16 Mo par image de 2048² gaspillés.
            RenderAssetUsages::RENDER_WORLD,
        )
        .map_err(|e| CarModelError::Image {
            name: def.name.clone(),
            reason: e.to_string(),
        })?;
        self.images.push(decoded);
        self.known.insert((image, srgb), self.images.len() - 1);
        Ok(self.images.len() - 1)
    }
}

/// L'échantillonneur du GLB : répétition, filtrage linéaire avec mipmaps (9987) pour la texture qui en demande.
fn sampler(gltf: &Gltf, texture: usize) -> ImageSampler {
    let def = gltf
        .textures()
        .get(texture)
        .and_then(|t| t.sampler)
        .and_then(|s| gltf.samplers().get(s));
    let mode = |wrap: Option<u32>| match wrap {
        Some(33071) => ImageAddressMode::ClampToEdge,
        Some(33648) => ImageAddressMode::MirrorRepeat,
        _ => ImageAddressMode::Repeat,
    };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode(def.and_then(|d| d.wrap_s)),
        address_mode_v: mode(def.and_then(|d| d.wrap_t)),
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..ImageSamplerDescriptor::default()
    })
}

fn material(gltf: &Gltf, def: &MaterialDef, images: &mut ImageTable) -> Result<MaterialData> {
    let pbr = &def.pbr_metallic_roughness;
    let [r, g, b, a] = pbr.base_color_factor.unwrap_or([1.0; 4]);
    let [er, eg, eb] = def.emissive_factor.unwrap_or([0.0; 3]);
    let mut image = |texture: Option<super::doc::TextureRef>, srgb: bool| {
        texture.map(|t| images.get(gltf, t.index, srgb)).transpose()
    };
    Ok(MaterialData {
        name: def.name.clone(),
        role: MaterialRole::from_name(&def.name),
        base_color: LinearRgba::new(r, g, b, a),
        metallic: pbr.metallic_factor.unwrap_or(1.0),
        roughness: pbr.roughness_factor.unwrap_or(1.0),
        emissive: LinearRgba::new(er, eg, eb, 1.0),
        emissive_strength: def.emissive_strength(),
        alpha: match def.alpha_mode.as_deref() {
            Some("BLEND") => Alpha::Blend,
            Some("MASK") => Alpha::Mask(def.alpha_cutoff.unwrap_or(0.5)),
            _ => Alpha::Opaque,
        },
        double_sided: def.double_sided,
        base_color_image: image(pbr.base_color_texture, true)?,
        metallic_roughness_image: image(pbr.metallic_roughness_texture, false)?,
        normal_image: image(def.normal_texture, false)?,
        mask_image: image(def.occlusion_texture, false)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str) -> CarModelAsset {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/car").join(name);
        let bytes = std::fs::read(&path).expect("fichier du modèle");
        CarModelAsset::from_glb(&bytes).expect("modèle lisible")
    }

    #[test]
    fn le_modele_porte_les_noeuds_et_materiaux_que_le_code_pilote() {
        let car = load("car-low.glb");
        for name in [
            "spin_FL",
            "spin_FR",
            "spin_RL",
            "spin_RR",
            "steer_FL",
            "steer_FR",
            "anchor_headlight_L",
            "anchor_headlight_R",
            "anchor_underglow",
            "logo_rear",
            "body",
        ] {
            assert!(car.node(name).is_some(), "nœud {name} absent");
        }
        let has = |role: MaterialRole| car.materials.iter().any(|m| m.role == role);
        assert!(has(MaterialRole::BodyPaint) && has(MaterialRole::NeonPrimary) && has(MaterialRole::LogoRear));
        assert!(has(MaterialRole::Signal(Side::Left, Fore::Rear)) && has(MaterialRole::LightBrake));
        // La peinture a ses quatre images : couleur, métal/rugosité, normales et masque.
        let paint = car.materials.iter().find(|m| m.role == MaterialRole::BodyPaint).expect("peinture");
        assert!(paint.base_color_image.is_some() && paint.mask_image.is_some() && paint.normal_image.is_some());
        assert!(paint.metallic_roughness_image.is_some());
        // Les néons sont des émissifs forts : c'est ce que le bloom reprend.
        let neon = car.materials.iter().find(|m| m.role == MaterialRole::NeonPrimary).expect("néon");
        assert!((neon.emissive_strength - 4.0).abs() < 1e-6);
    }

    #[test]
    fn le_masque_de_peinture_est_decode_en_non_srgb_et_la_couleur_en_srgb() {
        use bevy::render::render_resource::TextureFormat;
        let car = load("car-low.glb");
        let paint = car.materials.iter().find(|m| m.role == MaterialRole::BodyPaint).expect("peinture");
        let color = &car.images[paint.base_color_image.expect("couleur")];
        let mask = &car.images[paint.mask_image.expect("masque")];
        assert_eq!(color.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(mask.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        assert_eq!(color.width(), 1024, "variante légère : textures 1k (CYBERCAR.md §7)");
    }
}
