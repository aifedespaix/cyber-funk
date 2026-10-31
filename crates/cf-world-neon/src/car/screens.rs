//! Les écrans de l'habitacle : le compteur (`screen_speed`) et l'autoradio (`screen_radio`).
//!
//! Sillage les dessinait dans un `CanvasTexture` (`CYBERCAR.md` §6). Ici chaque écran a une image RGBA8 propre, posée en
//! texture émissive du matériau de même nom. Le dessin est **logique pure** (rectangles dans un tampon d'octets) : pas
//! d'allocation, pas de GPU, testable seul. Les chiffres sont à sept segments, donc sans police à embarquer.
//!
//! # Sécurité photosensible (règle 7)
//!
//! Les barres de la radio ne suivent pas les kicks : elles lissent les trois bandes d'énergie (attaque et chute
//! bornées), l'image se redessine à 15 Hz au plus, et l'émission reste à 1 (jamais le bloom des néons). Quand le show
//! interdit les flashs, les barres restent figées à un niveau bas.

use bevy::asset::{Assets, Handle, RenderAssetUsages};
use bevy::color::LinearRgba;
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use cf_render::{AudioFeatures, CurrentShow, CurrentShowFx, ShowSheet};

use super::plate::{PLATE_H, PLATE_W, draw_plate, plate_text};
use super::rig::{CarRig, CarState};

/// Largeur du compteur, px.
pub const SPEED_W: i32 = 128;
/// Hauteur du compteur, px.
pub const SPEED_H: i32 = 128;
/// Largeur de l'autoradio, px.
pub const RADIO_W: i32 = 256;
/// Hauteur de l'autoradio, px.
pub const RADIO_H: i32 = 96;
/// Nombre de barres de l'autoradio.
pub const BARS: usize = 16;
/// Fréquence maximale de redessin, Hz (`CYBERCAR.md` §7 : « compteur ≤ 15 Hz »).
const REFRESH_HZ: f32 = 15.0;
/// Vitesse en bout de barre du compteur, km/h.
const SPEED_FULL_KMH: f32 = 240.0;

/// Quel écran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    /// Compteur de vitesse.
    Speed,
    /// Autoradio.
    Radio,
    /// Plaque d'immatriculation : le nom du DJ en cours, qui défile.
    Plate,
}

impl ScreenKind {
    /// Écran désigné par le nom du matériau.
    #[must_use]
    pub fn from_material(name: &str) -> Option<Self> {
        match name {
            "screen_speed" => Some(Self::Speed),
            "screen_radio" => Some(Self::Radio),
            "screen_plate" => Some(Self::Plate),
            _ => None,
        }
    }

    /// Dimensions de l'image, px.
    #[must_use]
    pub const fn size(self) -> (i32, i32) {
        match self {
            Self::Speed => (SPEED_W, SPEED_H),
            Self::Radio => (RADIO_W, RADIO_H),
            Self::Plate => (PLATE_W, PLATE_H),
        }
    }
}

/// Un écran posé : son image (modifiée sur place) et ce qu'il affiche.
#[derive(Debug)]
pub struct ScreenRig {
    /// Quel écran.
    pub kind: ScreenKind,
    /// Image émissive du matériau.
    pub image: Handle<Image>,
}

/// Image noire, prête à recevoir le dessin (l'émissif d'un écran éteint est noir).
#[must_use]
pub fn blank_image(kind: ScreenKind) -> Image {
    let (w, h) = kind.size();
    Image::new_fill(
        Extent3d {
            width: w.unsigned_abs(),
            height: h.unsigned_abs(),
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Un écran RGBA8 en cours de dessin.
pub(super) struct Pen<'a> {
    pub(super) data: &'a mut [u8],
    pub(super) width: i32,
    pub(super) height: i32,
}

impl Pen<'_> {
    pub(super) fn clear(&mut self, rgb: [u8; 3]) {
        for px in self.data.as_chunks_mut::<4>().0 {
            *px = [rgb[0], rgb[1], rgb[2], 255];
        }
    }

    /// Rectangle plein, rogné aux bords de l'image.
    pub(super) fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, rgb: [u8; 3]) {
        let x0 = x.clamp(0, self.width);
        let x1 = (x + w).clamp(0, self.width);
        let stride = usize::try_from(self.width).unwrap_or(0);
        for row in y.clamp(0, self.height)..(y + h).clamp(0, self.height) {
            let line = usize::try_from(row).unwrap_or(0) * stride;
            let start = (line + usize::try_from(x0).unwrap_or(0)) * 4;
            let end = (line + usize::try_from(x1).unwrap_or(0)) * 4;
            for px in self.data[start..end].as_chunks_mut::<4>().0 {
                *px = [rgb[0], rgb[1], rgb[2], 255];
            }
        }
    }
}

/// Couleurs d'un chiffre : (allumé, éteint).
type Colors = ([u8; 3], [u8; 3]);

/// Segments allumés par chiffre : bits a (haut), b (haut droite), c (bas droite), d (bas), e (bas gauche), f (haut
/// gauche), g (milieu).
const DIGITS: [u8; 10] = [
    0b011_1111, // 0
    0b000_0110, // 1
    0b101_1011, // 2
    0b100_1111, // 3
    0b110_0110, // 4
    0b110_1101, // 5
    0b111_1101, // 6
    0b000_0111, // 7
    0b111_1111, // 8
    0b110_1111, // 9
];

/// Chiffre à sept segments dans la boîte `(w, h)` ; sans valeur, tous les segments sont éteints (fond « 8 » estompé).
/// `colors` : (allumé, éteint).
#[allow(clippy::many_single_char_names)] // x, y, w, h, t : la géométrie d'un chiffre, lisible ainsi.
fn digit(pen: &mut Pen<'_>, x: i32, y: i32, (w, h): (i32, i32), value: Option<u8>, (on, off): Colors) {
    let t = (w / 5).max(2);
    let half = h / 2;
    let mask = value.map_or(0, |v| DIGITS[usize::from(v.min(9))]);
    let seg = |pen: &mut Pen<'_>, bit: u8, sx: i32, sy: i32, sw: i32, sh: i32| {
        let color = if mask & (1 << bit) != 0 { on } else { off };
        pen.rect(sx, sy, sw, sh, color);
    };
    seg(pen, 0, x + t, y, w - 2 * t, t);
    seg(pen, 1, x + w - t, y + t, t, half - t);
    seg(pen, 2, x + w - t, y + half, t, half - t);
    seg(pen, 3, x + t, y + h - t, w - 2 * t, t);
    seg(pen, 4, x, y + half, t, half - t);
    seg(pen, 5, x, y + t, t, half - t);
    seg(pen, 6, x + t, y + half - t / 2, w - 2 * t, t);
}

fn scale(rgb: [u8; 3], k: f32) -> [u8; 3] {
    [
        (f32::from(rgb[0]) * k) as u8,
        (f32::from(rgb[1]) * k) as u8,
        (f32::from(rgb[2]) * k) as u8,
    ]
}

/// Compteur : trois chiffres de km/h au centre et un arc de 270° autour (la vitesse sur [`SPEED_FULL_KMH`]), sa fin
/// en `warn` au-delà de 80 %.
pub fn draw_speed(data: &mut [u8], kmh: u32, accent: [u8; 3], warn: [u8; 3]) {
    let mut pen = Pen {
        data,
        width: SPEED_W,
        height: SPEED_H,
    };
    pen.clear([2, 5, 8]);
    let kmh = kmh.min(999);
    let digits = [kmh / 100, kmh / 10 % 10, kmh % 10].map(|d| u8::try_from(d).unwrap_or(0));
    for (i, d) in (0_i32..).zip(digits) {
        // Zéros de tête éteints : « 042 » s'affiche « 42 ».
        let shown = i == 2 || kmh >= if i == 0 { 100 } else { 10 };
        digit(
            &mut pen,
            24 + i * 27,
            42,
            (22, 42),
            shown.then_some(d),
            (accent, scale(accent, 0.08)),
        );
    }
    let fraction = (kmh as f32 / SPEED_FULL_KMH).clamp(0.0, 1.0);
    let stride = usize::try_from(SPEED_W).unwrap_or(0);
    for (row, y) in (0..SPEED_H).enumerate() {
        for (col, x) in (0..SPEED_W).enumerate() {
            let (dx, dy) = ((x - SPEED_W / 2) as f32 + 0.5, (y - SPEED_H / 2) as f32 + 0.5);
            let radius = dx.hypot(dy);
            if !(52.0..60.0).contains(&radius) {
                continue;
            }
            // Angle en degrés depuis le bas à gauche (135°), dans le sens des aiguilles d'une montre.
            let relative = (dy.atan2(dx).to_degrees() - 135.0).rem_euclid(360.0);
            if relative > 270.0 {
                continue;
            }
            let t = relative / 270.0;
            let color = match (t <= fraction, t > 0.8) {
                (true, true) => warn,
                (true, false) => accent,
                (false, _) => scale(accent, 0.1),
            };
            let at = (row * stride + col) * 4;
            pen.data[at..at + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
}

/// Autoradio : fréquence fixe en haut (`104.5`) et [`BARS`] barres d'égaliseur en bas.
pub fn draw_radio(data: &mut [u8], bars: &[f32; BARS], accent: [u8; 3], dim: [u8; 3]) {
    let mut pen = Pen {
        data,
        width: RADIO_W,
        height: RADIO_H,
    };
    pen.clear([3, 2, 6]);
    for (i, d) in (0_i32..).zip([1_u8, 0, 4]) {
        digit(
            &mut pen,
            12 + i * 20,
            8,
            (16, 28),
            Some(d),
            (accent, scale(accent, 0.08)),
        );
    }
    pen.rect(12 + 3 * 20 - 4, 33, 4, 4, accent);
    digit(
        &mut pen,
        12 + 3 * 20 + 6,
        8,
        (16, 28),
        Some(5),
        (accent, scale(accent, 0.08)),
    );
    let base = RADIO_H - 6;
    for (i, level) in (0_i32..).zip(bars) {
        let h = (level.clamp(0.0, 1.0) * 44.0).round() as i32 + 3;
        let x = 8 + i * 15;
        pen.rect(x, base - h, 11, h, if *level > 0.8 { accent } else { dim });
    }
}

/// Ce que l'autoradio montre : les barres lissées d'après les trois bandes d'énergie (graves à gauche).
#[derive(Debug, Default)]
pub struct ScreenClock {
    since_draw: f32,
    last_kmh: Option<u32>,
    bars: [f32; BARS],
}

/// Cible de chaque barre pour trois bandes : interpolation graves → médiums → aigus.
#[must_use]
pub fn bar_targets(bass: f32, mid: f32, high: f32) -> [f32; BARS] {
    let mut out = [0.0; BARS];
    for (i, bar) in out.iter_mut().enumerate() {
        let t = i as f32 / (BARS - 1) as f32;
        let level = if t < 0.5 {
            bass + (mid - bass) * (t * 2.0)
        } else {
            mid + (high - mid) * ((t - 0.5) * 2.0)
        };
        // Un léger relief propre à chaque barre, stable (pas de bruit : rien ne scintille).
        let relief = 0.85 + 0.15 * ((i as f32 * 1.7).sin() * 0.5 + 0.5);
        *bar = (level * relief).clamp(0.0, 1.0);
    }
    out
}

/// `CfSet::Present` : redessine le compteur quand le chiffre change, l'autoradio et la plaque à 15 Hz au plus.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn update_screens(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShowFx>>,
    current: Option<Res<CurrentShow>>,
    sheet: Option<Res<ShowSheet>>,
    cars: Query<(&CarRig, &CarState)>,
    mut images: ResMut<Assets<Image>>,
    mut clock: Local<ScreenClock>,
) {
    clock.since_draw += time.delta_secs();
    if clock.since_draw < 1.0 / REFRESH_HZ {
        return;
    }
    let dt = std::mem::take(&mut clock.since_draw);
    let flashes = show.is_none_or(|s| s.0.flashes);
    let (bass, mid, high) = features.map_or((0.0, 0.0, 0.0), |f| (f.0.bass, f.0.mid, f.0.high));
    let targets = if flashes {
        bar_targets(bass, mid, high)
    } else {
        [0.25; BARS]
    };
    // Attaque et chute bornées : une barre ne change pas de plus de 0,35 par image à 15 Hz.
    let step = (dt * 5.0).clamp(0.0, 0.35);
    for (bar, target) in clock.bars.iter_mut().zip(targets) {
        *bar += (target - *bar).clamp(-step, step);
    }
    let bars = clock.bars;
    // Le DJ du créneau en cours : le nom de la feuille de conduite, ou « DJ n ».
    let slot = current.as_deref().map_or(0, |c| c.0.slot);
    let name = sheet
        .as_deref()
        .and_then(|s| s.slot(slot))
        .map_or("", |s| s.dj.as_str());
    let plate = plate_text(name, slot);
    let seconds = time.elapsed_secs();
    for (rig, state) in &cars {
        // Couleurs du thème (la lueur du moment et son second néon), ramenées à pleine luminosité : l'écran garde la
        // teinte de la voiture quand le thème change, sans devenir plus clair que ses voisins.
        let (accent, hot) = state.look.map_or(([0, 230, 255], [255, 13, 115]), |look| {
            (screen_rgb(look.glow), screen_rgb(look.secondary))
        });
        let kmh = (state.speed_mps.max(0.0) * 3.6).round() as u32;
        for screen in &rig.screens {
            let Some(mut image) = images.get_mut(&screen.image) else {
                continue;
            };
            let Some(data) = image.data.as_mut() else { continue };
            match screen.kind {
                ScreenKind::Speed if clock.last_kmh != Some(kmh) => draw_speed(data, kmh, accent, hot),
                ScreenKind::Radio => draw_radio(data, &bars, hot, scale(hot, 0.55)),
                ScreenKind::Plate => draw_plate(data, &plate, seconds, accent, hot),
                ScreenKind::Speed => {}
            }
        }
        clock.last_kmh = Some(kmh);
    }
}

/// Couleur d'écran (sRGB 8 bits) d'une lueur linéaire, normalisée à son canal le plus fort.
fn screen_rgb(color: LinearRgba) -> [u8; 3] {
    let srgb = Srgba::from(color);
    let peak = srgb.red.max(srgb.green).max(srgb.blue).max(1e-3);
    [srgb.red, srgb.green, srgb.blue].map(|c| (c / peak * 255.0).clamp(0.0, 255.0) as u8)
}

/// Couleur linéaire de l'émission d'un écran allumé : blanc, la couleur vient de l'image.
pub const SCREEN_EMISSIVE: LinearRgba = LinearRgba::WHITE;

#[cfg(test)]
mod tests {
    use super::*;

    fn lit_pixels(data: &[u8]) -> usize {
        data.as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 100 || p[1] > 100 || p[2] > 100)
            .count()
    }

    fn buffer(kind: ScreenKind) -> Vec<u8> {
        let (w, h) = kind.size();
        vec![0; usize::try_from(w * h * 4).unwrap_or(0)]
    }

    #[test]
    fn le_compteur_allume_plus_de_segments_a_grande_vitesse() {
        let mut slow = buffer(ScreenKind::Speed);
        let mut fast = buffer(ScreenKind::Speed);
        draw_speed(&mut slow, 0, [0, 230, 255], [255, 13, 115]);
        draw_speed(&mut fast, 188, [0, 230, 255], [255, 13, 115]);
        assert!(
            lit_pixels(&fast) > lit_pixels(&slow) * 2,
            "{} / {}",
            lit_pixels(&fast),
            lit_pixels(&slow)
        );
    }

    #[test]
    fn le_compteur_ne_deborde_pas_et_reste_opaque() {
        let mut data = buffer(ScreenKind::Speed);
        draw_speed(&mut data, 5000, [0, 230, 255], [255, 13, 115]);
        assert!(data.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn la_radio_dessine_des_barres_proportionnelles() {
        let mut quiet = buffer(ScreenKind::Radio);
        let mut loud = buffer(ScreenKind::Radio);
        draw_radio(&mut quiet, &[0.0; BARS], [255, 13, 115], [140, 7, 63]);
        draw_radio(&mut loud, &[1.0; BARS], [255, 13, 115], [140, 7, 63]);
        assert!(lit_pixels(&loud) > lit_pixels(&quiet) + 1000);
    }

    #[test]
    fn les_barres_vont_des_graves_aux_aigus() {
        let bars = bar_targets(1.0, 0.5, 0.0);
        assert!(bars[0] > bars[BARS / 2] && bars[BARS / 2] > bars[BARS - 1], "{bars:?}");
        assert!(bars.iter().all(|b| (0.0..=1.0).contains(b)));
    }

    #[test]
    fn les_noms_de_materiaux_designent_les_ecrans() {
        assert_eq!(ScreenKind::from_material("screen_speed"), Some(ScreenKind::Speed));
        assert_eq!(ScreenKind::from_material("screen_radio"), Some(ScreenKind::Radio));
        assert_eq!(ScreenKind::from_material("screen_mirror"), None);
    }
}
