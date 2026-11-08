//! Vignettes des logos de DJ pour la colonne de gauche.
//!
//! Chaque logo est lu et réduit une seule fois, la première fois qu'une carte le demande (donc à l'ouverture, avant
//! que le show ne joue), puis gardé en texture. Un logo introuvable ou illisible n'est pas une erreur : la carte s'en
//! passe, et le fichier n'est pas relu à chaque image.

use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;
use bevy_egui::egui;

/// Côté maximal d'une vignette (pixels) : assez pour un affichage à 40 points sur un écran dense.
const THUMBNAIL_PX: u32 = 96;

/// Les textures des logos déjà chargés, par chemin sous `assets/`. `None` : le chargement a échoué, on n'insiste pas.
#[derive(Resource, Default)]
pub(crate) struct LogoCache {
    textures: HashMap<String, Option<egui::TextureHandle>>,
}

impl std::fmt::Debug for LogoCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LogoCache")
            .field("logos", &self.textures.len())
            .finish()
    }
}

impl LogoCache {
    /// La texture du logo `logo` (chemin sous `assets_dir`), chargée au premier appel.
    pub(crate) fn texture(&mut self, ctx: &egui::Context, assets_dir: &Path, logo: &str) -> Option<egui::TextureId> {
        let entry = self
            .textures
            .entry(logo.to_owned())
            .or_insert_with(|| load(ctx, assets_dir, logo));
        entry.as_ref().map(egui::TextureHandle::id)
    }
}

fn load(ctx: &egui::Context, assets_dir: &Path, logo: &str) -> Option<egui::TextureHandle> {
    let path = assets_dir.join(logo);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "logo du DJ introuvable");
            return None;
        }
    };
    let Some(image) = decode_thumbnail(&bytes, THUMBNAIL_PX) else {
        tracing::warn!(path = %path.display(), "logo du DJ illisible (PNG ou JPEG attendu)");
        return None;
    };
    Some(ctx.load_texture(format!("logo:{logo}"), image, egui::TextureOptions::LINEAR))
}

/// Décode une image PNG ou JPEG et la réduit à `max` pixels de côté au plus (jamais agrandie).
pub(crate) fn decode_thumbnail(bytes: &[u8], max: u32) -> Option<egui::ColorImage> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let thumbnail = if decoded.width() > max || decoded.height() > max {
        decoded.thumbnail(max, max)
    } else {
        decoded
    };
    let rgba = thumbnail.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 90, 255]));
        let mut bytes = Vec::new();
        image
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .expect("encodage PNG");
        bytes
    }

    #[test]
    fn une_petite_image_garde_sa_taille() {
        let image = decode_thumbnail(&png(4, 2), 96).expect("image");
        assert_eq!(image.size, [4, 2]);
    }

    #[test]
    fn une_grande_image_est_reduite_en_gardant_ses_proportions() {
        let image = decode_thumbnail(&png(400, 200), 96).expect("image");
        assert_eq!(image.size, [96, 48]);
    }

    #[test]
    fn un_fichier_qui_n_est_pas_une_image_donne_rien() {
        assert!(decode_thumbnail(b"pas une image", 96).is_none());
        assert!(decode_thumbnail(&[], 96).is_none());
    }
}
