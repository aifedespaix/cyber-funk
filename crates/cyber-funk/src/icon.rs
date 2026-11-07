//! Icône des fenêtres (projecteur et opérateur) : le PNG est embarqué, décodé au premier affichage de chaque fenêtre.

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::NonSendMarker;
use bevy::image::{CompressedImageFormats, ImageSampler, ImageType};
use bevy::prelude::*;
use bevy::window::WindowCreated;
use bevy::winit::WINIT_WINDOWS;
use winit::window::Icon;

/// Pose l'icône sur chaque fenêtre à sa création (projecteur et opérateur).
#[derive(Debug, Default, Clone, Copy)]
pub struct WindowIconPlugin;

impl Plugin for WindowIconPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, set_window_icons);
    }
}

/// Icône embarquée : 256 px, la plus grande que les barres de titre et la barre des tâches utilisent réellement.
const ICON_PNG: &[u8] = include_bytes!("../../../assets/images/icons/icon-256.png");

/// Pixels RGBA 8 bits d'une icône décodée.
struct DecodedIcon {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

/// Décode le PNG embarqué via le décodeur de Bevy (pas de dépendance directe à `image`).
fn decode_icon() -> Option<DecodedIcon> {
    let image = Image::from_buffer(
        ICON_PNG,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::default(),
        RenderAssetUsages::default(),
    )
    .inspect_err(|error| warn!("icône de fenêtre : décodage impossible : {error}"))
    .ok()?;
    // Un PNG RGBA 8 bits ressort en `Rgba8UnormSrgb` : un autre format ne serait pas lisible comme RGBA brut.
    if image.texture_descriptor.format != bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb {
        warn!("icône de fenêtre : format de pixels inattendu");
        return None;
    }
    let (width, height) = (image.width(), image.height());
    Some(DecodedIcon {
        rgba: image.data?,
        width,
        height,
    })
}

/// Applique l'icône aux fenêtres tout juste créées. Sans icône (décodage raté), les fenêtres gardent celle du système.
fn set_window_icons(
    mut created: MessageReader<WindowCreated>,
    mut icon: Local<Option<Icon>>,
    mut decoded_once: Local<bool>,
    _main_thread: NonSendMarker,
) {
    for event in created.read() {
        // Décodé une seule fois, même en cas d'échec (pas de journal répété à chaque fenêtre).
        if !*decoded_once {
            *decoded_once = true;
            *icon = decode_icon().and_then(|decoded| {
                Icon::from_rgba(decoded.rgba, decoded.width, decoded.height)
                    .inspect_err(|error| warn!("icône de fenêtre : pixels refusés : {error}"))
                    .ok()
            });
        }
        let Some(icon) = icon.clone() else { continue };
        WINIT_WINDOWS.with_borrow(|windows| {
            if let Some(window) = windows.get_window(event.window) {
                window.set_window_icon(Some(icon));
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_icon_decodes_and_is_square_256() {
        let icon = decode_icon().expect("le PNG embarqué se décode");
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert!(
            icon.rgba.as_chunks::<4>().0.iter().any(|px| px[3] > 0),
            "icône entièrement transparente"
        );
    }
}
