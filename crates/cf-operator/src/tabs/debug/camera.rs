//! La colonne « Caméra » : tous les plans du réalisateur, à côté des quatre colonnes d'événements.
//!
//! Elle tient sur toute la hauteur de la grille pour que l'opérateur voie en un coup d'œil quel plan est à l'antenne et
//! puisse en forcer un. Choisir un plan y coupe et le garde ; « Auto » rend la main au réalisateur. Aucun mode n'est
//! touché : le mode manuel reste ce qu'il était. Les plans portent leur nom en clair et leur position (« DANS la
//! voiture ») : c'est ainsi qu'on repère la caméra mal placée.

use bevy_egui::egui::{self, Rect};
use cf_cinema::ShotOrder;
use cf_control::Action;

use super::chip::push_order;
use super::rows::{Row, draw_row};
use super::shot_names::{shot_name, shot_title};
use super::{DebugView, Gesture};

/// Largeur de la colonne (px).
pub(crate) const CAMERA_WIDTH: f32 = 200.0;
/// Teinte des plans caméra.
const SHOT_COLOR: egui::Color32 = egui::Color32::from_rgb(0xb0, 0x8c, 0xff);
/// Marge intérieure (px).
const INNER_MARGIN: f32 = 6.0;

/// Les ordres caméra qui existent, dans l'ordre des boutons : « Plan suivant » ici, « Enseigne DJ » dans la colonne des
/// panneaux.
pub(crate) fn camera_actions() -> [Action; 2] {
    [Action::SkipShot, Action::ShowDj]
}

/// Sépare la grille : les colonnes d'événements à gauche, la colonne caméra à droite. Une fenêtre étroite laisse sa place
/// aux colonnes d'événements.
pub(crate) fn split_camera(rect: Rect, width: f32, gap: f32) -> (Rect, Rect) {
    let width = width.min((rect.width() * 0.3).max(0.0));
    let left = (rect.right() - width).max(rect.left());
    let events = Rect::from_min_max(rect.min, egui::pos2((left - gap).max(rect.left()), rect.bottom()));
    let camera = Rect::from_min_max(egui::pos2(left, rect.top()), rect.max);
    (events, camera)
}

/// La colonne.
pub(crate) fn draw_camera_column(ui: &mut egui::Ui, rect: Rect, view: &mut DebugView<'_>) {
    let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().rect_stroke(rect, 6.0, stroke, egui::StrokeKind::Inside);
    super::grid::in_rect(ui, rect.shrink(INNER_MARGIN), |ui| {
        let on_air = view.director.and_then(cf_cinema::CinemaDirector::shot_id);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Caméra").strong().color(SHOT_COLOR));
            if ui
                .small_button("Plan suivant")
                .on_hover_text("Le réalisateur coupe vers un autre plan (comme la touche N).")
                .clicked()
            {
                push_order(view.orders, Action::SkipShot);
            }
        });
        match on_air {
            Some(id) => ui.label(egui::RichText::new(format!("À l'antenne : {}", shot_title(id))).small()),
            None => ui.weak("Aucun plan à l'antenne."),
        };
        let Some(director) = view.director else {
            ui.weak("Réalisateur absent (monde absent).");
            return;
        };
        let held = director.held_shot();
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        egui::ScrollArea::vertical()
            .id_salt("debug-shots")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let auto = Row {
                    detail: "Le réalisateur enchaîne les plans",
                    selected: held.is_none(),
                    ..Row::new("Auto", SHOT_COLOR)
                };
                if draw_row(ui, &auto).clicked() && held.is_some() {
                    view.state.gestures.push(Gesture::Shot(ShotOrder::Release));
                }
                for info in director.shot_infos() {
                    let detail = format!("{} · {:.0} à {:.0} s", info.id, info.min_s, info.max_s);
                    let row = Row {
                        detail: &detail,
                        badge: if on_air == Some(info.id) { "antenne" } else { "" },
                        selected: held == Some(info.id),
                        running: on_air == Some(info.id),
                        ..Row::new(shot_name(info.id), SHOT_COLOR)
                    };
                    if draw_row(ui, &row).clicked() {
                        view.state.gestures.push(Gesture::Shot(ShotOrder::Hold(info.id)));
                    }
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_colonne_camera_prend_sa_largeur_a_droite() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 500.0));
        let (events, camera) = split_camera(rect, CAMERA_WIDTH, 10.0);
        assert!((camera.width() - CAMERA_WIDTH).abs() < 1e-3);
        assert!((camera.right() - rect.right()).abs() < 1e-3);
        assert!(
            (events.right() + 10.0 - camera.left()).abs() < 1e-3,
            "un écart entre les deux"
        );
        assert!((events.height() - 500.0).abs() < 1e-3 && (camera.height() - 500.0).abs() < 1e-3);
    }

    #[test]
    fn une_fenetre_etroite_ne_mange_pas_les_colonnes_d_evenements() {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
        let (events, camera) = split_camera(rect, CAMERA_WIDTH, 10.0);
        assert!(camera.width() <= 120.0 + 1e-3);
        assert!(events.width() > camera.width());
        let tiny = split_camera(
            Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(0.0, 0.0)),
            CAMERA_WIDTH,
            10.0,
        );
        assert!(tiny.0.width() >= 0.0 && tiny.1.width() >= 0.0);
    }
}
