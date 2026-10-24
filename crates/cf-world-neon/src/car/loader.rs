//! Le chargeur d'assets du modèle : lit le GLB, le décompresse et le décode hors du fil principal.

use bevy::asset::io::Reader;
use bevy::asset::{AssetLoader, LoadContext};
use bevy::reflect::TypePath;

use super::glb::CarModelError;
use super::model::CarModelAsset;

/// Charge `car-high.glb` / `car-low.glb` en [`CarModelAsset`].
///
/// Les GLB de la voiture sont compressés (`EXT_meshopt_compression`, `EXT_texture_webp`) : `bevy_gltf` ne les lit pas.
/// L'extension `glb` est aussi celle de `GltfLoader` : Bevy choisit d'après le type demandé (`load::<CarModelAsset>`),
/// les deux chargeurs ne se marchent pas dessus.
#[derive(Debug, Default, Clone, Copy, TypePath)]
pub struct CarModelLoader;

impl AssetLoader for CarModelLoader {
    type Asset = CarModelAsset;
    type Settings = ();
    type Error = CarModelError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _settings: &Self::Settings,
        _load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        CarModelAsset::from_glb(&bytes)
    }

    fn extensions(&self) -> &[&str] {
        &["glb"]
    }
}
