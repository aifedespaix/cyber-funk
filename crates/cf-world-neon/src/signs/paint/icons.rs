//! Les icônes de remplacement (`drawIcon` de `art-icons.ts`), dessinées dans une boîte d'environ 110 × 110 px centrée
//! sur (cx, cy). Sillage les animait à chaque image ; la cuisson en garde la pose de repos (valeurs de `pulse`, `bass`,
//! `mid` et `sin(t)` à 0 ou à leur moyenne) et le matériau ajoute la pulsation d'ensemble.
//!
//! Canaux : `R` = accent de la pub, `G` = glace, `B` = couleur propre à la cellule, `K` = encre.

use std::f32::consts::{PI, TAU};

use tiny_skia::Transform;

use super::canvas::{B, Canvas, G, Ink, R};
use super::text::{Face, Fonts};
use crate::signs::icons::IconId;

/// Points d'un arc de cercle (angles en radians, sens trigonométrique de Canvas 2D : y vers le bas).
pub fn arc_points(cx: f32, cy: f32, r: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    let steps = (((a1 - a0).abs() * r / 3.0).ceil() as usize).clamp(8, 96);
    (0..=steps)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / steps as f32;
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect()
}

/// Dessine `icon` centrée sur (cx, cy).
pub fn draw_icon(canvas: &mut Canvas, fonts: &mut Fonts, icon: IconId, cx: f32, cy: f32) {
    let saved = canvas.xf;
    canvas.xf = saved.pre_translate(cx, cy);
    let glow = 10.0;
    let accent = Ink::new(R).glow(glow);
    let line = |w: f32| w;
    match icon {
        IconId::Cage => {
            // Un œuf derrière des barreaux.
            canvas.ellipse(Ink::new(G), 0.0, 8.0, 17.0, 22.0, None);
            canvas.rect_outline(accent, -46.0, -46.0, 92.0, 92.0, line(3.0));
            for i in 0..5 {
                let x = -32.0 + 16.0 * i as f32;
                canvas.segment(accent, (x, -46.0), (x, 46.0), 3.0);
            }
        }
        IconId::Tag => {
            // Étiquette de prix, avec son trou de fil et son code-barres.
            canvas.polygon(
                accent,
                &[(-32.0, -42.0), (32.0, -42.0), (32.0, 28.0), (0.0, 50.0), (-32.0, 28.0)],
            );
            canvas.circle(Ink::cut(R), 0.0, -30.0, 5.0);
            text_center(canvas, fonts, Ink::cut(R), "%", Face::Sans, 34.0, 0.0, 4.0);
            for i in 0..7 {
                let w = if i % 3 == 0 { 4.0 } else { 2.0 };
                canvas.rect(Ink::cut(R), -20.0 + i as f32 * 6.0, 20.0, w, 12.0);
            }
        }
        IconId::Needle => {
            // Bobine de fil et aiguille.
            canvas.rect(accent, -24.0, -42.0, 48.0, 7.0);
            canvas.rect(accent, -24.0, 35.0, 48.0, 7.0);
            canvas.rect(accent.alpha(0.65), -18.0, -35.0, 36.0, 70.0);
            let mut y = -30.0;
            while y < 34.0 {
                canvas.segment(Ink::cut(R), (-18.0, y), (18.0, y + 3.0), 3.0);
                y += 7.0;
            }
            canvas.segment(Ink::new(G), (40.0, -46.0), (54.0, 40.0), 3.0);
            let thread: Vec<(f32, f32)> = (0..=8)
                .map(|i| (18.0 + i as f32 * 4.5, -20.0 + i as f32 * 7.0 + (i as f32).sin() * 4.0))
                .collect();
            canvas.polyline(accent, &thread, 3.0, false);
        }
        IconId::Lock => {
            // Cadenas : l'anse, le corps, le trou de serrure.
            canvas.polyline(accent, &arc_points(0.0, -10.0, 22.0, PI, TAU), 3.0, false);
            canvas.rounded(accent, -34.0, -8.0, 68.0, 52.0, 8.0, None);
            canvas.circle(Ink::cut(R), 0.0, 14.0, 7.0);
            canvas.rect(Ink::cut(R), -3.0, 16.0, 6.0, 16.0);
        }
        IconId::Chip => {
            // Puce dont les broches s'allument l'une après l'autre : ici toutes à demi.
            canvas.rect_outline(accent, -28.0, -28.0, 56.0, 56.0, 3.0);
            canvas.rect_outline(accent, -14.0, -14.0, 28.0, 28.0, 3.0);
            for i in 0..5 {
                let p = -20.0 + i as f32 * 10.0;
                let alpha = if i % 2 == 0 { 1.0 } else { 0.5 };
                for (a, b) in [
                    ((p, -28.0), (p, -42.0)),
                    ((p, 28.0), (p, 42.0)),
                    ((-28.0, p), (-42.0, p)),
                    ((28.0, p), (42.0, p)),
                ] {
                    canvas.segment(accent.alpha(alpha), a, b, 3.0);
                }
            }
            canvas.rect(accent.alpha(0.75), -8.0, -8.0, 16.0, 16.0);
        }
        IconId::Hourglass => {
            // Sablier à moitié écoulé.
            let f = 0.5_f32;
            canvas.polyline(
                accent,
                &[
                    (-26.0, -46.0),
                    (26.0, -46.0),
                    (3.0, 0.0),
                    (26.0, 46.0),
                    (-26.0, 46.0),
                    (-3.0, 0.0),
                ],
                3.0,
                true,
            );
            canvas.rect(
                accent.alpha(0.75),
                -20.0 * (1.0 - f),
                -40.0 + f * 30.0,
                40.0 * (1.0 - f),
                30.0 * (1.0 - f) + 1.0,
            );
            canvas.rect(
                accent.alpha(0.75),
                -22.0 * f - 2.0,
                44.0 - 34.0 * f,
                44.0 * f + 4.0,
                34.0 * f,
            );
            canvas.rect(accent, -1.0, -2.0, 2.0, 44.0 - 34.0 * f);
        }
        _ => draw_play_icon(canvas, fonts, icon, accent),
    }
    canvas.xf = saved;
}

/// Les icônes de jeu, de politique et d'immobilier (la suite de `draw_icon`, même repère).
fn draw_play_icon(canvas: &mut Canvas, fonts: &mut Fonts, icon: IconId, accent: Ink) {
    match icon {
        IconId::Pad => {
            // Manette : croix directionnelle, deux boutons, barre de vie.
            canvas.rounded(accent, -50.0, -22.0, 100.0, 50.0, 18.0, Some(3.0));
            canvas.rect(accent, -36.0, -4.0, 22.0, 6.0);
            canvas.rect(accent, -28.0, -12.0, 6.0, 22.0);
            canvas.circle(Ink::new(G), 20.0, 4.0, 7.5);
            canvas.circle(Ink::new(B), 36.0, -6.0, 7.5);
            canvas.rect(accent, -46.0, -44.0, 92.0 * 0.6, 8.0);
            canvas.rect_outline(accent, -46.0, -44.0, 92.0, 8.0, 3.0);
        }
        IconId::Pause => {
            canvas.ellipse(accent, 0.0, 0.0, 45.0, 45.0, Some(3.0));
            canvas.rect(accent, -16.0, -22.0, 10.0, 44.0);
            canvas.rect(accent, 6.0, -22.0, 10.0, 44.0);
        }
        IconId::Power => {
            let ring = arc_points(0.0, 4.0, 37.0, -PI / 2.0 + 0.6, -PI / 2.0 - 0.6 + TAU);
            canvas.polyline(accent, &ring, 6.0, false);
            canvas.segment(accent, (0.0, -44.0), (0.0, -6.0), 6.0);
        }
        IconId::Ballot => {
            // Urne et bulletin à moitié glissé dans la fente.
            canvas.rect_outline(accent, -40.0, -6.0, 80.0, 52.0, 3.0);
            canvas.rect(accent, -22.0, -8.0, 44.0, 5.0);
            canvas.rect(Ink::new(G), -14.0, -40.0, 28.0, 32.0);
            canvas.polyline(Ink::cut(G), &[(-7.0, -22.0), (-2.0, -16.0), (8.0, -30.0)], 3.0, false);
        }
        IconId::Chart => {
            // Histogramme qui grimpe, flèche qui dépasse du cadre.
            for (i, level) in [0.4_f32, 0.6, 0.7, 0.9].into_iter().enumerate() {
                canvas.rect(accent, -42.0 + i as f32 * 22.0, 44.0 - level * 80.0, 16.0, level * 80.0);
            }
            canvas.polyline(
                Ink::new(G),
                &[(-46.0, 20.0), (-10.0, -4.0), (10.0, 6.0), (46.0, -40.0)],
                3.0,
                false,
            );
            canvas.polyline(Ink::new(G), &[(32.0, -38.0), (46.0, -40.0), (44.0, -26.0)], 3.0, false);
        }
        IconId::Liar => {
            // Le sourire du candidat, et un nez qui s'allonge.
            let nose = 8.0 + 0.4 * 46.0;
            canvas.circle(accent, -8.0, 0.0, 46.0);
            canvas.segment(accent, (-8.0, 4.0), (-8.0 + 40.0 + nose * 0.6, 8.0 + nose * 0.15), 7.0);
            canvas.circle(Ink::cut(R), -26.0, -14.0, 5.0);
            canvas.circle(Ink::cut(R), 8.0, -14.0, 5.0);
            canvas.polyline(Ink::cut(R), &arc_points(-8.0, 14.0, 20.0, 0.2, PI - 0.2), 4.0, false);
        }
        IconId::House => {
            // Maison, fenêtres allumées, bande « VENDU ».
            canvas.polyline(accent, &[(-42.0, 6.0), (0.0, -40.0), (42.0, 6.0)], 3.0, false);
            canvas.polyline(
                accent,
                &[(-32.0, 0.0), (-32.0, 44.0), (32.0, 44.0), (32.0, 0.0)],
                3.0,
                false,
            );
            canvas.rect(accent, -8.0, 20.0, 16.0, 24.0);
            canvas.rect(accent, 14.0, 8.0, 12.0, 10.0);
            canvas.rect(accent.alpha(0.3), -26.0, 8.0, 12.0, 10.0);
            let inner = canvas.xf;
            canvas.xf = inner.pre_rotate(-0.28 * 180.0 / PI);
            canvas.rect(Ink::new(B), -44.0, -6.0, 88.0, 20.0);
            text_center(canvas, fonts, Ink::cut(B), "VENDU", Face::Mono, 15.0, 0.0, 4.0);
            canvas.xf = inner;
        }
        IconId::Spot => {
            // Scène, deux projecteurs et une foule.
            for side in [-1.0_f32, 1.0] {
                let sweep = (1.4 * 0.0 + side).sin() * 22.0;
                canvas.polygon(
                    accent.alpha(0.28),
                    &[(side * 40.0, -46.0), (sweep - 14.0, 22.0), (sweep + 14.0, 22.0)],
                );
            }
            canvas.rect(accent, -46.0, 22.0, 92.0, 5.0);
            for i in 0..9 {
                let hop = ((i as f32 * 1.3).sin()).abs() * 8.0;
                canvas.circle(Ink::new(B), -40.0 + i as f32 * 10.0, 42.0 - hop, 4.0);
            }
        }
        // Les autres sont dans `draw_icon`.
        _ => {}
    }
}

/// Texte centré en (x, y) dans le repère courant, `y` étant le milieu de la ligne (`textBaseline = "middle"`).
pub fn text_center(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    ink: Ink,
    text: &str,
    face: Face,
    size: f32,
    x: f32,
    y: f32,
) {
    let run = fonts.layout(text, face, size);
    draw_run(canvas, ink, &run, x - run.width / 2.0, y + size * 0.34, None);
}

/// Pose une ligne de texte dont la ligne de base est en (x, y), remplie ou tracée (`outline` : largeur du trait).
pub fn draw_run(canvas: &mut Canvas, ink: Ink, run: &super::text::Run, x: f32, y: f32, outline: Option<f32>) {
    let mut pb = tiny_skia::PathBuilder::new();
    for glyph in &run.glyphs {
        pb.push_path(glyph);
    }
    let Some(path) = pb.finish() else { return };
    let saved = canvas.xf;
    canvas.xf = saved.pre_concat(Transform::from_translate(x, y));
    match outline {
        Some(width) => canvas.stroke(ink, &path, &Canvas::line_stroke(width)),
        None => canvas.fill(ink, &path),
    }
    canvas.xf = saved;
}
