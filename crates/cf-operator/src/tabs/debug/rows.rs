//! Une ligne de liste : un nom, un détail en dessous, un badge à droite. Sert aux événements, aux quartiers, aux
//! transitions et aux plans caméra, pour que tout l'onglet se lise (et défile) de la même façon.

use bevy_egui::egui;

/// Hauteur d'une ligne à deux textes (px) : cible cliquable confortable.
const TWO_LINES: f32 = 38.0;
/// Hauteur d'une ligne sans détail (px).
const ONE_LINE: f32 = 26.0;
/// Marge gauche du texte (px).
const PAD: f32 = 8.0;

/// Ce qu'une ligne affiche.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Row<'a> {
    /// Nom (en gras).
    pub title: &'a str,
    /// Détail, plus petit et estompé ; vide pour une ligne simple.
    pub detail: &'a str,
    /// Texte à droite (`×2`, `courant`, `★`…).
    pub badge: &'a str,
    /// La ligne est le choix en cours.
    pub selected: bool,
    /// Nom barré (fonction pas encore implémentée).
    pub strike: bool,
    /// Teinte du contour quand la ligne est active ou choisie.
    pub color: egui::Color32,
    /// Remplissage de gauche à droite, 0..=1 (avancement d'un événement en cours).
    pub progress: Option<f32>,
    /// Quelque chose tourne : le contour prend la teinte.
    pub running: bool,
}

impl<'a> Row<'a> {
    /// Une ligne nue.
    pub(crate) fn new(title: &'a str, color: egui::Color32) -> Self {
        Self {
            title,
            detail: "",
            badge: "",
            selected: false,
            strike: false,
            color,
            progress: None,
            running: false,
        }
    }
}

/// Dessine la ligne sur toute la largeur disponible. Le texte est rogné à la ligne plutôt que de la déborder.
pub(crate) fn draw_row(ui: &mut egui::Ui, row: &Row<'_>) -> egui::Response {
    let height = if row.detail.is_empty() { ONE_LINE } else { TWO_LINES };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let visuals = ui.visuals();
    let radius = visuals.widgets.inactive.corner_radius;
    let fill = if row.selected {
        row.color.gamma_multiply(0.22)
    } else if response.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        visuals.widgets.inactive.weak_bg_fill
    };
    let stroke = if row.selected || row.running {
        egui::Stroke::new(1.0, row.color)
    } else {
        visuals.widgets.inactive.bg_stroke
    };
    let text_color = visuals.text_color();
    let weak_color = visuals.weak_text_color();
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, radius, fill);
    if let Some(progress) = row.progress {
        let mut filled = rect;
        filled.set_width(rect.width() * progress.clamp(0.0, 1.0));
        painter.rect_filled(filled, radius, row.color.gamma_multiply(0.35));
    }
    painter.rect_stroke(rect, radius, stroke, egui::StrokeKind::Inside);

    // Le badge d'abord : il réserve sa largeur, le titre se rogne contre lui.
    let mut right = rect.right() - PAD;
    if !row.badge.is_empty() {
        let galley = painter.layout_no_wrap(row.badge.to_owned(), egui::FontId::proportional(11.0), weak_color);
        right -= galley.size().x;
        let y = rect.top() + (ONE_LINE - galley.size().y) / 2.0;
        painter.galley(egui::pos2(right, y), galley, weak_color);
        right -= PAD;
    }
    let clip = egui::Rect::from_min_max(rect.min, egui::pos2(right.max(rect.left()), rect.bottom()));
    let text_painter = painter.with_clip_rect(clip);
    let title_y = if row.detail.is_empty() {
        rect.center().y
    } else {
        rect.top() + 12.0
    };
    text_painter.text(
        egui::pos2(rect.left() + PAD, title_y),
        egui::Align2::LEFT_CENTER,
        row.title,
        egui::FontId::proportional(13.0),
        text_color,
    );
    if row.strike {
        // `painter.text` ignore le style de `RichText` : le trait est tiré à la main.
        let width = painter
            .layout_no_wrap(row.title.to_owned(), egui::FontId::proportional(13.0), text_color)
            .size()
            .x;
        text_painter.hline(
            rect.left() + PAD..=rect.left() + PAD + width,
            title_y,
            egui::Stroke::new(1.0, text_color),
        );
    }
    if !row.detail.is_empty() {
        text_painter.text(
            egui::pos2(rect.left() + PAD, rect.top() + 28.0),
            egui::Align2::LEFT_CENTER,
            row.detail,
            egui::FontId::proportional(10.5),
            weak_color,
        );
    }
    response
}
