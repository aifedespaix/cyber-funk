//! La grille de l'onglet Debug : quatre colonnes égales (Panneaux, Immeubles, Voitures, Quartiers) à gauche du dock.
//!
//! La grille prend toute la hauteur disponible. Dans chaque colonne, sous un en-tête de hauteur fixe, la hauteur
//! restante est partagée **2/3 puces, 1/3 « En cours »** (l'en-tête du couloir garde lui aussi sa hauteur), chaque
//! partie avec son propre défilement. Les rectangles sont calculés à la main ([`split_columns`], [`split_column`]) :
//! c'est la seule façon d'imposer ce partage à egui, et ces fonctions pures se testent sans fenêtre.

use bevy_egui::egui::{self, Rect};
use cf_render::{ActiveEvents, EventCategory};

use super::chip::{ChipClick, chip_run, draw_chip, fire};
use super::columns::{COLUMNS, column_color, column_index, column_title};
use super::districts::{district_orders, draw_district_column};
use super::lane::{draw_lane, draw_lane_header};
use super::{DebugState, DebugView, Gesture};

/// Écart entre deux colonnes, et entre la grille et le dock (px).
pub(crate) const GAP: f32 = 10.0;
/// Hauteur de l'en-tête d'une colonne (px).
const HEADER_HEIGHT: f32 = 24.0;
/// Hauteur de l'en-tête du couloir « En cours » (px).
const LANE_HEADER_HEIGHT: f32 = 24.0;
/// Largeur d'en-tête (px) à partir de laquelle le compteur dit « N actifs » plutôt que « N ».
const WIDE_HEADER: f32 = 150.0;
/// Marge intérieure d'une colonne (px).
const INNER_MARGIN: f32 = 6.0;

/// Les parties d'une colonne, de haut en bas.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColumnRects {
    /// Titre et nombre d'actifs.
    pub header: Rect,
    /// Puces (ou commandes des quartiers), 2/3 de la hauteur restante.
    pub chips: Rect,
    /// « En cours » et Stop.
    pub lane_header: Rect,
    /// Couloir, 1/3 de la hauteur restante.
    pub lane: Rect,
}

/// Quatre colonnes de même largeur, séparées de `gap`, sur toute la hauteur de `rect`.
pub(crate) fn split_columns(rect: Rect, gap: f32) -> [Rect; 4] {
    let n = COLUMNS.len() as f32;
    let width = ((rect.width() - gap * (n - 1.0)) / n).max(0.0);
    std::array::from_fn(|i| {
        let left = rect.left() + i as f32 * (width + gap);
        Rect::from_min_max(egui::pos2(left, rect.top()), egui::pos2(left + width, rect.bottom()))
    })
}

/// Le partage d'une colonne : en-têtes de hauteur fixe, puis 2/3 puces et 1/3 couloir. Jamais de hauteur négative.
pub(crate) fn split_column(rect: Rect, header_height: f32, lane_header_height: f32) -> ColumnRects {
    let band = |top: f32, height: f32| {
        let top = top.min(rect.bottom());
        Rect::from_min_max(
            egui::pos2(rect.left(), top),
            egui::pos2(rect.right(), (top + height.max(0.0)).min(rect.bottom())),
        )
    };
    let free = (rect.height() - header_height - lane_header_height).max(0.0);
    let header = band(rect.top(), header_height);
    let chips = band(header.bottom(), free * 2.0 / 3.0);
    let lane_header = band(chips.bottom(), lane_header_height);
    let lane = band(lane_header.bottom(), rect.bottom() - lane_header.bottom());
    ColumnRects {
        header,
        chips,
        lane_header,
        lane,
    }
}

/// La grille à gauche, le dock (largeur fixe) à droite.
pub(crate) fn split_dock(rect: Rect, dock_width: f32, gap: f32) -> (Rect, Rect) {
    let dock_left = (rect.right() - dock_width).max(rect.left());
    let grid = Rect::from_min_max(rect.min, egui::pos2((dock_left - gap).max(rect.left()), rect.bottom()));
    let dock = Rect::from_min_max(egui::pos2(dock_left, rect.top()), rect.max);
    (grid, dock)
}

/// Dessine dans ce rectangle, de haut en bas.
pub(crate) fn in_rect<R>(ui: &mut egui::Ui, rect: Rect, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
        add,
    )
    .inner
}

/// La grille.
pub(crate) fn draw_grid(ui: &mut egui::Ui, rect: Rect, view: &mut DebugView<'_>) {
    for (column, column_rect) in COLUMNS.into_iter().zip(split_columns(rect, GAP)) {
        draw_column(ui, column, column_rect, view);
    }
}

fn draw_column(ui: &mut egui::Ui, column: EventCategory, rect: Rect, view: &mut DebugView<'_>) {
    let color = column_color(column);
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
    let parts = split_column(rect.shrink(INNER_MARGIN), HEADER_HEIGHT, LANE_HEADER_HEIGHT);
    let running = view.active.map_or(0, |active| active.in_category(column).count());
    in_rect(ui, parts.header, |ui| {
        // Le compteur d'abord, à droite (« actifs » seulement s'il y a la place), puis le titre tronqué dans le reste :
        // sur une fenêtre étroite (640 px, colonnes de ~80 px) rien ne se chevauche.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if parts.header.width() >= WIDE_HEADER {
                ui.weak(format!("{running} actifs"));
            } else {
                ui.weak(running.to_string()).on_hover_text("événements en cours");
            }
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.add(egui::Label::new(egui::RichText::new(column_title(column)).strong().color(color)).truncate());
            });
        });
    });
    let index = column_index(column);
    in_rect(ui, parts.chips, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(("debug-chips", index))
            .auto_shrink([false, false])
            .max_height(parts.chips.height())
            .show(ui, |ui| {
                if column == EventCategory::District {
                    draw_districts(ui, view);
                } else {
                    draw_chips(ui, column, view);
                }
            });
    });
    let DebugState { gestures, .. } = &mut *view.state;
    in_rect(ui, parts.lane_header, |ui| draw_lane_header(ui, column, gestures));
    // Une transition de quartier n'a pas de durée au registre : sa barre suit l'avancement du moteur.
    let indeterminate = (column == EventCategory::District)
        .then(|| view.district.and_then(|s| s.target.map(|_| s.progress)))
        .flatten();
    in_rect(ui, parts.lane, |ui| {
        egui::ScrollArea::vertical()
            .id_salt(("debug-lane", index))
            .auto_shrink([false, false])
            .max_height(parts.lane.height())
            .show(ui, |ui| {
                draw_lane(ui, view.active, column, view.now_s, color, indeterminate, gestures);
            });
    });
}

/// Les puces d'une colonne, rangées par le plan de l'image.
fn draw_chips(ui: &mut egui::Ui, column: EventCategory, view: &mut DebugView<'_>) {
    let Some(catalog) = view.catalog else {
        ui.weak("Aucun événement inscrit (monde absent).");
        return;
    };
    let pinned = view.pinned();
    let empty = ActiveEvents::default();
    let active = view.active.unwrap_or(&empty);
    let DebugState { plan, gestures, .. } = &mut *view.state;
    let mut any = false;
    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
    for event in plan.column(catalog, column) {
        any = true;
        let run = chip_run(event.id, active, view.now_s);
        match draw_chip(ui, event, pinned.contains(event.id), run, column_color(column)) {
            Some(ChipClick::Fire) => {
                fire(view.orders, event);
            }
            Some(ChipClick::TogglePin) => gestures.push(Gesture::TogglePin(event.id)),
            None => {}
        }
    }
    if !any {
        ui.weak("Aucun événement (voir la recherche).");
    }
}

/// Les commandes des quartiers, traduites en ordres de l'image.
fn draw_districts(ui: &mut egui::Ui, view: &mut DebugView<'_>) {
    let DebugState { district, gestures, .. } = &mut *view.state;
    let pick = draw_district_column(ui, view.district, view.library, district);
    // La coque ne dessine jamais l'onglet en Live ; `district_orders` refuse en plus tout ordre hors Préparation.
    gestures.extend(district_orders(view.mode, district, &pick).map(Gesture::District));
}
