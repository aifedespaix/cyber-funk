//! Les maillages des véhicules : les engins de Kenney (GLB), la police (le racer et ses gyrophares), la moto
//! procédurale. Construits une fois, au démarrage, depuis `assets/models/crafts/` : rien ne se charge pendant un show.

pub mod bike;
mod builder;
pub mod crafts;

use std::path::Path;

use bevy::mesh::Mesh;

pub use builder::MeshBuilder;

use super::fleet::ModelDims;
use super::variant::{BIKE, VARIANT_COUNT, file_of};
use crate::car::CarModelError;

/// Un modèle prêt à dessiner.
#[derive(Debug, Clone)]
pub struct CraftModel {
    /// Maillage unique : positions, normales, surface de chaque sommet dans le canal rouge de la couleur.
    pub mesh: Mesh,
    /// Demi-largeur dans le repère du modèle (non mis à l'échelle).
    pub half_width: f32,
    /// Abscisse z de l'arrière, dans le repère du modèle.
    pub rear: f32,
}

impl CraftModel {
    /// Les dimensions que la flotte garde pour poser les traînées.
    #[must_use]
    pub fn dims(&self) -> ModelDims {
        ModelDims {
            half_width: self.half_width,
            rear: self.rear,
        }
    }
}

/// Pourquoi un modèle n'a pas pu être construit.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// Le fichier n'a pas pu être lu.
    #[error("lecture de {path} : {source}")]
    Read {
        /// Chemin du fichier.
        path: String,
        /// Erreur du système de fichiers.
        #[source]
        source: std::io::Error,
    },
    /// Le GLB est illisible.
    #[error(transparent)]
    Glb(#[from] CarModelError),
    /// Le GLB ne porte aucun triangle.
    #[error("aucun triangle dans le modèle")]
    Empty,
}

/// Construit les dix modèles depuis `dir` (`assets/models/crafts`). Un fichier manquant ou illisible est signalé et
/// remplacé par un engin de secours : le trafic existe quand même, la ville ne reste pas vide.
#[must_use]
pub fn load_models(dir: &Path) -> Vec<CraftModel> {
    (0..VARIANT_COUNT as u8)
        .map(|variant| {
            if variant == BIKE {
                return bike::bike_model();
            }
            let police = variant == super::variant::POLICE;
            let Some(name) = file_of(variant) else {
                return crafts::fallback_craft(police);
            };
            let path = dir.join(format!("craft_{name}.glb"));
            read(&path, police).unwrap_or_else(|error| {
                tracing::warn!(%error, variant, "modèle de véhicule absent : engin de secours");
                crafts::fallback_craft(police)
            })
        })
        .collect()
}

fn read(path: &Path, police: bool) -> Result<CraftModel, ModelError> {
    let bytes = std::fs::read(path).map_err(|source| ModelError::Read {
        path: path.display().to_string(),
        source,
    })?;
    crafts::build_craft(&bytes, police)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::variant::{CRAFT_FILES, CRAFT_LENGTH, POLICE};
    use bevy::math::Vec3;

    fn crafts_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models/crafts")
    }

    fn bounds(model: &CraftModel) -> (Vec3, Vec3) {
        let positions = model
            .mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .and_then(|a| a.as_float3())
            .expect("positions");
        positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| {
            (lo.min(Vec3::from(*p)), hi.max(Vec3::from(*p)))
        })
    }

    #[test]
    fn les_huit_glb_de_kenney_se_lisent_et_sont_recentres_a_la_longueur_de_reference() {
        for (i, name) in CRAFT_FILES.iter().enumerate() {
            let bytes = std::fs::read(crafts_dir().join(format!("craft_{name}.glb"))).expect("fichier");
            let model = crafts::build_craft(&bytes, false).expect(name);
            let (lo, hi) = bounds(&model);
            // Le sol à y = 0 (les phares et bandes dépassent de quelques centimètres), centré en x, de la bonne longueur.
            assert!(lo.y.abs() < 0.02, "{name} : bas à {}", lo.y);
            assert!((hi.z - lo.z - CRAFT_LENGTH).abs() < 0.1, "{name} : longueur {}", hi.z - lo.z);
            assert!(lo.x < 0.0 && hi.x > 0.0, "{name}");
            assert!(model.half_width > 0.05 && model.rear > 0.5, "{name} : {} {}", model.half_width, model.rear);
            assert!(model.mesh.count_vertices() > 100, "variante {i}");
        }
    }

    #[test]
    fn la_police_porte_ses_gyrophares_et_pas_les_autres() {
        let bytes = std::fs::read(crafts_dir().join("craft_racer.glb")).expect("fichier");
        let count_kind = |model: &CraftModel, k: f32| {
            model
                .mesh
                .attribute(Mesh::ATTRIBUTE_COLOR)
                .map(|a| match a {
                    bevy::mesh::VertexAttributeValues::Float32x4(v) => v.iter().filter(|c| c[0] == k).count(),
                    _ => 0,
                })
                .unwrap_or(0)
        };
        let civilian = crafts::build_craft(&bytes, false).expect("racer");
        let police = crafts::build_craft(&bytes, true).expect("police");
        assert_eq!(count_kind(&civilian, crate::traffic::variant::kind::BEACON_LEFT), 0);
        assert_eq!(count_kind(&police, crate::traffic::variant::kind::BEACON_LEFT), 36);
        assert_eq!(count_kind(&police, crate::traffic::variant::kind::BEACON_RIGHT), 36);
        assert_eq!(file_of(POLICE), Some("racer"));
    }

    #[test]
    fn load_models_rend_dix_modeles_meme_sans_dossier() {
        let models = load_models(Path::new("/inexistant"));
        assert_eq!(models.len(), VARIANT_COUNT);
        assert!(models.iter().all(|m| m.mesh.count_vertices() > 0));
    }
}
