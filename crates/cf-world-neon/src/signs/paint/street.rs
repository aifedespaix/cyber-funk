//! Les visuels « rue » (`art-street.ts`) : propagande, immobilier et concert, matraquage. Même principe que `arts` : la
//! pose de repos est cuite, le stroboscope de « ACHÈTE MOI » passe par le limiteur de flashs (`FLAG_GATE_R`).
//!
//! Canaux : `R` = accent de la pub, `G` = glace, `B` = couleur propre à la cellule, `K` = encre.

use super::arts::{Draw, fit_text, lines, paragraph};
use super::canvas::{B, Canvas, G, Ink, R};
use super::icons::{draw_icon, draw_run};
use super::text::{Face, Fonts};
use crate::signs::formats::{CANVAS_W, IMAGE_H};

const W: f32 = CANVAS_W as f32;
const H: f32 = IMAGE_H as f32;

pub(super) fn politics(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    // Un texte qui fonce sur la caméra : au repos, sa taille de base.
    paragraph(canvas, fonts, d, 18.0, W * 0.5, Ink::new(G).glow(10.0));
    let (cx, cy) = (W * 0.79, H * 0.46);
    if let Some(icon) = d.icon {
        draw_icon(canvas, fonts, icon, cx, cy);
        return;
    }
    let grin = 0.4_f32;
    canvas.ellipse(Ink::new(R).glow(14.0), cx, cy, 52.0, 64.0 + grin * 6.0, None);
    // Yeux fixes et vides, sourire figé qui dépasse du visage.
    canvas.circle(Ink::cut(R), cx - 20.0, cy - 16.0, 7.0);
    canvas.circle(Ink::cut(R), cx + 20.0, cy - 16.0, 7.0);
    let reach = 26.0 + grin * 34.0;
    let mut smile = Vec::new();
    for i in 0..=24 {
        let t = i as f32 / 24.0;
        let (p0, p1, p2, p3) = (
            (cx - reach, cy + 18.0 - grin * 10.0),
            (cx - reach * 0.5, cy + 36.0 + grin * 26.0),
            (cx + reach * 0.5, cy + 36.0 + grin * 26.0),
            (cx + reach, cy + 18.0 - grin * 10.0),
        );
        let u = 1.0 - t;
        let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        smile.push((
            a * p0.0 + b * p1.0 + c * p2.0 + e * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + e * p3.1,
        ));
    }
    canvas.polyline(Ink::cut(R), &smile, 4.0, false);
    for i in -3..=3 {
        let x = cx + i as f32 * (reach / 3.4);
        let drop = (i as f32).abs() * 3.0;
        canvas.segment(
            Ink::cut(R),
            (x, cy + 27.0 + grin * 10.0 - drop),
            (x, cy + 34.0 + grin * 14.0 - drop),
            4.0,
        );
    }
}

pub(super) fn estate(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    if d.rank.is_multiple_of(2) {
        // Texte de l'annonce immobilière, en pas fixe, sur sa halo violet.
        let len = d.slogan.chars().count();
        let size = if len > 70 {
            18.0
        } else if len > 50 {
            21.0
        } else {
            25.0
        };
        lines(
            canvas,
            fonts,
            d.slogan,
            18.0,
            W * 0.5,
            size,
            size * 1.3,
            Ink::new(G).glow(8.0),
        );
    } else {
        concert(canvas, fonts, d);
    }
    let (cx, base) = (W * 0.78, H - 14.0);
    if let Some(icon) = d.icon {
        draw_icon(canvas, fonts, icon, cx, H * 0.5);
        return;
    }
    // Trois immeubles filaires dont les étages s'allument.
    for (i, dx) in [-26.0_f32, 0.0, 24.0].into_iter().enumerate() {
        let tall = 70.0 + ((i * 37) % 3) as f32 * 22.0;
        let half = (14.0 + 0.6 * 12.0) * if i == 1 { 1.2 } else { 1.0 };
        canvas.rect_outline(
            Ink::new(R).glow(8.0),
            cx + dx - half,
            base - tall,
            half * 2.0,
            tall,
            2.0,
        );
        for row in 0..(tall / 11.0) as usize {
            let alpha = 0.25 + 0.5 * ((row + i) % 3) as f32 / 2.0;
            let y = base - tall + row as f32 * 11.0;
            canvas.segment(Ink::new(R).alpha(alpha), (cx + dx - half, y), (cx + dx + half, y), 2.0);
        }
    }
}

/// Affiche de concert : un égaliseur à mi-hauteur, le nom du groupe (ou du DJ) et l'événement.
fn concert(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    let (name, event) = d.live.map_or(("DAFT PURGES", "STADE BARDELA"), |(n, e)| {
        (n, if e.is_empty() { "LIVE SET" } else { e })
    });
    fit_text(
        canvas,
        fonts,
        Ink::new(G).glow(8.0),
        name,
        Face::Sans,
        40.0,
        (W * 0.28, H * 0.2),
        W * 0.5,
    );
    fit_text(
        canvas,
        fonts,
        Ink::new(G),
        event,
        Face::Mono,
        22.0,
        (W * 0.28, H * 0.34),
        W * 0.5,
    );
    for i in 0..16 {
        let level = 0.45 + 0.35 * ((i as f32 * 1.3).sin()).abs();
        let height = 8.0 + level * (H * 0.45);
        let ink = if i % 2 == 0 { Ink::new(R) } else { Ink::new(B) };
        canvas.rect(ink, 18.0 + i as f32 * 15.5, H - 14.0 - height, 11.0, height);
    }
}

pub(super) fn brute(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    if d.rank.is_multiple_of(2) {
        // « ACHÈTE MOI » en stroboscope : tout le canal R est soumis au limiteur (`FLAG_GATE_R`).
        fit_text(
            canvas,
            fonts,
            Ink::new(R).glow(24.0),
            "ACHÈTE MOI",
            Face::Sans,
            118.0,
            (W / 2.0, H / 2.0),
            W * 0.94,
        );
        return;
    }
    // Silhouette cliché et barres de censure.
    let cx = W * 0.78;
    canvas.ellipse(Ink::new(B).alpha(0.7), cx, H * 0.28, 17.0, 20.0, None);
    canvas.ellipse(Ink::new(B).alpha(0.7), cx, H * 0.68, 30.0, 56.0, None);
    for i in 0..7 {
        let row = (i * 3) % 4;
        let ink = if i % 2 == 0 { Ink::cut(B) } else { Ink::new(G) };
        canvas.rect(
            ink,
            cx - 36.0 + ((i * 29) % 40) as f32,
            H * 0.4 + i as f32 * 10.0,
            28.0 + row as f32 * 12.0,
            8.0,
        );
    }
    // Néon de motel qui se déchire en aberration.
    let amount = 2.5;
    for (ink, dx) in [
        (Ink::new(B).alpha(0.55), -amount),
        (Ink::new(G).alpha(0.55), 0.0),
        (Ink::new(R).alpha(0.55), amount),
    ] {
        let run = fonts.layout("SeXxX", Face::Sans, 64.0);
        draw_run(
            canvas,
            ink.glow(10.0),
            &run,
            W * 0.3 - run.width / 2.0 + dx,
            H * 0.45 + 64.0 * 0.34,
            None,
        );
    }
    fit_text(
        canvas,
        fonts,
        Ink::new(G),
        "TU VEUX DU",
        Face::Mono,
        22.0,
        (W * 0.3, H * 0.15),
        W * 0.5,
    );
    fit_text(
        canvas,
        fonts,
        Ink::new(G),
        "ACHÈTE-MOI !",
        Face::Sans,
        34.0,
        (W * 0.3, H * 0.78),
        W * 0.5,
    );
}
