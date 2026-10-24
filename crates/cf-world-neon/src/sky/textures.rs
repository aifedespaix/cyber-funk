//! Les textures des astres : JPEG embarqués dans le binaire, décodés au démarrage, avec leur chaîne de mips.
//!
//! Règle 8 de `CLAUDE.md` : aucun chargement d'asset pendant un show. Les sept textures (≈ 350 Ko) sont donc compilées dans
//! le binaire (`include_bytes!`), décodées une fois à l'ouverture, sans passer par l'`AssetServer` (pas de fichier à
//! retrouver, pas de chargement paresseux, pas d'attente du premier affichage).
//!
//! Sillage laissait three.js générer les mips (`anisotropy = 4`). Bevy ne le fait pas : sans mips, une planète de 17 m à
//! 900 m (quelques pixels) scintillerait en tournant. La chaîne est donc calculée ici, une fois, par moyenne de 2×2.

use bevy::asset::RenderAssetUsages;
use bevy::image::{
    CompressedImageFormats, Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor, ImageType,
};

/// Anisotropie de l'échantillonnage (`texture.anisotropy` de Sillage).
const ANISOTROPY: u16 = 4;

/// Octets d'une texture de planète, par nom (`assets/textures/planets/<nom>.jpg`).
#[must_use]
pub fn embedded(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "moon" => include_bytes!("../../../../assets/textures/planets/moon.jpg"),
        "gas-ember" => include_bytes!("../../../../assets/textures/planets/gas-ember.jpg"),
        "ocean" => include_bytes!("../../../../assets/textures/planets/ocean.jpg"),
        "gas-azure" => include_bytes!("../../../../assets/textures/planets/gas-azure.jpg"),
        "ice" => include_bytes!("../../../../assets/textures/planets/ice.jpg"),
        "gas-rose" => include_bytes!("../../../../assets/textures/planets/gas-rose.jpg"),
        "gas-teal" => include_bytes!("../../../../assets/textures/planets/gas-teal.jpg"),
        _ => return None,
    })
}

/// Pourquoi une texture n'a pas pu être préparée.
#[derive(Debug, thiserror::Error)]
pub enum TextureError {
    /// Le nom n'est pas celui d'une texture embarquée.
    #[error("texture de planète inconnue : « {0} »")]
    Unknown(String),
    /// Le JPEG ne se décode pas.
    #[error("texture « {name} » illisible : {source}")]
    Decode {
        /// Nom de la texture.
        name: String,
        /// Erreur du décodeur.
        source: bevy::image::TextureError,
    },
    /// Le format décodé n'est pas du RGBA 8 bits (impossible avec un JPEG ; refusé plutôt que mal copié).
    #[error("texture « {0} » : format décodé inattendu")]
    Format(String),
}

/// Décode une texture embarquée et lui ajoute sa chaîne de mips.
///
/// # Errors
///
/// [`TextureError`] si le nom est inconnu ou le JPEG illisible : l'astre est alors omis, comme dans Sillage (« un astre
/// dont la texture manque est simplement omis »), et le ciel continue.
pub fn load(name: &str) -> Result<Image, TextureError> {
    let bytes = embedded(name).ok_or_else(|| TextureError::Unknown(name.to_owned()))?;
    let mut image = Image::from_buffer(
        bytes,
        ImageType::Extension("jpg"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Descriptor(sampler()),
        // Rien à relire côté processeur une fois la texture envoyée au GPU.
        RenderAssetUsages::RENDER_WORLD,
    )
    .map_err(|source| TextureError::Decode {
        name: name.to_owned(),
        source,
    })?;
    let (width, height) = (image.width(), image.height());
    let Some(base) = image.data.take() else {
        return Err(TextureError::Format(name.to_owned()));
    };
    if base.len() != width as usize * height as usize * 4 {
        return Err(TextureError::Format(name.to_owned()));
    }
    let (data, levels) = with_mips(base, width, height);
    image.data = Some(data);
    image.texture_descriptor.mip_level_count = levels;
    Ok(image)
}

/// L'échantillonneur des astres : la longitude se répète (la couture de la sphère), les mips se mélangent.
fn sampler() -> ImageSamplerDescriptor {
    ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::ClampToEdge,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: ANISOTROPY,
        ..ImageSamplerDescriptor::default()
    }
}

/// Ajoute à la suite du niveau 0 tous les niveaux jusqu'à 1×1 (moyenne de 2×2). Renvoie les données et le nombre de niveaux.
///
/// La moyenne se fait sur les octets sRGB, sans repasser par la lumière linéaire : sur des textures de surface planétaire,
/// l'écart (un peu plus sombre) ne se voit pas à des tailles où le mip sert.
fn with_mips(base: Vec<u8>, width: u32, height: u32) -> (Vec<u8>, u32) {
    let mut data = base;
    let (mut w, mut h) = (width as usize, height as usize);
    let mut level_start = 0;
    let mut levels = 1;
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let level_end = data.len();
        data.reserve(nw * nh * 4);
        for y in 0..nh {
            for x in 0..nw {
                // Une dimension déjà à 1 se replie sur elle-même (x1 = x0).
                let (x0, x1) = ((x * 2).min(w - 1), (x * 2 + 1).min(w - 1));
                let (y0, y1) = ((y * 2).min(h - 1), (y * 2 + 1).min(h - 1));
                for c in 0..4 {
                    let at = |px: usize, py: usize| u32::from(data[level_start + (py * w + px) * 4 + c]);
                    let sum = at(x0, y0) + at(x1, y0) + at(x0, y1) + at(x1, y1);
                    data.push(((sum + 2) / 4) as u8);
                }
            }
        }
        level_start = level_end;
        (w, h) = (nw, nh);
        levels += 1;
    }
    (data, levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sky::spec::PLANETS;

    #[test]
    fn la_chaine_de_mips_va_jusqu_a_un_pixel_et_moyenne_les_blocs() {
        // 4×2 : un niveau de 2×1 puis un niveau de 1×1, soit 3 niveaux.
        let mut base = Vec::new();
        for _row in 0..2 {
            for x in 0..4u8 {
                base.extend([x * 40, 0, 255, 255]);
            }
        }
        let (data, levels) = with_mips(base, 4, 2);
        assert_eq!(levels, 3);
        assert_eq!(data.len(), (8 + 2 + 1) * 4);
        // Niveau 1 : (0 + 40) / 2 = 20 et (80 + 120) / 2 = 100 ; le canal bleu reste à 255.
        assert_eq!(&data[32..36], &[20, 0, 255, 255]);
        assert_eq!(&data[36..40], &[100, 0, 255, 255]);
        // Niveau 2 : (20 + 100) / 2 = 60.
        assert_eq!(&data[40..44], &[60, 0, 255, 255]);
    }

    #[test]
    fn toutes_les_textures_des_astres_sont_embarquees_et_se_decodent_avec_leurs_mips() {
        for spec in PLANETS {
            let image = load(spec.texture).map_err(|e| e.to_string()).expect("décodage");
            let (w, h) = (image.width(), image.height());
            assert!(w >= 512 && h >= 256, "{} : {w}×{h}", spec.texture);
            assert_eq!(image.texture_descriptor.mip_level_count, w.max(h).ilog2() + 1);
            let expected: usize = (0..image.texture_descriptor.mip_level_count)
                .map(|l| ((w >> l).max(1) * (h >> l).max(1) * 4) as usize)
                .sum();
            assert_eq!(image.data.as_ref().map(Vec::len), Some(expected), "{}", spec.texture);
        }
    }

    #[test]
    fn un_nom_inconnu_est_une_erreur_pas_une_panique() {
        assert!(matches!(load("pluton"), Err(TextureError::Unknown(_))));
    }
}
