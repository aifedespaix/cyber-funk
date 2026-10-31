//! Aperçu de la sortie projecteur dans la vue Live (J8-05) : une copie basse résolution, à cadence réduite.
//!
//! ```text
//! Update            request_capture   ≤ 10 i/s, une demande à la fois : Screenshot::primary_window()
//!   observateur       ScreenshotCaptured ─▶ réduction CPU (moyenne par blocs) ─▶ `Preview.frame`
//! EguiPrimaryContextPass
//!   upload_texture    `Preview.frame` ─▶ texture egui (rangée dans la mémoire du contexte)
//!   card              dessinée par la colonne de droite de la vue Live ; porte l'interrupteur
//! ```
//!
//! Le coût GPU est celui d'une copie de la cible de la fenêtre vers un tampon de lecture, 10 fois par seconde au
//! plus : aucune passe de rendu ni second rendu de la scène. La lecture est asynchrone côté Bevy ; la réduction
//! (CPU, hors fil de rendu GPU) est chronométrée et affichée. L'aperçu se désactive d'un clic et ne tourne qu'en Live.

use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};

use crate::live::OperatorMode;

/// Cadence maximale de l'aperçu, en images par seconde (la fiche impose 15 au plus).
pub(crate) const PREVIEW_FPS: f32 = 10.0;
/// Largeur de l'aperçu, en pixels : assez pour juger un plan, trop peu pour coûter.
pub(crate) const PREVIEW_WIDTH: usize = 320;
/// Au bout de ce délai sans réponse, une demande est considérée perdue et on en refait une.
const LOST_AFTER: Duration = Duration::from_secs(2);

/// Clé de l'interrupteur dans la mémoire d'egui : la carte (dessinée sans accès à Bevy) l'écrit, le système le lit.
fn enabled_key() -> egui::Id {
    egui::Id::new("cf_preview_enabled")
}

/// Clé de la texture dans la mémoire d'egui.
fn texture_key() -> egui::Id {
    egui::Id::new("cf_preview_texture")
}

/// Image réduite en RGBA 8 bits, octets sRGB.
#[derive(Debug, Default)]
pub(crate) struct PreviewFrame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

/// État de l'aperçu.
#[derive(Resource, Debug)]
pub(crate) struct Preview {
    /// L'aperçu est demandé (interrupteur de la carte).
    pub enabled: bool,
    timer: Timer,
    in_flight_since: Option<Instant>,
    frame: PreviewFrame,
    fresh: bool,
    /// Durée de la dernière réduction, en millisecondes.
    pub reduce_ms: f32,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            enabled: true,
            timer: Timer::from_seconds(1.0 / PREVIEW_FPS, TimerMode::Repeating),
            in_flight_since: None,
            frame: PreviewFrame::default(),
            fresh: false,
            reduce_ms: 0.0,
        }
    }
}

/// Branche l'aperçu. Ajouté par `OperatorPlugin`.
pub(crate) fn build(app: &mut App) {
    app.init_resource::<Preview>().add_systems(Update, request_capture);
}

/// Demande une capture de la fenêtre projecteur si c'est l'heure, qu'aucune n'est en cours et que l'aperçu est utile.
fn request_capture(
    time: Res<Time<Real>>,
    mode: Res<OperatorMode>,
    mut preview: ResMut<Preview>,
    mut commands: Commands,
) {
    preview.timer.tick(time.delta());
    if let Some(since) = preview.in_flight_since
        && since.elapsed() > LOST_AFTER
    {
        preview.in_flight_since = None;
    }
    let wanted = preview.enabled && *mode == OperatorMode::Live;
    if !wanted || preview.in_flight_since.is_some() || !preview.timer.just_finished() {
        return;
    }
    preview.in_flight_since = Some(Instant::now());
    commands.spawn(Screenshot::primary_window()).observe(on_captured);
}

fn on_captured(captured: On<ScreenshotCaptured>, mut preview: ResMut<Preview>) {
    let started = Instant::now();
    let image = &captured.image;
    let size = image.texture_descriptor.size;
    let bgra = match image.texture_descriptor.format {
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => Some(true),
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => Some(false),
        _ => None,
    };
    if let (Some(bgra), Some(data)) = (bgra, image.data.as_deref()) {
        let Preview { frame, .. } = &mut *preview;
        let (w, h) = reduce(
            data,
            size.width as usize,
            size.height as usize,
            bgra,
            PREVIEW_WIDTH,
            &mut frame.rgba,
        );
        frame.width = w;
        frame.height = h;
        preview.fresh = w > 0;
        preview.reduce_ms = started.elapsed().as_secs_f32() * 1000.0;
    }
    preview.in_flight_since = None;
}

/// Réduit `src` (largeur `sw`, hauteur `sh`, 4 octets par pixel) à `out_w` de large par moyenne de blocs, en RGBA.
/// Renvoie la taille obtenue ; `dst` est réutilisé. Un format ou des données incohérents donnent `(0, 0)`.
pub(crate) fn reduce(src: &[u8], sw: usize, sh: usize, bgra: bool, out_w: usize, dst: &mut Vec<u8>) -> (usize, usize) {
    if sw == 0 || sh == 0 || out_w == 0 || src.len() < sw * sh * 4 {
        return (0, 0);
    }
    let step = sw.div_ceil(out_w).max(1);
    let (ow, oh) = (sw / step, sh / step);
    if ow == 0 || oh == 0 {
        return (0, 0);
    }
    dst.clear();
    dst.reserve(ow * oh * 4);
    let (r, b) = if bgra { (2, 0) } else { (0, 2) };
    let count = (step * step) as u32;
    for oy in 0..oh {
        for ox in 0..ow {
            let mut sum = [0u32; 3];
            for y in 0..step {
                let row = ((oy * step + y) * sw + ox * step) * 4;
                for px in src[row..row + step * 4].as_chunks::<4>().0 {
                    sum[0] += u32::from(px[r]);
                    sum[1] += u32::from(px[1]);
                    sum[2] += u32::from(px[b]);
                }
            }
            for channel in sum {
                dst.push(u8::try_from(channel / count).unwrap_or(u8::MAX));
            }
            dst.push(255);
        }
    }
    (ow, oh)
}

/// Passe la dernière image réduite dans une texture egui et recopie l'interrupteur de la carte.
pub(crate) fn sync_egui(mut context: Single<&mut EguiContext, With<PrimaryEguiContext>>, mut preview: ResMut<Preview>) {
    let ctx = context.get_mut();
    let enabled: bool = ctx.data(|d| d.get_temp(enabled_key())).unwrap_or(true);
    if preview.enabled != enabled {
        preview.enabled = enabled;
    }
    if !preview.fresh {
        return;
    }
    preview.fresh = false;
    let image =
        egui::ColorImage::from_rgba_unmultiplied([preview.frame.width, preview.frame.height], &preview.frame.rgba);
    let existing: Option<egui::TextureHandle> = ctx.data(|d| d.get_temp(texture_key()));
    if let Some(mut handle) = existing {
        handle.set(image, egui::TextureOptions::LINEAR);
    } else {
        let handle = ctx.load_texture("cf_preview", image, egui::TextureOptions::LINEAR);
        ctx.data_mut(|d| d.insert_temp(texture_key(), handle));
    }
}

/// La carte « APERÇU » de la colonne de droite : l'image réduite et l'interrupteur.
pub(crate) fn card(ui: &mut egui::Ui) {
    let mut enabled: bool = ui.data(|d| d.get_temp(enabled_key())).unwrap_or(true);
    ui.horizontal(|ui| {
        ui.strong("APERÇU");
        if ui.checkbox(&mut enabled, "actif").changed() {
            ui.data_mut(|d| d.insert_temp(enabled_key(), enabled));
        }
    });
    if !enabled {
        ui.weak("Aperçu désactivé : aucun coût de capture.");
        return;
    }
    let texture: Option<egui::TextureHandle> = ui.data(|d| d.get_temp(texture_key()));
    match texture {
        Some(texture) => {
            let size = texture.size_vec2();
            let width = ui.available_width();
            ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(width, width * size.y / size.x.max(1.0))));
        }
        None => {
            ui.weak("En attente de la première image…");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, px: [u8; 4]) -> Vec<u8> {
        px.iter().copied().cycle().take(w * h * 4).collect()
    }

    #[test]
    fn une_image_unie_reste_unie_et_prend_la_taille_visee() {
        let src = solid(1920, 1080, [10, 20, 30, 255]);
        let mut dst = Vec::new();
        let (w, h) = reduce(&src, 1920, 1080, false, PREVIEW_WIDTH, &mut dst);
        assert_eq!((w, h), (320, 180));
        assert_eq!(dst.len(), 320 * 180 * 4);
        assert_eq!(&dst[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn bgra_est_remis_en_rgba() {
        let src = solid(4, 4, [30, 20, 10, 255]);
        let mut dst = Vec::new();
        let (w, _) = reduce(&src, 4, 4, true, 2, &mut dst);
        assert_eq!(w, 2);
        assert_eq!(&dst[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn la_moyenne_de_blocs_melange_les_pixels() {
        let mut src = solid(2, 2, [0, 0, 0, 255]);
        src[..4].copy_from_slice(&[200, 200, 200, 255]);
        let mut dst = Vec::new();
        assert_eq!(reduce(&src, 2, 2, false, 1, &mut dst), (1, 1));
        assert_eq!(&dst[..3], &[50, 50, 50]);
    }

    #[test]
    fn des_donnees_incoherentes_donnent_une_image_vide() {
        let mut dst = Vec::new();
        assert_eq!(reduce(&[0; 8], 100, 100, false, 10, &mut dst), (0, 0));
        assert_eq!(reduce(&[], 0, 0, false, 10, &mut dst), (0, 0));
    }

    #[test]
    fn la_cadence_respecte_la_limite_de_la_fiche() {
        const { assert!(PREVIEW_FPS <= 15.0) };
    }

    #[test]
    fn la_reduction_d_une_image_hd_est_peu_couteuse() {
        let src = solid(1920, 1080, [1, 2, 3, 255]);
        let mut dst = Vec::new();
        let start = Instant::now();
        reduce(&src, 1920, 1080, true, PREVIEW_WIDTH, &mut dst);
        // Large marge : le test garde l'ordre de grandeur (quelques ms en optimisé), pas un chiffre précis.
        assert!(start.elapsed() < Duration::from_millis(500));
    }
}
