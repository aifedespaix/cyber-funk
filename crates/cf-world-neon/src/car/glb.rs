//! Lecture d'un GLB `EXT_meshopt_compression` + `EXT_texture_webp` (le format de `car-high.glb` / `car-low.glb`).
//!
//! `bevy_gltf` ne sait lire ni l'une ni l'autre de ces extensions (« ❌ » dans sa table). Sillage les chargeait avec
//! `MeshoptDecoder` de three.js ; ici on décompresse nous-mêmes les vues de tampon (bibliothèque `meshoptimizer`) et on
//! laisse les images WebP à `bevy_image` (feature `webp`). Le résultat ([`Gltf`]) est un glTF « à plat » : des tampons
//! bruts, des accesseurs lisibles par [`Gltf::floats`] et [`Gltf::indices`]. Pas de Bevy ici : tout se teste sans GPU.
//!
//! Le fichier de la voiture n'utilise qu'un sous-ensemble de glTF (un GLB, un tampon binaire, des accesseurs quantifiés
//! `KHR_mesh_quantization` : positions en `i16` normalisés, normales en `i8`, UV en `u16`) ; le reste est refusé par une
//! erreur claire plutôt que mal interprété.

use super::doc::{AccessorDef, Document, ImageDef, MaterialDef, MeshDef, MeshoptDef, NodeDef, SamplerDef, TextureDef};

/// Erreurs de lecture du modèle de la voiture.
#[derive(Debug, thiserror::Error)]
pub enum CarModelError {
    /// Le fichier n'est pas un GLB valide.
    #[error("GLB invalide : {0}")]
    Container(&'static str),
    /// Le fichier n'a pas pu être lu.
    #[error("lecture de {path} : {source}")]
    Read {
        /// Chemin du fichier.
        path: String,
        /// Erreur du système de fichiers.
        #[source]
        source: std::io::Error,
    },
    /// Le JSON glTF est illisible.
    #[error("JSON glTF illisible : {0}")]
    Json(#[from] serde_json::Error),
    /// Une donnée référencée n'existe pas ou sort de son tampon.
    #[error("glTF incohérent : {0}")]
    Invalid(String),
    /// Une fonctionnalité de glTF que le modèle de la voiture n'utilise pas.
    #[error("glTF non pris en charge : {0}")]
    Unsupported(String),
    /// La bibliothèque `meshoptimizer` a refusé des données.
    #[error("décompression meshopt : {0}")]
    Meshopt(String),
    /// Une image n'a pas pu être décodée.
    #[error("image « {name} » : {reason}")]
    Image {
        /// Nom de l'image dans le glTF.
        name: String,
        /// Raison donnée par le décodeur.
        reason: String,
    },
}

type Result<T> = std::result::Result<T, CarModelError>;

/// Un glTF décompressé : le document et ses tampons à plat.
#[derive(Debug)]
pub struct Gltf {
    doc: Document,
    /// Un tampon par entrée de `buffers` ; le tampon « de repli » de meshopt y est rempli par la décompression.
    buffers: Vec<Vec<u8>>,
}

/// Sépare les deux morceaux (JSON, binaire) d'un GLB.
fn split_glb(bytes: &[u8]) -> Result<(&[u8], &[u8])> {
    let word = |at: usize| -> Result<usize> {
        let raw = bytes
            .get(at..at + 4)
            .ok_or(CarModelError::Container("fichier tronqué"))?;
        Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize)
    };
    if bytes.get(..4) != Some(b"glTF") {
        return Err(CarModelError::Container("signature « glTF » absente"));
    }
    if word(4)? != 2 {
        return Err(CarModelError::Container("seule la version 2 est lue"));
    }
    let (mut json, mut bin) = (None, None);
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let length = word(at)?;
        let kind = word(at + 4)?;
        let body = bytes
            .get(at + 8..at + 8 + length)
            .ok_or(CarModelError::Container("morceau tronqué"))?;
        match kind {
            0x4E4F_534A => json = Some(body),
            0x004E_4942 => bin = Some(body),
            _ => {}
        }
        // Les morceaux sont alignés sur 4 octets.
        at += 8 + length.div_ceil(4) * 4;
    }
    Ok((
        json.ok_or(CarModelError::Container("morceau JSON absent"))?,
        bin.ok_or(CarModelError::Container("morceau binaire absent"))?,
    ))
}

impl Gltf {
    /// Lit un GLB et décompresse ses vues `EXT_meshopt_compression`.
    ///
    /// # Errors
    ///
    /// Si le fichier est tronqué, si le JSON est illisible, si une référence sort de son tampon ou si `meshoptimizer`
    /// refuse des données.
    pub fn from_glb(bytes: &[u8]) -> Result<Self> {
        let (json, bin) = split_glb(bytes)?;
        let doc: Document = serde_json::from_slice(json)?;
        // Le tampon 0 est le morceau binaire ; un autre tampon sans `uri` est le tampon de repli de meshopt, vide dans
        // le fichier (« fallback: true ») : on le remplit en décompressant.
        let mut buffers = Vec::with_capacity(doc.buffers.len());
        for (i, def) in doc.buffers.iter().enumerate() {
            buffers.push(if i == 0 { bin.to_vec() } else { vec![0; def.byte_length] });
        }
        let mut gltf = Self { doc, buffers };
        gltf.decompress()?;
        Ok(gltf)
    }

    fn decompress(&mut self) -> Result<()> {
        for index in 0..self.doc.buffer_views.len() {
            let view = &self.doc.buffer_views[index];
            let Some(def) = &view.extensions.meshopt else { continue };
            let compressed = self
                .buffers
                .get(def.buffer)
                .and_then(|b| b.get(def.byte_offset..def.byte_offset + def.byte_length))
                .ok_or_else(|| CarModelError::Invalid(format!("vue {index} : octets compressés hors tampon")))?;
            let decoded = decode_view(def, compressed)?;
            let (buffer, offset) = (view.buffer, view.byte_offset);
            let target = self
                .buffers
                .get_mut(buffer)
                .and_then(|b| b.get_mut(offset..offset + decoded.len()))
                .ok_or_else(|| CarModelError::Invalid(format!("vue {index} : sortie hors tampon")))?;
            target.copy_from_slice(&decoded);
        }
        Ok(())
    }

    /// Octets d'une vue de tampon.
    ///
    /// # Errors
    ///
    /// Si la vue ou son tampon n'existe pas, ou si elle en sort.
    pub fn view_bytes(&self, view: usize) -> Result<&[u8]> {
        let def = self
            .doc
            .buffer_views
            .get(view)
            .ok_or_else(|| CarModelError::Invalid(format!("vue {view} absente")))?;
        self.buffers
            .get(def.buffer)
            .and_then(|b| b.get(def.byte_offset..def.byte_offset + def.byte_length))
            .ok_or_else(|| CarModelError::Invalid(format!("vue {view} hors tampon")))
    }

    /// Lit un accesseur en `f32` : les entiers normalisés sont ramenés à −1..1 ou 0..1, les autres convertis tels quels.
    /// Les composantes se suivent (`count × composantes`).
    ///
    /// # Errors
    ///
    /// Si l'accesseur sort de sa vue ou a un type que la voiture n'utilise pas.
    pub fn floats(&self, accessor: usize) -> Result<Vec<f32>> {
        let def = self.accessor(accessor)?;
        let width = components(&def.kind)?;
        let size = component_size(def.component_type)?;
        let (bytes, stride) = self.accessor_bytes(def, width * size)?;
        let mut out = Vec::with_capacity(def.count * width);
        for element in 0..def.count {
            for c in 0..width {
                let at = element * stride + c * size;
                let raw = bytes
                    .get(at..at + size)
                    .ok_or_else(|| CarModelError::Invalid(format!("accesseur {accessor} hors vue")))?;
                out.push(read_component(raw, def.component_type, def.normalized)?);
            }
        }
        Ok(out)
    }

    /// Lit un accesseur d'indices (`u8`, `u16` ou `u32`).
    ///
    /// # Errors
    ///
    /// Si l'accesseur n'est pas scalaire entier ou sort de sa vue.
    pub fn indices(&self, accessor: usize) -> Result<Vec<u32>> {
        let def = self.accessor(accessor)?;
        let size = component_size(def.component_type)?;
        if def.kind != "SCALAR" || !matches!(def.component_type, 5121 | 5123 | 5125) {
            return Err(CarModelError::Unsupported(format!(
                "indices de type {} {}",
                def.kind, def.component_type
            )));
        }
        let (bytes, stride) = self.accessor_bytes(def, size)?;
        (0..def.count)
            .map(|i| {
                let raw = bytes
                    .get(i * stride..i * stride + size)
                    .ok_or_else(|| CarModelError::Invalid(format!("indices {accessor} hors vue")))?;
                Ok(match size {
                    1 => u32::from(raw[0]),
                    2 => u32::from(u16::from_le_bytes([raw[0], raw[1]])),
                    _ => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
                })
            })
            .collect()
    }

    fn accessor(&self, index: usize) -> Result<&AccessorDef> {
        self.doc
            .accessors
            .get(index)
            .ok_or_else(|| CarModelError::Invalid(format!("accesseur {index} absent")))
    }

    /// Octets d'un accesseur (à partir de son début) et pas entre deux éléments.
    fn accessor_bytes(&self, def: &AccessorDef, element_size: usize) -> Result<(&[u8], usize)> {
        let view = def
            .buffer_view
            .ok_or_else(|| CarModelError::Unsupported("accesseur sans vue de tampon".into()))?;
        let bytes = self.view_bytes(view)?;
        let stride = match self.doc.buffer_views[view].byte_stride {
            0 => element_size,
            s => s,
        };
        let bytes = bytes
            .get(def.byte_offset..)
            .ok_or_else(|| CarModelError::Invalid("décalage d'accesseur hors vue".into()))?;
        Ok((bytes, stride))
    }

    /// Accesseurs, vues, images… (lecture seule du document).
    #[must_use]
    pub fn images(&self) -> &[ImageDef] {
        &self.doc.images
    }

    /// Les textures.
    #[must_use]
    pub fn textures(&self) -> &[TextureDef] {
        &self.doc.textures
    }

    /// Les échantillonneurs.
    #[must_use]
    pub fn samplers(&self) -> &[SamplerDef] {
        &self.doc.samplers
    }

    /// Les matériaux.
    #[must_use]
    pub fn materials(&self) -> &[MaterialDef] {
        &self.doc.materials
    }

    /// Les maillages.
    #[must_use]
    pub fn meshes(&self) -> &[MeshDef] {
        &self.doc.meshes
    }

    /// Les nœuds.
    #[must_use]
    pub fn nodes(&self) -> &[NodeDef] {
        &self.doc.nodes
    }

    /// Les nœuds racines de la scène par défaut.
    #[must_use]
    pub fn root_nodes(&self) -> &[usize] {
        let scene = self.doc.scene.unwrap_or(0);
        self.doc.scenes.get(scene).map_or(&[], |s| s.nodes.as_slice())
    }
}

fn components(kind: &str) -> Result<usize> {
    Ok(match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        other => return Err(CarModelError::Unsupported(format!("type d'accesseur {other}"))),
    })
}

fn component_size(component_type: u32) -> Result<usize> {
    Ok(match component_type {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        other => return Err(CarModelError::Unsupported(format!("type de composante {other}"))),
    })
}

fn read_component(raw: &[u8], component_type: u32, normalized: bool) -> Result<f32> {
    let n = |value: f32, max: f32| if normalized { (value / max).max(-1.0) } else { value };
    Ok(match component_type {
        5120 => n(f32::from(i8::from_le_bytes([raw[0]])), 127.0),
        5121 => n(f32::from(raw[0]), 255.0),
        5122 => n(f32::from(i16::from_le_bytes([raw[0], raw[1]])), 32767.0),
        5123 => n(f32::from(u16::from_le_bytes([raw[0], raw[1]])), 65535.0),
        5125 => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as f32,
        5126 => f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
        other => return Err(CarModelError::Unsupported(format!("type de composante {other}"))),
    })
}

/// Décompresse une vue : `count × byte_stride` octets.
fn decode_view(def: &MeshoptDef, compressed: &[u8]) -> Result<Vec<u8>> {
    match (def.mode.as_str(), def.byte_stride) {
        ("ATTRIBUTES", 4) => decode_attributes::<4>(def, compressed),
        ("ATTRIBUTES", 8) => decode_attributes::<8>(def, compressed),
        ("ATTRIBUTES", 12) => decode_attributes::<12>(def, compressed),
        ("ATTRIBUTES", 16) => decode_attributes::<16>(def, compressed),
        ("TRIANGLES", 2) => {
            let indices: Vec<u16> = meshopt::decode_index_buffer(compressed, def.count).map_err(meshopt_error)?;
            Ok(indices.iter().flat_map(|i| i.to_le_bytes()).collect())
        }
        ("TRIANGLES", 4) => {
            let indices: Vec<u32> = meshopt::decode_index_buffer(compressed, def.count).map_err(meshopt_error)?;
            Ok(indices.iter().flat_map(|i| i.to_le_bytes()).collect())
        }
        (mode, stride) => Err(CarModelError::Unsupported(format!(
            "meshopt {mode} avec un pas de {stride} octets"
        ))),
    }
}

/// Un élément de sommet de `N` octets : `[u8; N]` n'implémente `Default` que pour des `N` fixes, pas en générique.
#[derive(Clone, Copy)]
#[repr(transparent)]
struct Element<const N: usize>([u8; N]);

impl<const N: usize> Default for Element<N> {
    fn default() -> Self {
        Self([0; N])
    }
}

/// Décompresse les attributs d'une vue (éléments de `N` octets) puis applique le filtre de meshopt, s'il y en a un.
fn decode_attributes<const N: usize>(def: &MeshoptDef, compressed: &[u8]) -> Result<Vec<u8>> {
    let decoded: Vec<Element<N>> = meshopt::decode_vertex_buffer(compressed, def.count).map_err(meshopt_error)?;
    let mut elements: Vec<[u8; N]> = decoded.into_iter().map(|e| e.0).collect();
    match (def.filter.as_deref().unwrap_or("NONE"), N) {
        ("NONE", _) => {}
        // Normales en octaèdre : quatre composantes de 8 ou de 16 bits.
        ("OCTAHEDRAL", 4) => {
            let mut typed: Vec<[i8; 4]> = elements
                .iter()
                .map(|e| [0, 1, 2, 3].map(|i| i8::from_le_bytes([e[i]])))
                .collect();
            meshopt::decode_filter_oct(&mut typed);
            for (element, value) in elements.iter_mut().zip(&typed) {
                for (byte, component) in element.iter_mut().zip(value) {
                    *byte = component.to_le_bytes()[0];
                }
            }
        }
        ("OCTAHEDRAL", 8) => {
            let mut typed: Vec<[i16; 4]> = elements
                .iter()
                .map(|e| [0, 2, 4, 6].map(|i| i16::from_le_bytes([e[i], e[i + 1]])))
                .collect();
            meshopt::decode_filter_oct(&mut typed);
            for (element, value) in elements.iter_mut().zip(&typed) {
                let (pairs, _) = element.as_chunks_mut::<2>();
                for (bytes, component) in pairs.iter_mut().zip(value) {
                    *bytes = component.to_le_bytes();
                }
            }
        }
        (filter, stride) => {
            return Err(CarModelError::Unsupported(format!(
                "filtre meshopt {filter} sur {stride} octets"
            )));
        }
    }
    Ok(elements.iter().flatten().copied().collect())
}

fn meshopt_error(error: meshopt::Error) -> CarModelError {
    CarModelError::Meshopt(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glb(name: &str) -> Gltf {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/models/car")
            .join(name);
        let bytes = std::fs::read(&path).expect("fichier du modèle");
        Gltf::from_glb(&bytes).expect("GLB lisible")
    }

    /// Les deux fichiers livrés se décompressent, et leurs accesseurs ont du sens : positions quantifiées dans
    /// [−1, 1], normales unitaires (après le filtre octaédrique), indices dans les bornes.
    #[test]
    fn les_glb_de_la_voiture_se_decompressent() {
        for name in ["car-high.glb", "car-low.glb"] {
            let gltf = glb(name);
            assert_eq!(gltf.nodes().len(), 54, "{name}");
            assert_eq!(gltf.images().len(), 8, "{name}");
            let mut triangles = 0;
            for mesh in gltf.meshes() {
                for primitive in &mesh.primitives {
                    let position = gltf.floats(primitive.attributes["POSITION"]).expect("positions");
                    let normal = gltf.floats(primitive.attributes["NORMAL"]).expect("normales");
                    assert_eq!(position.len(), normal.len());
                    assert!(
                        position.iter().all(|v| (-1.0..=1.0).contains(v)),
                        "{name} : positions quantifiées"
                    );
                    for n in normal.as_chunks::<3>().0 {
                        let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                        assert!((length - 1.0).abs() < 0.03, "{name} : normale de longueur {length}");
                    }
                    let indices = gltf
                        .indices(primitive.indices.expect("indices"))
                        .expect("indices lisibles");
                    assert_eq!(indices.len() % 3, 0);
                    assert!(
                        indices.iter().all(|&i| (i as usize) < position.len() / 3),
                        "{name} : indices en bornes"
                    );
                    triangles += indices.len() / 3;
                }
            }
            // ~31 000 triangles en haute définition (la dirty car et ses tubes néon), ~25 000 en basse.
            let expected = if name.contains("high") {
                25_000..40_000
            } else {
                20_000..30_000
            };
            assert!(expected.contains(&triangles), "{name} : {triangles} triangles");
        }
    }

    #[test]
    fn un_fichier_qui_n_est_pas_un_glb_est_refuse() {
        assert!(matches!(
            Gltf::from_glb(b"pas un glb"),
            Err(CarModelError::Container(_))
        ));
        assert!(matches!(Gltf::from_glb(&[]), Err(CarModelError::Container(_))));
    }
}
