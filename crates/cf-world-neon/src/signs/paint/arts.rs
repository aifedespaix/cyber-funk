//! Les visuels « produit » (`art-product.ts`, `art-street.ts`) : viande, mode, esprit, évasion, propagande, immobilier,
//! matraquage. Sillage les animait avec la musique ; on cuit leur pose de repos, le matériau y ajoute la pulsation, et
//! les effets qui claquent (stroboscope de « ACHÈTE MOI », « GAME OVER ») passent par le limiteur de flashs.
//!
//! Canaux : `R` = accent de la pub, `G` = glace, `B` = couleur propre à la cellule, `K` = encre.

use super::canvas::{B, Canvas, G, Ink, K, R};
use super::icons::{draw_icon, draw_run, text_center};
use super::text::{Face, Fonts};
use crate::signs::ads::{Ad, AdKind};
use crate::signs::formats::{CANVAS_W, IMAGE_H};
use crate::signs::icons::IconId;

const W: f32 = CANVAS_W as f32;
const H: f32 = IMAGE_H as f32;

/// Ce qu'un visuel sait de son panneau.
#[derive(Debug, Clone, Copy)]
pub struct Draw<'a> {
    /// La publicité.
    pub ad: &'a Ad,
    /// Le slogan affiché.
    pub slogan: &'a str,
    /// Rang du slogan (la parité choisit le texte ou l'affiche pour l'immobilier et le matraquage).
    pub rank: usize,
    /// Icône de remplacement appelée par le slogan.
    pub icon: Option<IconId>,
    /// Pour un panneau « DJ » : (nom, événement).
    pub live: Option<(&'a str, &'a str)>,
}

/// Dessine le visuel du produit de `d.ad`.
pub fn draw(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    match d.ad.kind {
        AdKind::Food => food(canvas, fonts, d),
        AdKind::Fashion => fashion(canvas, fonts, d),
        AdKind::Mind => mind(canvas, fonts, d),
        AdKind::Escape => escape(canvas, fonts, d),
        AdKind::Politics => politics(canvas, fonts, d),
        AdKind::Estate => estate(canvas, fonts, d),
        AdKind::Brute => brute(canvas, fonts, d),
        AdKind::Watch | AdKind::Neon => {}
    }
}

/// Texte centré qui rentre dans `max_width` : la taille se réduit au besoin (`fitText`).
pub fn fit_text(
    canvas: &mut Canvas,
    fonts: &mut Fonts,
    ink: Ink,
    text: &str,
    face: Face,
    size: f32,
    (x, y): (f32, f32),
    max_width: f32,
) {
    let mut run = fonts.layout(text, face, size);
    let mut used = size;
    if run.width > max_width {
        used = (size * max_width / run.width).floor().max(6.0);
        run = fonts.layout(text, face, used);
    }
    draw_run(canvas, ink, &run, x - run.width / 2.0, y + used * 0.34, None);
}

/// Slogan en police à pas fixe, aligné à gauche : la taille baisse avec la longueur du texte (`paragraph`).
fn paragraph(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>, x: f32, width: f32, ink: Ink) {
    let len = d.slogan.chars().count();
    let size = if len > 70 {
        17.0
    } else if len > 45 {
        20.0
    } else {
        25.0
    };
    lines(canvas, fonts, d.slogan, x, width, size, size * 1.25, ink);
}

/// Pose `text` en lignes à pas fixe, centrées verticalement sur la zone d'image.
fn lines(canvas: &mut Canvas, fonts: &mut Fonts, text: &str, x: f32, width: f32, size: f32, lead: f32, ink: Ink) {
    let rows = fonts.wrap(text, Face::Mono, size, width);
    let top = H / 2.0 - (rows.len() as f32 - 1.0) * lead / 2.0;
    for (i, row) in rows.iter().enumerate() {
        let run = fonts.layout(row, Face::Mono, size);
        draw_run(canvas, ink, &run, x, top + i as f32 * lead + size * 0.34, None);
    }
}

fn food(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    let (cx, cy) = (W * 0.78, H * 0.5);
    paragraph(canvas, fonts, d, 18.0, W * 0.5, Ink::new(G).glow(6.0));
    if let Some(icon) = d.icon {
        draw_icon(canvas, fonts, icon, cx, cy);
        return;
    }
    // Un pavé de viande vu de face, étiqueté.
    let (width, height) = (90.0, 104.0);
    canvas.rect(
        Ink::new(R).glow(18.0),
        cx - width / 2.0,
        cy - height / 2.0,
        width,
        height,
    );
    canvas.rect(
        Ink::cut(R),
        cx - width / 2.0 + 5.0,
        cy - height / 2.0 + 14.0,
        width - 10.0,
        22.0,
    );
    let run = fonts.layout("VIANDE", Face::Mono, 13.0);
    draw_run(
        canvas,
        Ink::new(R),
        &run,
        cx - run.width / 2.0,
        cy - height / 2.0 + 25.0 + 4.4,
        None,
    );
}

const SHIRT: [(f32, f32); 13] = [
    (-48.0, -50.0),
    (-22.0, -56.0),
    (0.0, -42.0),
    (22.0, -56.0),
    (48.0, -50.0),
    (78.0, -18.0),
    (58.0, -2.0),
    (40.0, -14.0),
    (40.0, 58.0),
    (-40.0, 58.0),
    (-40.0, -14.0),
    (-58.0, -2.0),
    (-78.0, -18.0),
];

fn fashion(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    paragraph(canvas, fonts, d, 18.0, W * 0.5, Ink::new(G).glow(6.0));
    let (cx, cy) = (W * 0.78, H * 0.52);
    if let Some(icon) = d.icon {
        draw_icon(canvas, fonts, icon, cx, cy);
        return;
    }
    let pts = |dx: f32| SHIRT.iter().map(|(x, y)| (cx + x + dx, cy + y)).collect::<Vec<_>>();
    // Aberration chromatique : le même t-shirt décalé dans la couleur propre à la cellule.
    canvas.polygon(Ink::new(B).alpha(0.55), &pts(-3.0));
    canvas.polygon(Ink::new(R).alpha(0.85), &pts(0.0));
    // Logo thermocollé.
    text_center(canvas, fonts, Ink::cut(R), "◆ SECT.4", Face::Mono, 22.0, cx, cy + 10.0);
}

fn mind(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    paragraph(canvas, fonts, d, 18.0, W * 0.52, Ink::new(G).glow(6.0));
    let (cx, cy) = (W * 0.79, H * 0.4);
    if let Some(icon) = d.icon {
        draw_icon(canvas, fonts, icon, cx, cy);
    } else {
        // Cerveau scanné vu de trois quarts, ses circuits imprimés.
        let saved = canvas.xf;
        canvas.xf = saved.pre_translate(cx, cy).pre_scale(0.7, 1.0);
        let glow = Ink::new(R).glow(10.0);
        canvas.ellipse(glow.alpha(0.35), -20.0, 0.0, 34.0, 42.0, None);
        canvas.ellipse(Ink::new(R).alpha(0.35), 20.0, 0.0, 34.0, 42.0, None);
        canvas.ellipse(Ink::new(R), -20.0, 0.0, 34.0, 42.0, Some(2.0));
        canvas.ellipse(Ink::new(R), 20.0, 0.0, 34.0, 42.0, Some(2.0));
        for i in 0..9 {
            let y = -34.0 + i as f32 * 8.5;
            let alpha = if i % 2 == 0 { 0.9 } else { 0.25 };
            let (a, b) = ((i % 3) as f32 * 8.0, (i % 4) as f32 * 8.0);
            canvas.polyline(
                Ink::new(R).alpha(alpha),
                &[(-46.0 + a, y), (-6.0, y), (4.0, y + 5.0), (46.0 - b, y + 5.0)],
                2.0,
                false,
            );
        }
        canvas.xf = saved;
    }
    // Électrocardiogramme : un battement par temps.
    let base = H - 26.0;
    let beat = 0.6_f32;
    let mut points = Vec::new();
    let mut x = 0.0_f32;
    while x <= W {
        let t = x / 330.0;
        let f = t - t.floor();
        let spike = if f < 0.08 {
            -34.0 * (f / 0.08 * std::f32::consts::PI).sin()
        } else if f < 0.14 {
            12.0 * ((f - 0.08) / 0.06 * std::f32::consts::PI).sin()
        } else {
            0.0
        };
        points.push((x, base + spike * (beat + 0.4)));
        x += 3.0;
    }
    canvas.polyline(Ink::new(B).glow(6.0), &points, 2.0, false);
}

fn escape(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
    paragraph(canvas, fonts, d, 18.0, W * 0.5, Ink::new(G).glow(6.0));
    let (cx, cy) = (W * 0.78, H * 0.42);
    if let Some(icon) = d.icon {
        draw_icon(canvas, fonts, icon, cx, cy);
    } else {
        // Casque VR ensanglanté.
        canvas.rounded(Ink::new(G), cx - 62.0, cy - 28.0, 124.0, 56.0, 14.0, Some(3.0));
        canvas.rect(Ink::new(B).glow(8.0), cx - 46.0, cy - 12.0, 34.0, 24.0);
        canvas.rect(Ink::new(B), cx + 12.0, cy - 12.0, 34.0, 24.0);
        for i in 0..6 {
            let length = 8.0 + ((i * 11 + 7 * (i + 6)) % 38) as f32;
            canvas.rect(Ink::new(R), cx - 54.0 + i as f32 * 21.0, cy + 24.0, 6.0, length);
        }
    }
    // Le verdict, sur le canal d'encre : il n'apparaît que lorsque le limiteur de flashs l'accorde (`FLAG_VERDICT`).
    fit_text(
        canvas,
        fonts,
        Ink::new(K),
        "GAME OVER",
        Face::Sans,
        78.0,
        (W / 2.0, H / 2.0),
        W * 0.92,
    );
}

fn politics(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
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

fn estate(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
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

fn brute(canvas: &mut Canvas, fonts: &mut Fonts, d: &Draw<'_>) {
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
