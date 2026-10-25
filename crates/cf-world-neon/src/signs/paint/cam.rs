//! L'habillage des écrans-caméra (`art-cam.ts`, `art-live.ts`) : coins, étiquettes (REC, LIVE, SANS SIGNAL), identifiant
//! et vitesse, plaque du DJ sur l'écran géant.
//!
//! Le cadre opaque, le réticule, les bandes de cinéma, la neige et le flash du radar sont dessinés par le matériau (ce
//! sont des formes simples et c'est lui qui connaît l'état du flux). La cellule ne porte que ce qui est du texte :
//!
//! - les coins (accent) et les mots fixes (« CAM », « KM/H ») ;
//! - dans la bande **sous l'image** (y ≥ [`BAND_Y`], là où les autres cellules n'ont rien), ce que le matériau pioche :
//!   les trois étiquettes ([`LABELS`]) et les dix chiffres de la police à pas fixe ([`DIGITS_Y`]), pour écrire le numéro
//!   de caméra et la vitesse sans refaire de texture.

use super::canvas::{Canvas, G, Ink, K, R};
use super::icons::draw_run;
use super::text::{Face, Fonts};
use crate::signs::formats::{CANVAS_W, IMAGE_H};

const W: f32 = CANVAS_W as f32;

/// La fenêtre vidéo dans le canevas : (x, y, largeur, hauteur) en pixels (`FEED`).
pub const FEED: (f32, f32, f32, f32) = (10.0, 10.0, W - 20.0, (IMAGE_H - 14) as f32);
/// Début de la bande de stockage sous l'image, en pixels.
pub const BAND_Y: f32 = IMAGE_H as f32;
/// Ligne de base des étiquettes dans la bande.
const LABEL_BASELINE: f32 = BAND_Y + 15.0;
/// Corps des étiquettes, en pixels.
const LABEL_SIZE: f32 = 18.0;
/// Les étiquettes de la bande : (texte, x de départ, largeur réservée). Même ordre que dans `sign.wesl`.
pub const LABELS: [(&str, f32, f32); 3] = [("REC", 4.0, 56.0), ("LIVE", 64.0, 70.0), ("SANS SIGNAL", 140.0, 150.0)];
/// Haut de la rangée de chiffres, en pixels.
pub const DIGITS_Y: f32 = BAND_Y + 20.0;
/// Hauteur de la rangée de chiffres, en pixels.
pub const DIGITS_H: f32 = 20.0;
/// Largeur d'une case de chiffre, en pixels.
pub const DIGIT_SLOT: f32 = 13.0;
/// Abscisse de la première case de chiffre.
pub const DIGITS_X: f32 = 4.0;

/// Dessine l'habillage ; `live` : le nom et l'événement du DJ pour un écran géant du spectacle.
pub fn cam(canvas: &mut Canvas, fonts: &mut Fonts, live: Option<(&str, &str)>) {
    let (fx, fy, fw, fh) = FEED;
    // Coins du cadre : quatre équerres d'accent, avec halo.
    let l = 26.0;
    for (cx, cy, sx, sy) in [
        (fx, fy, 1.0, 1.0),
        (fx + fw, fy, -1.0, 1.0),
        (fx, fy + fh, 1.0, -1.0),
        (fx + fw, fy + fh, -1.0, -1.0),
    ] {
        canvas.polyline(
            Ink::new(R).glow(8.0),
            &[(cx, cy + sy * l), (cx, cy), (cx + sx * l, cy)],
            3.0,
            false,
        );
    }
    // Mots fixes : « CAM » devant le numéro (deux chiffres, écrits par le matériau), « KM/H » après la vitesse.
    text_right(
        canvas,
        fonts,
        Ink::new(R),
        "CAM",
        Face::Mono,
        18.0,
        fx + fw - 14.0 - 2.0 * 10.8 - 6.0,
        fy + 11.0 + 17.0,
    );
    text_right(
        canvas,
        fonts,
        Ink::new(G),
        "KM/H",
        Face::Mono,
        26.0,
        fx + fw - 14.0,
        fy + fh - 8.0 - 3.0,
    );
    // Étiquettes et chiffres : le stockage sous l'image.
    for (text, x, _) in LABELS {
        let run = fonts.layout(text, Face::Mono, LABEL_SIZE);
        draw_run(canvas, Ink::new(R), &run, x, LABEL_BASELINE, None);
    }
    for d in 0..10 {
        let text = char::from(b'0' + d).to_string();
        let run = fonts.layout(&text, Face::Mono, 18.0);
        let x = DIGITS_X + f32::from(d) * DIGIT_SLOT + (DIGIT_SLOT - run.width) / 2.0;
        draw_run(canvas, Ink::new(G), &run, x, DIGITS_Y + 16.0, None);
    }
    if let Some((name, event)) = live {
        plate(canvas, fonts, name, event);
    }
}

/// Texte dont le bord droit est en `right`, ligne de base en `baseline`.
fn text_right(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    ink: Ink,
    text: &str,
    face: Face,
    size: f32,
    right: f32,
    baseline: f32,
) {
    let run = fonts.layout(text, face, size);
    draw_run(canvas, ink, &run, right - run.width, baseline, None);
}

/// Surimpression de l'écran géant du DJ : une plaque en bas du flux avec le nom en grosses lettres et l'événement. Le
/// tempo (que Sillage ajoutait à droite) n'est pas écrit : il change à chaque image, on ne le cuit pas.
fn plate(canvas: &mut Canvas, fonts: &mut Fonts, name: &str, event: &str) {
    let (fx, fy, fw, fh) = FEED;
    let height = fh * 0.34;
    let top = fy + fh - height - 8.0;
    // Fond sombre sous la plaque (canal K, posé avant les couleurs), liseré d'accent à gauche.
    canvas.rect(Ink::new(K).alpha(0.82), fx + 10.0, top, fw - 20.0, height);
    canvas.rect(Ink::new(R), fx + 10.0, top, 5.0, height);
    let room = fw - 160.0;
    let mut run = fonts.layout(name, Face::Sans, 58.0);
    let mut size = 58.0;
    if run.width > room {
        size = (58.0 * room / run.width).floor();
        run = fonts.layout(name, Face::Sans, size);
    }
    draw_run(
        canvas,
        Ink::new(G).glow(10.0),
        &run,
        fx + 28.0,
        top + height * 0.42 + size * 0.34,
        None,
    );
    let line = if event.is_empty() { "EN DIRECT" } else { event };
    let small = fonts.layout(line, Face::Mono, 20.0);
    draw_run(
        canvas,
        Ink::new(R),
        &small,
        fx + 28.0,
        top + height * 0.82 + 20.0 * 0.34,
        None,
    );
}
