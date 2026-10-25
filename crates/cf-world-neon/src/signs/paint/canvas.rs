//! Le canevas de cuisson : quatre couches de couverture (R, G, B, K) dessinées avec `tiny-skia`.
//!
//! Sillage dessinait chaque enseigne en couleur dans un `CanvasTexture`, repeint à chaque image. Ici une cellule est
//! dessinée **une fois**, à l'ouverture, en quatre canaux de *couverture* ; la couleur de chaque canal est donnée à
//! l'affichage par le matériau (palette vivante, fondus de thème compris) :
//!
//! - `R` : la couleur d'accent de la publicité ;
//! - `G` : la glace (texte clair, éclats) ;
//! - `B` : une couleur propre à la cellule (`Cell::secondary`) ;
//! - `K` : l'encre, un voile sombre posé par-dessus (barres de censure, découpes).
//!
//! Le halo (`shadowBlur` de Canvas 2D) est un flou de boîte en trois passes de la forme, posé sous la forme nette.

use tiny_skia::{
    BlendMode, Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke,
    StrokeDash, Transform,
};

/// Canal d'accent.
pub const R: usize = 0;
/// Canal de glace.
pub const G: usize = 1;
/// Canal propre à la cellule.
pub const B: usize = 2;
/// Canal d'encre (voile sombre).
pub const K: usize = 3;

/// Comment poser une forme : sur quel canal, avec quelle opacité, et avec quel halo.
#[derive(Debug, Clone, Copy)]
pub struct Ink {
    /// Canal visé ([`R`], [`G`], [`B`], [`K`]).
    pub ch: usize,
    /// Opacité de la forme, 0 à 1.
    pub alpha: f32,
    /// Rayon du halo en pixels (`shadowBlur` / 2 de Canvas 2D) ; 0 : aucun.
    pub glow: f32,
    /// Découpe : la forme *efface* le canal au lieu d'y poser de la couverture (texte sombre sur une forme colorée).
    pub erase: bool,
}

impl Ink {
    /// Une encre sans halo, opaque.
    #[must_use]
    pub const fn new(ch: usize) -> Self {
        Self {
            ch,
            alpha: 1.0,
            glow: 0.0,
            erase: false,
        }
    }

    /// Une découpe du canal `ch` : ce qui était dessous disparaît (le fond de l'enseigne réapparaît).
    #[must_use]
    pub const fn cut(ch: usize) -> Self {
        Self {
            ch,
            alpha: 1.0,
            glow: 0.0,
            erase: true,
        }
    }

    /// Avec une opacité.
    #[must_use]
    pub const fn alpha(mut self, alpha: f32) -> Self {
        self.alpha = alpha;
        self
    }

    /// Avec un halo de `shadowBlur` pixels (le rayon de flou est la moitié, comme le sigma de Canvas 2D).
    #[must_use]
    pub const fn glow(mut self, shadow_blur: f32) -> Self {
        self.glow = shadow_blur / 2.0;
        self
    }
}

/// Le canevas : quatre couches de même taille et la transformation courante.
pub struct Canvas {
    w: u32,
    h: u32,
    layers: [Pixmap; 4],
    /// Transformation appliquée aux formes (translation, rotation de certaines icônes).
    pub xf: Transform,
}

impl std::fmt::Debug for Canvas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Canvas")
            .field("w", &self.w)
            .field("h", &self.h)
            .finish_non_exhaustive()
    }
}

fn white(ink: Ink) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba(1.0, 1.0, 1.0, ink.alpha.clamp(0.0, 1.0)).unwrap_or(Color::WHITE));
    paint.anti_alias = true;
    if ink.erase {
        paint.blend_mode = BlendMode::DestinationOut;
    }
    paint
}

fn solid(alpha: f32) -> Paint<'static> {
    white(Ink::new(0).alpha(alpha))
}

impl Canvas {
    /// Un canevas vide, `None` si la taille est nulle.
    #[must_use]
    pub fn new(w: u32, h: u32) -> Option<Self> {
        let layer = || Pixmap::new(w, h);
        Some(Self {
            w,
            h,
            layers: [layer()?, layer()?, layer()?, layer()?],
            xf: Transform::identity(),
        })
    }

    /// Largeur en pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.w
    }

    /// Hauteur en pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.h
    }

    /// Efface les quatre couches et remet la transformation à l'identité.
    pub fn clear(&mut self) {
        for layer in &mut self.layers {
            layer.fill(Color::TRANSPARENT);
        }
        self.xf = Transform::identity();
    }

    /// Pose une forme pleine.
    pub fn fill(&mut self, ink: Ink, path: &Path) {
        if ink.glow > 0.0 && !ink.erase {
            self.halo(ink, path, None);
        }
        let xf = self.xf;
        self.layers[ink.ch].fill_path(path, &white(ink), FillRule::Winding, xf, None);
    }

    /// Pose le halo d'une forme pleine **sans la forme** : l'ombre seule de Canvas 2D (`shadowOffset` hors cadre), comme
    /// l'enseigne du DJ la dessine pour ses canaux de halo.
    pub fn glow_only(&mut self, ink: Ink, path: &Path) {
        if ink.glow > 0.0 && !ink.erase {
            self.halo(ink, path, None);
        }
    }

    /// Pose un trait.
    pub fn stroke(&mut self, ink: Ink, path: &Path, stroke: &Stroke) {
        if ink.glow > 0.0 && !ink.erase {
            self.halo(ink, path, Some(stroke));
        }
        let xf = self.xf;
        self.layers[ink.ch].stroke_path(path, &white(ink), stroke, xf, None);
    }

    /// Un trait rond de `width` pixels.
    #[must_use]
    pub fn line_stroke(width: f32) -> Stroke {
        Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        }
    }

    /// Un trait pointillé de `width` pixels (`setLineDash`).
    #[must_use]
    pub fn dashed_stroke(width: f32, on: f32, off: f32, phase: f32) -> Stroke {
        Stroke {
            width,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            dash: StrokeDash::new(vec![on.max(0.1), off.max(0.1)], phase),
            ..Stroke::default()
        }
    }

    /// Rectangle plein.
    pub fn rect(&mut self, ink: Ink, x: f32, y: f32, w: f32, h: f32) {
        if let Some(rect) = Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)) {
            self.fill(ink, &PathBuilder::from_rect(rect));
        }
    }

    /// Contour de rectangle.
    pub fn rect_outline(&mut self, ink: Ink, x: f32, y: f32, w: f32, h: f32, width: f32) {
        if let Some(rect) = Rect::from_xywh(x, y, w.max(0.01), h.max(0.01)) {
            self.stroke(ink, &PathBuilder::from_rect(rect), &Self::line_stroke(width));
        }
    }

    /// Contour d'un rectangle à coins arrondis (le coin est une quadratique).
    #[must_use]
    pub fn rounded_path(x: f32, y: f32, w: f32, h: f32, radius: f32) -> Option<Path> {
        let r = radius.min(w / 2.0).min(h / 2.0);
        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        pb.finish()
    }

    /// Rectangle à coins arrondis, plein ou en contour.
    pub fn rounded(&mut self, ink: Ink, x: f32, y: f32, w: f32, h: f32, radius: f32, outline: Option<f32>) {
        if let Some(path) = Self::rounded_path(x, y, w, h, radius) {
            match outline {
                Some(width) => self.stroke(ink, &path, &Self::line_stroke(width)),
                None => self.fill(ink, &path),
            }
        }
    }

    /// Ellipse pleine ou en contour.
    pub fn ellipse(&mut self, ink: Ink, cx: f32, cy: f32, rx: f32, ry: f32, outline: Option<f32>) {
        if let Some(rect) = Rect::from_xywh(cx - rx, cy - ry, (rx * 2.0).max(0.01), (ry * 2.0).max(0.01))
            && let Some(path) = PathBuilder::from_oval(rect)
        {
            match outline {
                Some(width) => self.stroke(ink, &path, &Self::line_stroke(width)),
                None => self.fill(ink, &path),
            }
        }
    }

    /// Cercle plein.
    pub fn circle(&mut self, ink: Ink, cx: f32, cy: f32, r: f32) {
        self.ellipse(ink, cx, cy, r, r, None);
    }

    /// Ligne brisée (trait).
    pub fn polyline(&mut self, ink: Ink, points: &[(f32, f32)], width: f32, closed: bool) {
        let mut pb = PathBuilder::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 {
                pb.move_to(*x, *y);
            } else {
                pb.line_to(*x, *y);
            }
        }
        if closed {
            pb.close();
        }
        if let Some(path) = pb.finish() {
            self.stroke(ink, &path, &Self::line_stroke(width));
        }
    }

    /// Polygone plein.
    pub fn polygon(&mut self, ink: Ink, points: &[(f32, f32)]) {
        let mut pb = PathBuilder::new();
        for (i, (x, y)) in points.iter().enumerate() {
            if i == 0 {
                pb.move_to(*x, *y);
            } else {
                pb.line_to(*x, *y);
            }
        }
        pb.close();
        if let Some(path) = pb.finish() {
            self.fill(ink, &path);
        }
    }

    /// Un segment.
    pub fn segment(&mut self, ink: Ink, a: (f32, f32), b: (f32, f32), width: f32) {
        self.polyline(ink, &[a, b], width, false);
    }

    /// Efface un rectangle de toutes les couches (`clearRect`).
    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let Some(rect) = Rect::from_xywh(x, y, w, h) else {
            return;
        };
        let path = PathBuilder::from_rect(rect);
        let mut paint = solid(1.0);
        paint.blend_mode = BlendMode::Clear;
        let xf = self.xf;
        for layer in &mut self.layers {
            layer.fill_path(&path, &paint, FillRule::Winding, xf, None);
        }
    }

    /// Halo : la forme (pleine ou tracée) floutée, posée dessous. Le flou travaille sur la boîte englobante élargie.
    fn halo(&mut self, ink: Ink, path: &Path, stroke: Option<&Stroke>) {
        let pad = (ink.glow * 3.0).ceil() + stroke.map_or(0.0, |s| s.width);
        let bounds = path.bounds();
        let Some(device) = bounds.transform(self.xf) else {
            return;
        };
        let x0 = (device.left() - pad).floor().max(0.0) as i32;
        let y0 = (device.top() - pad).floor().max(0.0) as i32;
        let x1 = ((device.right() + pad).ceil() as i32).min(self.w as i32);
        let y1 = ((device.bottom() + pad).ceil() as i32).min(self.h as i32);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let Some(mut tmp) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else {
            return;
        };
        let xf = self.xf.post_translate(-(x0 as f32), -(y0 as f32));
        match stroke {
            Some(s) => tmp.stroke_path(path, &solid(1.0), s, xf, None),
            None => tmp.fill_path(path, &solid(1.0), FillRule::Winding, xf, None),
        }
        blur_alpha(&mut tmp, ink.glow);
        let paint = PixmapPaint {
            opacity: ink.alpha.clamp(0.0, 1.0),
            ..PixmapPaint::default()
        };
        self.layers[ink.ch].draw_pixmap(x0, y0, tmp.as_ref(), &paint, Transform::identity(), None);
    }

    /// Les quatre canaux de couverture, entrelacés RGBA (le canal `K` est l'alpha).
    #[must_use]
    pub fn pack_rgba(&self) -> Vec<u8> {
        let n = (self.w * self.h) as usize;
        let mut out = vec![0_u8; n * 4];
        for (c, layer) in self.layers.iter().enumerate() {
            for (i, alpha) in alpha_of(layer.data()).enumerate() {
                out[i * 4 + c] = alpha;
            }
        }
        out
    }

    /// Le canal `R` seul (cellules à une couleur : projections, bandeaux).
    #[must_use]
    pub fn pack_r8(&self) -> Vec<u8> {
        alpha_of(self.layers[R].data()).collect()
    }
}

/// L'alpha de chaque pixel d'un tampon RGBA : la couverture, puisque les formes sont posées en blanc.
fn alpha_of(data: &[u8]) -> impl Iterator<Item = u8> + '_ {
    data.as_chunks::<4>().0.iter().map(|px| px[3])
}

/// Flou de boîte en trois passes sur le canal alpha, rayon `r` pixels ; les canaux couleur reprennent l'alpha (blanc
/// prémultiplié).
fn blur_alpha(pixmap: &mut Pixmap, r: f32) {
    let (w, h) = (pixmap.width() as usize, pixmap.height() as usize);
    let radius = (r * 0.9).round().max(1.0) as usize;
    let mut a: Vec<f32> = alpha_of(pixmap.data()).map(f32::from).collect();
    let mut tmp = vec![0.0_f32; a.len()];
    for _ in 0..3 {
        box_pass(&a, &mut tmp, w, h, radius, true);
        box_pass(&tmp, &mut a, w, h, radius, false);
    }
    for (px, v) in pixmap.data_mut().as_chunks_mut::<4>().0.iter_mut().zip(&a) {
        let v = v.round().clamp(0.0, 255.0) as u8;
        px.copy_from_slice(&[v, v, v, v]);
    }
}

/// Une passe de moyenne glissante, horizontale (`horizontal`) ou verticale.
fn box_pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len, stride, step) = if horizontal { (h, w, w, 1) } else { (w, h, 1, w) };
    let norm = 1.0 / (2 * r + 1) as f32;
    for line in 0..lines {
        let base = line * stride;
        let at = |i: isize| -> f32 {
            if i < 0 || i >= len as isize {
                0.0
            } else {
                src[base + i as usize * step]
            }
        };
        let mut sum: f32 = (-(r as isize)..=r as isize).map(at).sum();
        for i in 0..len {
            dst[base + i * step] = sum * norm;
            sum += at(i as isize + r as isize + 1) - at(i as isize - r as isize);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_rectangle_remplit_son_canal_et_pas_les_autres() {
        let Some(mut c) = Canvas::new(32, 32) else { return };
        c.rect(Ink::new(G), 8.0, 8.0, 16.0, 16.0);
        let rgba = c.pack_rgba();
        let px = |x: usize, y: usize| &rgba[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(px(16, 16), &[0, 255, 0, 0]);
        assert_eq!(px(2, 2), &[0, 0, 0, 0]);
    }

    #[test]
    fn le_halo_deborde_de_la_forme_sans_la_depasser_en_intensite() {
        let Some(mut c) = Canvas::new(64, 64) else { return };
        c.circle(Ink::new(R).glow(12.0), 32.0, 32.0, 6.0);
        let r8 = c.pack_r8();
        assert_eq!(r8[32 * 64 + 32], 255);
        let outside = r8[32 * 64 + 32 + 12];
        assert!(outside > 0 && outside < 255, "halo = {outside}");
    }

    #[test]
    fn effacer_un_rectangle_vide_toutes_les_couches() {
        let Some(mut c) = Canvas::new(16, 16) else { return };
        c.rect(Ink::new(K), 0.0, 0.0, 16.0, 16.0);
        c.clear_rect(4.0, 4.0, 8.0, 8.0);
        let rgba = c.pack_rgba();
        assert_eq!(rgba[(8 * 16 + 8) * 4 + 3], 0);
        assert_eq!(rgba[3], 255);
    }
}
